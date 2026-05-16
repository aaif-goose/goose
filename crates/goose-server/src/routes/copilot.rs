//! Goose Copilot review endpoint.
//!
//! Receives a per-PR review request from the switchboard, clones the user's
//! repo to a temp directory using the GitHub App installation token, invokes
//! `goose review --range BASE...HEAD` as a subprocess (parallel orchestrator
//! lives in goose-cli), parses the JSONL findings, and posts them back to
//! GitHub as inline review comments + a check-run conclusion.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use axum::{extract::State, response::Json, routing::post, Router};
use goose::config::signup_copilot::CopilotInstallFlow;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use utoipa::ToSchema;

use crate::routes::errors::ErrorResponse;
use crate::state::AppState;

const SWITCHBOARD_URL: &str = "https://goose-copilot-switchboard.example.workers.dev";
const SWITCHBOARD_URL_ENV: &str = "GOOSE_COPILOT_SWITCHBOARD_URL";
const USER_AGENT: &str = "goose-copilot/0.1";

#[derive(Debug, Serialize, ToSchema)]
pub struct CopilotSetupResponse {
    pub installation_id: u64,
}

#[utoipa::path(
    post,
    path = "/copilot/setup",
    responses(
        (status = 200, description = "Goose Copilot connected", body = CopilotSetupResponse),
        (status = 408, description = "Install timed out"),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn setup(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CopilotSetupResponse>, ErrorResponse> {
    let oauth_client_id = fetch_oauth_client_id()
        .await
        .map_err(|e| ErrorResponse::internal(format!("oauth-config lookup failed: {e}")))?;

    let mut flow = CopilotInstallFlow::new().with_oauth_client_id(oauth_client_id);
    let callback = flow
        .complete_flow()
        .await
        .map_err(|e| ErrorResponse::internal(e.to_string()))?;

    let tunnel_info = state.tunnel_manager.get_info().await;
    if tunnel_info.state != crate::tunnel::TunnelState::Running {
        state
            .tunnel_manager
            .start()
            .await
            .map_err(|e| ErrorResponse::internal(format!("tunnel start failed: {e}")))?;
    }
    let tunnel_info = state.tunnel_manager.get_info().await;
    let agent_id = extract_agent_id(&tunnel_info.url)
        .ok_or_else(|| ErrorResponse::internal("tunnel URL is missing the agent id".to_string()))?;

    let body = serde_json::json!({
        "oauth_code": callback.oauth_code,
        "agent_id": agent_id,
        "tunnel_secret": tunnel_info.secret,
        "tunnel_url": tunnel_info.url,
    });
    let res = Client::new()
        .post(format!("{}/copilot/register", switchboard_url()))
        .json(&body)
        .send()
        .await
        .map_err(|e| ErrorResponse::internal(format!("switchboard unreachable: {e}")))?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        return Err(ErrorResponse::internal(format!(
            "switchboard rejected registration: {status} {detail}"
        )));
    }

    #[derive(Deserialize)]
    struct RegisterResponse {
        installation_id: u64,
    }
    let register: RegisterResponse = res
        .json()
        .await
        .map_err(|e| ErrorResponse::internal(format!("parse register response: {e}")))?;

    Ok(Json(CopilotSetupResponse {
        installation_id: register.installation_id,
    }))
}

fn switchboard_url() -> String {
    env::var(SWITCHBOARD_URL_ENV).unwrap_or_else(|_| SWITCHBOARD_URL.to_string())
}

/// Fetch the public OAuth client ID from the switchboard. The client *id* is
/// public — only the client *secret* stays on the worker.
async fn fetch_oauth_client_id() -> Result<String> {
    #[derive(Deserialize)]
    struct OAuthConfig {
        oauth_client_id: String,
    }
    let res = Client::new()
        .get(format!("{}/copilot/oauth-config", switchboard_url()))
        .send()
        .await
        .context("switchboard unreachable")?;
    if !res.status().is_success() {
        bail!("switchboard returned {}", res.status());
    }
    let cfg: OAuthConfig = res.json().await.context("parse oauth-config")?;
    if cfg.oauth_client_id.trim().is_empty() {
        bail!("switchboard returned an empty oauth_client_id");
    }
    Ok(cfg.oauth_client_id)
}

fn extract_agent_id(tunnel_url: &str) -> Option<String> {
    tunnel_url
        .rsplit_once("/tunnel/")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or("").to_string())
}

#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct CopilotReviewRequest {
    /// Short-lived GitHub App installation token, scoped to the user's repos.
    pub github_token: String,
    /// `owner/repo` form, e.g. `block/goose`.
    pub repo: String,
    pub pr_number: u64,
    pub head_sha: String,
    pub pr_url: String,
    /// The endpoint updates this Check Run on completion.
    #[serde(default)]
    pub check_run_id: Option<u64>,
    /// Issue-comment id when the review was triggered via `@goose-copilot review`.
    /// goosed reacts on this comment when done.
    #[serde(default)]
    pub comment_id: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CopilotReviewResponse {
    pub accepted: bool,
}

#[utoipa::path(
    post,
    path = "/copilot/review",
    request_body = CopilotReviewRequest,
    responses(
        (status = 200, description = "Review accepted, running in background", body = CopilotReviewResponse),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn review(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CopilotReviewRequest>,
) -> Result<Json<CopilotReviewResponse>, ErrorResponse> {
    let pr_label = format!("{} #{}", req.repo, req.pr_number);
    tokio::spawn(async move {
        let result = run_review(req.clone()).await;
        if let Err(e) = &result {
            tracing::error!("[copilot] review {} failed: {:#}", pr_label, e);
        }
        if let Some(id) = req.comment_id {
            let reaction = if result.is_ok() { "+1" } else { "confused" };
            if let Err(e) = post_comment_reaction(&req.repo, id, reaction, &req.github_token).await
            {
                tracing::warn!("[copilot] review reaction failed: {:#}", e);
            }
        }
    });
    Ok(Json(CopilotReviewResponse { accepted: true }))
}

/// One review finding emitted by `goose review` as JSONL. Matches the shape
/// of `goose_cli::commands::review::orchestrator::Finding`.
#[derive(Debug, Clone, Deserialize)]
struct Finding {
    severity: String,
    path: String,
    #[serde(default)]
    line_start: i64,
    #[serde(default)]
    line_end: i64,
    summary: String,
    #[serde(default)]
    check: String,
    /// Optional concrete fix the orchestrator captured. Rendered as a
    /// GitHub ```suggestion block in the posted review comment.
    #[serde(default)]
    suggestion: Option<String>,
}

async fn run_review(req: CopilotReviewRequest) -> Result<()> {
    let pr = fetch_pr_metadata(&req).await?;
    let workdir = tempfile::tempdir().context("create temp workdir")?;
    git_clone_and_checkout(&req, workdir.path()).await?;
    let findings = run_goose_review(workdir.path(), &pr.base_sha, &req.head_sha).await?;
    post_review(&req, &findings).await?;
    if let Some(crid) = req.check_run_id {
        complete_check_run(&req, crid, &findings).await?;
    }
    Ok(())
}

struct PrMetadata {
    base_sha: String,
}

async fn fetch_pr_metadata(req: &CopilotReviewRequest) -> Result<PrMetadata> {
    #[derive(Deserialize)]
    struct PrResponse {
        base: BaseRef,
    }
    #[derive(Deserialize)]
    struct BaseRef {
        sha: String,
    }

    let url = format!(
        "https://api.github.com/repos/{}/pulls/{}",
        req.repo, req.pr_number
    );
    let res = github_client()
        .get(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .send()
        .await
        .context("fetch PR metadata")?;
    if !res.status().is_success() {
        bail!(
            "fetch PR metadata: {} — {}",
            res.status(),
            res.text().await.unwrap_or_default()
        );
    }
    let pr: PrResponse = res.json().await.context("parse PR metadata")?;
    Ok(PrMetadata {
        base_sha: pr.base.sha,
    })
}

async fn git_clone_and_checkout(req: &CopilotReviewRequest, dest: &Path) -> Result<()> {
    let url = format!(
        "https://x-access-token:{}@github.com/{}",
        req.github_token, req.repo
    );
    git_run(dest, &["clone", "--quiet", "--no-tags", &url, "."]).await?;
    let refspec = format!("+refs/pull/{}/head:refs/copilot/pr", req.pr_number);
    git_run(dest, &["fetch", "--quiet", "origin", &refspec]).await?;
    git_run(dest, &["checkout", "--quiet", "refs/copilot/pr"]).await?;
    Ok(())
}

async fn git_run(cwd: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .context(format!("invoke git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

async fn run_goose_review(workdir: &Path, base: &str, head: &str) -> Result<Vec<Finding>> {
    let bin = locate_goose_binary()?;
    let range = format!("{base}...{head}");

    let output = Command::new(&bin)
        .current_dir(workdir)
        .args(["review", &range, "--severity", "medium", "--quiet"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context(format!("invoke {} review", bin.display()))?;

    if !output.status.success() {
        bail!(
            "goose review exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| anyhow!("goose review stdout was not UTF-8: {e}"))?;
    parse_findings(&stdout)
}

fn parse_findings(stdout: &str) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    for (idx, line) in stdout.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let f: Finding = serde_json::from_str(trimmed)
            .with_context(|| format!("parse finding on line {}", idx + 1))?;
        out.push(f);
    }
    Ok(out)
}

/// Resolve the `goose` binary path. Prefer the binary co-located with the
/// running `goosed` (typical Desktop install layout), then fall back to a
/// `PATH` lookup so dev environments without a co-located binary still work.
fn locate_goose_binary() -> Result<PathBuf> {
    if let Ok(current) = env::current_exe() {
        if let Some(parent) = current.parent() {
            let candidate = parent.join("goose");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Ok(PathBuf::from("goose"))
}

async fn post_review(req: &CopilotReviewRequest, findings: &[Finding]) -> Result<()> {
    let url = format!(
        "https://api.github.com/repos/{}/pulls/{}/reviews",
        req.repo, req.pr_number
    );
    let payload = build_review_payload(req, findings, false);

    let res = github_client()
        .post(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .json(&payload)
        .send()
        .await
        .context("POST PR review")?;

    if res.status().is_success() {
        return Ok(());
    }

    // 422 typically means an inline comment targeted a line not in the diff.
    // Fall back to summary-only so the review still lands.
    let status = res.status();
    let detail = res.text().await.unwrap_or_default();
    tracing::warn!(
        "[copilot] inline review rejected ({status}): {} — falling back to summary-only",
        detail.chars().take(300).collect::<String>()
    );

    let fallback = build_review_payload(req, findings, true);
    github_client()
        .post(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .json(&fallback)
        .send()
        .await
        .context("POST fallback summary-only review")?
        .error_for_status()
        .context("fallback summary-only review rejected")?;
    Ok(())
}

fn build_review_payload(
    req: &CopilotReviewRequest,
    findings: &[Finding],
    summary_only: bool,
) -> serde_json::Value {
    let comments: Vec<serde_json::Value> = if summary_only {
        Vec::new()
    } else {
        findings
            .iter()
            .filter(|f| !f.path.is_empty())
            .map(|f| {
                let line = if f.line_end > 0 {
                    f.line_end
                } else {
                    f.line_start
                };
                let line = line.max(1);
                let mut body = format!(
                    "**{}** ({}) — {}",
                    f.severity.to_ascii_uppercase(),
                    if f.check.is_empty() { "main" } else { &f.check },
                    f.summary
                );
                if let Some(code) = f
                    .suggestion
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    body.push_str("\n\n```suggestion\n");
                    body.push_str(code);
                    body.push_str("\n```");
                }
                serde_json::json!({
                    "path": f.path,
                    "line": line,
                    "side": "RIGHT",
                    "body": body,
                })
            })
            .collect()
    };

    let mut summary = format!(
        "**[goose Copilot]({})** — {} finding(s)",
        req.pr_url,
        findings.len()
    );
    if !findings.is_empty() {
        summary.push_str(":\n\n");
        for f in findings.iter().take(10) {
            summary.push_str(&format!(
                "- **{}** in `{}`: {}\n",
                f.severity, f.path, f.summary
            ));
        }
        if findings.len() > 10 {
            summary.push_str(&format!("…and {} more.\n", findings.len() - 10));
        }
    }
    if summary_only {
        summary.push_str("\n_(inline comments were rejected by GitHub; posted summary only.)_");
    }

    serde_json::json!({
        "commit_id": req.head_sha,
        "body": summary,
        "event": "COMMENT",
        "comments": comments,
    })
}

async fn complete_check_run(
    req: &CopilotReviewRequest,
    check_run_id: u64,
    findings: &[Finding],
) -> Result<()> {
    let title = if findings.is_empty() {
        "No blocking issues".to_string()
    } else {
        format!("Reviewed — {} finding(s)", findings.len())
    };
    let url = format!(
        "https://api.github.com/repos/{}/check-runs/{}",
        req.repo, check_run_id
    );
    github_client()
        .patch(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .json(&serde_json::json!({
            "status": "completed",
            "conclusion": "success",
            "output": {
                "title": title,
                "summary": "goose Copilot review complete.",
            },
        }))
        .send()
        .await
        .context("PATCH check run")?
        .error_for_status()
        .context("PATCH check run rejected")?;
    Ok(())
}

fn github_client() -> Client {
    Client::builder()
        .user_agent(USER_AGENT)
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(
                reqwest::header::ACCEPT,
                "application/vnd.github+json".parse().unwrap(),
            );
            h.insert("X-GitHub-Api-Version", "2022-11-28".parse().unwrap());
            h
        })
        .build()
        .expect("build reqwest client")
}

#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct CopilotCommentRequest {
    pub github_token: String,
    pub repo: String,
    pub pr_number: u64,
    pub pr_url: String,
    pub comment_body: String,
    pub commenter: String,
    /// The PR's head branch name (e.g. `feature/foo`). Used to push any
    /// edits the agent makes back to the PR.
    #[serde(default)]
    pub head_ref: String,
    /// Issue-comment id we're replying to. goosed reacts on this comment
    /// (`+1` on success, `confused` on failure) when done.
    #[serde(default)]
    pub comment_id: Option<u64>,
}

#[utoipa::path(
    post,
    path = "/copilot/comment",
    request_body = CopilotCommentRequest,
    responses(
        (status = 200, description = "Comment accepted, replying in background", body = CopilotReviewResponse),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn comment(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CopilotCommentRequest>,
) -> Result<Json<CopilotReviewResponse>, ErrorResponse> {
    let pr_label = format!("{} #{}", req.repo, req.pr_number);
    tokio::spawn(async move {
        let result = run_comment_reply(req.clone()).await;
        if let Err(e) = &result {
            tracing::error!("[copilot] comment {} failed: {:#}", pr_label, e);
        }
        if let Some(id) = req.comment_id {
            let reaction = if result.is_ok() { "+1" } else { "confused" };
            if let Err(e) = post_comment_reaction(&req.repo, id, reaction, &req.github_token).await
            {
                tracing::warn!("[copilot] comment reaction failed: {:#}", e);
            }
        }
    });
    Ok(Json(CopilotReviewResponse { accepted: true }))
}

async fn run_comment_reply(req: CopilotCommentRequest) -> Result<()> {
    let workdir = tempfile::tempdir().context("create temp workdir")?;
    git_clone_and_checkout_for_comment(&req, workdir.path()).await?;
    let prompt = build_comment_prompt(&req);
    let reply = run_goose_for_reply(workdir.path(), &prompt).await?;
    let pushed = commit_and_push_if_changed(&req, workdir.path()).await?;
    let final_reply = match pushed {
        Some(n) => format!(
            "{reply}\n\n_Pushed {n} file change(s) to `{}`._",
            req.head_ref
        ),
        None => reply,
    };
    post_pr_comment(&req, &final_reply).await?;
    Ok(())
}

async fn git_clone_and_checkout_for_comment(
    req: &CopilotCommentRequest,
    dest: &Path,
) -> Result<()> {
    let url = format!(
        "https://x-access-token:{}@github.com/{}",
        req.github_token, req.repo
    );
    git_run(dest, &["clone", "--quiet", "--no-tags", &url, "."]).await?;
    let refspec = format!("+refs/pull/{}/head:refs/copilot/pr", req.pr_number);
    git_run(dest, &["fetch", "--quiet", "origin", &refspec]).await?;
    git_run(dest, &["checkout", "--quiet", "refs/copilot/pr"]).await?;
    Ok(())
}

fn build_comment_prompt(req: &CopilotCommentRequest) -> String {
    // Strip the @goose-copilot mention so the model sees the user's actual ask.
    let cleaned = req
        .comment_body
        .replace("@goose-copilot", "")
        .trim()
        .to_string();
    format!(
        "You are responding to a comment on a GitHub pull request.\n\
\n\
Repository: {}\n\
Pull request: #{} ({})\n\
Commenter: @{}\n\
\n\
The repository is checked out at the current working directory at the\n\
PR's head commit. You can read AND modify files, run shell commands,\n\
and use any available tools.\n\
\n\
If the commenter asks you to fix, address, or apply changes:\n\
- Make the edits directly with your file-editing tools.\n\
- Any files you change will be automatically committed and pushed to\n\
  the PR branch after you finish — do NOT run `git commit` or `git push`\n\
  yourself.\n\
- Then reply with a short summary of what you changed.\n\
\n\
If the commenter is asking a question or for analysis only:\n\
- Don't modify any files.\n\
- Reply with a concise answer, referencing files/lines when relevant.\n\
\n\
The commenter's message (with @goose-copilot stripped):\n\
---\n\
{}\n\
---\n\
\n\
Reply with a single concise GitHub-flavored markdown response. Do NOT\n\
include a preamble like \"Sure, here's my response.\" Just answer.\n",
        req.repo, req.pr_number, req.pr_url, req.commenter, cleaned,
    )
}

async fn run_goose_for_reply(workdir: &Path, prompt: &str) -> Result<String> {
    let bin = locate_goose_binary()?;
    let output = Command::new(&bin)
        .current_dir(workdir)
        .args([
            "run",
            "--no-session",
            "--quiet",
            "--output-format",
            "json",
            "-t",
            prompt,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context(format!("invoke {} run", bin.display()))?;
    if !output.status.success() {
        bail!(
            "goose run exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| anyhow!("goose run stdout was not UTF-8: {e}"))?;
    extract_final_assistant_text(&stdout)
}

/// Pull the last assistant message's plain text out of `goose run --output-format json`.
/// The shape is `{ "messages": [{ "role": "user|assistant|...", "content": [...] }, ...] }`;
/// each content entry is `{ "type": "text", "text": "..." }` or a tool-call variant we ignore.
fn extract_final_assistant_text(stdout: &str) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct RunOutput {
        messages: Vec<RunMessage>,
    }
    #[derive(serde::Deserialize)]
    struct RunMessage {
        role: String,
        #[serde(default)]
        content: Vec<serde_json::Value>,
    }

    let parsed: RunOutput =
        serde_json::from_str(stdout).context("parse goose run --output-format json")?;
    let last = parsed
        .messages
        .iter()
        .rev()
        .find(|m| m.role.eq_ignore_ascii_case("assistant"))
        .ok_or_else(|| anyhow!("no assistant message in goose run output"))?;
    let text: String = last
        .content
        .iter()
        .filter_map(|c| c.get("text").and_then(|t| t.as_str()).map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.trim().is_empty() {
        bail!("assistant message had no text content");
    }
    Ok(text.trim().to_string())
}

/// React on an issue comment (`eyes`, `+1`, `-1`, `confused`, …). 200 means
/// the bot already had that reaction; 201 means new — both are success.
async fn post_comment_reaction(
    repo: &str,
    comment_id: u64,
    content: &str,
    github_token: &str,
) -> Result<()> {
    let url = format!("https://api.github.com/repos/{repo}/issues/comments/{comment_id}/reactions");
    github_client()
        .post(&url)
        .header("Authorization", format!("token {github_token}"))
        .json(&serde_json::json!({ "content": content }))
        .send()
        .await
        .context("POST comment reaction")?
        .error_for_status()
        .context("POST comment reaction rejected")?;
    Ok(())
}

async fn post_pr_comment(req: &CopilotCommentRequest, reply: &str) -> Result<()> {
    let url = format!(
        "https://api.github.com/repos/{}/issues/{}/comments",
        req.repo, req.pr_number
    );
    let body = format!("@{} {}", req.commenter, reply);
    github_client()
        .post(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .json(&serde_json::json!({ "body": body }))
        .send()
        .await
        .context("POST issue comment")?
        .error_for_status()
        .context("POST issue comment rejected")?;
    Ok(())
}

/// If the agent left dirty files in the working tree, commit them with the
/// goose-copilot[bot] identity and push to the PR branch. Returns the number
/// of files changed (None if the tree is clean).
///
/// Uses the same installation token the rest of the flow uses — push lands
/// in `git log` attributed to goose-copilot[bot] via GitHub App routing.
async fn commit_and_push_if_changed(
    req: &CopilotCommentRequest,
    workdir: &Path,
) -> Result<Option<usize>> {
    if req.head_ref.is_empty() {
        return Ok(None);
    }

    let status = Command::new("git")
        .current_dir(workdir)
        .args(["status", "--porcelain"])
        .stdin(Stdio::null())
        .output()
        .await
        .context("git status")?;
    if !status.status.success() {
        bail!(
            "git status failed: {}",
            String::from_utf8_lossy(&status.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&status.stdout);
    let changed = stdout.lines().filter(|l| !l.trim().is_empty()).count();
    if changed == 0 {
        return Ok(None);
    }

    git_run(
        workdir,
        &[
            "config",
            "user.email",
            "goose-copilot[bot]@users.noreply.github.com",
        ],
    )
    .await?;
    git_run(workdir, &["config", "user.name", "goose-copilot[bot]"]).await?;
    git_run(workdir, &["add", "-A"]).await?;
    let commit_msg = format!("Apply changes requested by @{}", req.commenter);
    git_run(workdir, &["commit", "--quiet", "-m", &commit_msg]).await?;
    let refspec = format!("HEAD:refs/heads/{}", req.head_ref);
    git_run(workdir, &["push", "--quiet", "origin", &refspec]).await?;
    Ok(Some(changed))
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/copilot/review", post(review))
        .route("/copilot/setup", post(setup))
        .route("/copilot/comment", post(comment))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_agent_id_parses_tunnel_url() {
        assert_eq!(
            extract_agent_id("https://tunnel-proxy.example/tunnel/abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_agent_id("https://tunnel-proxy.example/tunnel/abc123/extra?q=1"),
            Some("abc123".to_string())
        );
        assert_eq!(extract_agent_id("https://example.com/no-tunnel-path"), None);
    }

    #[test]
    fn parse_findings_drops_blank_lines() {
        let stdout = r#"{"severity":"high","path":"src/foo.rs","line_start":10,"line_end":12,"summary":"oops","check":"main"}

{"severity":"low","path":"src/bar.rs","line_start":5,"line_end":5,"summary":"nit","check":"perf"}
"#;
        let findings = parse_findings(stdout).unwrap();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].severity, "high");
        assert_eq!(findings[1].check, "perf");
    }

    #[test]
    fn parse_findings_rejects_malformed_line() {
        let stdout = "{\"severity\":\"high\"}\nnot-json\n";
        assert!(parse_findings(stdout).is_err());
    }

    #[test]
    fn build_review_payload_inline_includes_comments() {
        let req = CopilotReviewRequest {
            github_token: "t".into(),
            repo: "o/r".into(),
            pr_number: 1,
            head_sha: "h".into(),
            pr_url: "u".into(),
            check_run_id: None,
            comment_id: None,
        };
        let findings = vec![Finding {
            severity: "high".into(),
            path: "a.rs".into(),
            line_start: 1,
            line_end: 2,
            summary: "bug".into(),
            check: "main".into(),
            suggestion: None,
        }];
        let payload = build_review_payload(&req, &findings, false);
        let comments = payload["comments"].as_array().unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0]["path"], "a.rs");
        assert_eq!(comments[0]["line"], 2);
        assert!(!comments[0]["body"]
            .as_str()
            .unwrap()
            .contains("```suggestion"));
    }

    #[test]
    fn build_review_payload_inline_emits_suggestion_block() {
        let req = CopilotReviewRequest {
            github_token: "t".into(),
            repo: "o/r".into(),
            pr_number: 1,
            head_sha: "h".into(),
            pr_url: "u".into(),
            check_run_id: None,
            comment_id: None,
        };
        let findings = vec![Finding {
            severity: "medium".into(),
            path: "a.py".into(),
            line_start: 4,
            line_end: 4,
            summary: "mutable default arg".into(),
            check: "main".into(),
            suggestion: Some("def add_tag(tag, tags=None):".into()),
        }];
        let payload = build_review_payload(&req, &findings, false);
        let body = payload["comments"][0]["body"].as_str().unwrap();
        assert!(body.contains("```suggestion\ndef add_tag(tag, tags=None):\n```"));
    }

    #[test]
    fn extract_final_assistant_text_picks_last_assistant() {
        let stdout = r#"{
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": [{"type": "tool_use", "name": "shell"}]},
                {"role": "tool", "content": [{"type": "text", "text": "tool output"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "final reply"}]}
            ]
        }"#;
        let text = extract_final_assistant_text(stdout).unwrap();
        assert_eq!(text, "final reply");
    }

    #[test]
    fn extract_final_assistant_text_errors_when_no_assistant() {
        let stdout =
            r#"{"messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]}"#;
        assert!(extract_final_assistant_text(stdout).is_err());
    }

    #[test]
    fn build_review_payload_summary_only_drops_comments() {
        let req = CopilotReviewRequest {
            github_token: "t".into(),
            repo: "o/r".into(),
            pr_number: 1,
            head_sha: "h".into(),
            pr_url: "u".into(),
            check_run_id: None,
            comment_id: None,
        };
        let findings = vec![Finding {
            severity: "high".into(),
            path: "a.rs".into(),
            line_start: 1,
            line_end: 2,
            summary: "bug".into(),
            check: "main".into(),
            suggestion: None,
        }];
        let payload = build_review_payload(&req, &findings, true);
        assert!(payload["comments"].as_array().unwrap().is_empty());
        assert!(payload["body"]
            .as_str()
            .unwrap()
            .contains("rejected by GitHub"));
    }
}
