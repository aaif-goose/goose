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
use axum::{
    extract::State,
    response::Json,
    routing::{get, post},
    Router,
};
use goose::config::signup_copilot::CopilotInstallFlow;
use goose::config::Config;
use goose::copilot::{
    AnalyticsEvent, CopilotAnalytics, CopilotPrefs, CopilotReposResponse, ReviewModelChoice,
    ReviewOutputStyle, RoutingPrefs,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use utoipa::ToSchema;

use crate::routes::errors::ErrorResponse;
use crate::state::AppState;

const SWITCHBOARD_URL: &str = "https://goose-copilot-switchboard.example.workers.dev";
const SWITCHBOARD_URL_ENV: &str = "GOOSE_COPILOT_SWITCHBOARD_URL";
const USER_AGENT: &str = concat!("goose-copilot/", env!("CARGO_PKG_VERSION"));

const PREFS_CONFIG_KEY: &str = "copilot_prefs";
const INSTALLATION_ID_CONFIG_KEY: &str = "copilot_installation_id";

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

    if let Err(e) = Config::global().set_param(
        INSTALLATION_ID_CONFIG_KEY,
        serde_json::json!(register.installation_id),
    ) {
        tracing::warn!("[copilot] failed to persist installation_id: {e}");
    }

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
    State(state): State<Arc<AppState>>,
    Json(req): Json<CopilotReviewRequest>,
) -> Result<Json<CopilotReviewResponse>, ErrorResponse> {
    let pr_label = format!("{} #{}", req.repo, req.pr_number);
    tokio::spawn(async move {
        let result = run_review(req.clone()).await;
        match &result {
            Ok(_) => report_analytics_event(&state, AnalyticsEvent::PrReviewed).await,
            Err(e) => tracing::error!("[copilot] review {} failed: {:#}", pr_label, e),
        }
        if let Some(id) = req.comment_id {
            let reaction = if result.is_ok() { "+1" } else { "confused" };
            if let Err(e) =
                replace_comment_reaction(&req.repo, id, reaction, &req.github_token).await
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

async fn run_review(req: CopilotReviewRequest) -> Result<Vec<Finding>> {
    let prefs = load_prefs();
    let base_sha = fetch_pr_base_sha(&req).await?;
    let workdir = tempfile::tempdir().context("create temp workdir")?;
    git_clone_and_checkout(&req, workdir.path()).await?;
    let findings = run_goose_review(workdir.path(), &base_sha, &req.head_sha, &prefs).await?;
    post_review(&req, &findings, &prefs).await?;
    if let Some(crid) = req.check_run_id {
        complete_check_run(&req, crid, &findings).await?;
    }
    Ok(findings)
}

async fn fetch_pr_base_sha(req: &CopilotReviewRequest) -> Result<String> {
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
    Ok(pr.base.sha)
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

fn build_goose_review_args(base: &str, head: &str, prefs: &CopilotPrefs) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "review".into(),
        format!("{base}...{head}"),
        "--severity".into(),
        prefs.review_severity.as_cli_flag().into(),
        "--quiet".into(),
    ];
    if matches!(prefs.review_model_choice, ReviewModelChoice::Custom) {
        if let Some(provider) = prefs.review_provider.as_deref().filter(|s| !s.is_empty()) {
            args.push("--provider".into());
            args.push(provider.to_string());
        }
        if let Some(model) = prefs.review_model.as_deref().filter(|s| !s.is_empty()) {
            args.push("--model".into());
            args.push(model.to_string());
        }
    }
    let instructions = prefs.custom_instructions.trim();
    if !instructions.is_empty() {
        args.push("--instructions".into());
        args.push(instructions.to_string());
    }
    args
}

async fn run_goose_review(
    workdir: &Path,
    base: &str,
    head: &str,
    prefs: &CopilotPrefs,
) -> Result<Vec<Finding>> {
    let bin = locate_goose_binary()?;
    let args = build_goose_review_args(base, head, prefs);

    let output = Command::new(&bin)
        .current_dir(workdir)
        .args(&args)
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

async fn post_review(
    req: &CopilotReviewRequest,
    findings: &[Finding],
    prefs: &CopilotPrefs,
) -> Result<()> {
    let url = format!(
        "https://api.github.com/repos/{}/pulls/{}/reviews",
        req.repo, req.pr_number
    );
    let style = &prefs.review_output_style;
    let payload = build_review_payload(req, findings, style);

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

    let fallback = build_review_payload(req, findings, &ReviewOutputStyle::Summary);
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
    style: &ReviewOutputStyle,
) -> serde_json::Value {
    let include_inline = matches!(style, ReviewOutputStyle::Inline | ReviewOutputStyle::Both);
    let include_summary = matches!(style, ReviewOutputStyle::Summary | ReviewOutputStyle::Both);

    let comments: Vec<serde_json::Value> = if include_inline {
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
    } else {
        Vec::new()
    };

    let summary = if include_summary {
        let mut s = format!(
            "**[goose Copilot]({})** — {} finding(s)",
            req.pr_url,
            findings.len()
        );
        if !findings.is_empty() {
            s.push_str(":\n\n");
            for f in findings.iter().take(10) {
                s.push_str(&format!(
                    "- **{}** in `{}`: {}\n",
                    f.severity, f.path, f.summary
                ));
            }
            if findings.len() > 10 {
                s.push_str(&format!("…and {} more.\n", findings.len() - 10));
            }
        }
        if !include_inline {
            s.push_str("\n_(summary-only mode; inline annotations disabled.)_");
        }
        s
    } else {
        // Inline-only: GitHub requires a non-empty body; use a one-liner.
        format!(
            "[goose Copilot]({}) — {} inline finding(s).",
            req.pr_url,
            findings.len()
        )
    };

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
    /// `false` when the mention came from a plain issue (no PR head ref).
    /// Defaults to `true` so older switchboard payloads keep PR-only
    /// semantics until they redeploy.
    #[serde(default = "default_true")]
    pub is_pr: bool,
}

fn default_true() -> bool {
    true
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
    State(state): State<Arc<AppState>>,
    Json(req): Json<CopilotCommentRequest>,
) -> Result<Json<CopilotReviewResponse>, ErrorResponse> {
    let pr_label = format!("{} #{}", req.repo, req.pr_number);
    tokio::spawn(async move {
        let result = run_comment_reply(req.clone()).await;
        match &result {
            Ok(commit_pushed) => {
                if !req.is_pr {
                    report_analytics_event(&state, AnalyticsEvent::IssueHandled).await;
                }
                if *commit_pushed {
                    report_analytics_event(&state, AnalyticsEvent::CommitPushed).await;
                }
            }
            Err(e) => tracing::error!("[copilot] comment {} failed: {:#}", pr_label, e),
        }
        if let Some(id) = req.comment_id {
            let reaction = if result.is_ok() { "+1" } else { "confused" };
            if let Err(e) =
                replace_comment_reaction(&req.repo, id, reaction, &req.github_token).await
            {
                tracing::warn!("[copilot] comment reaction failed: {:#}", e);
            }
        }
    });
    Ok(Json(CopilotReviewResponse { accepted: true }))
}

async fn run_comment_reply(req: CopilotCommentRequest) -> Result<bool> {
    let prefs = load_prefs();
    let workdir = tempfile::tempdir().context("create temp workdir")?;
    git_clone_and_checkout_for_comment(&req, workdir.path()).await?;
    let context = match fetch_comment_context(&req).await {
        Ok(c) => Some(c),
        Err(e) => {
            tracing::warn!(
                "[copilot] could not fetch issue/PR context (continuing without): {e:#}"
            );
            None
        }
    };
    let prompt = build_comment_prompt(&req, &prefs, context.as_ref());
    let reply = run_goose_for_reply(workdir.path(), &prompt).await?;

    let outcome = decide_push_path(&req, &prefs, workdir.path()).await?;
    let final_reply = match &outcome {
        PushOutcome::CommittedToPr { files, branch } => {
            format!("{reply}\n\n_Pushed {files} file change(s) to `{branch}`._")
        }
        PushOutcome::OpenedPr { url, files } => {
            format!("{reply}\n\n_Opened a new pull request with {files} file change(s): {url}_")
        }
        PushOutcome::NoChanges | PushOutcome::Disabled => reply,
    };
    post_pr_comment(&req, &final_reply).await?;
    Ok(matches!(
        outcome,
        PushOutcome::CommittedToPr { .. } | PushOutcome::OpenedPr { .. }
    ))
}

enum PushOutcome {
    /// Agent didn't modify anything.
    NoChanges,
    /// Push paths are disabled for this combination of prefs / context.
    Disabled,
    /// Pushed to the existing PR branch.
    CommittedToPr { files: usize, branch: String },
    /// Created a fresh branch and opened a new PR (issue path).
    OpenedPr { url: String, files: usize },
}

async fn decide_push_path(
    req: &CopilotCommentRequest,
    prefs: &CopilotPrefs,
    workdir: &Path,
) -> Result<PushOutcome> {
    if req.is_pr {
        if !prefs.allow_commit_on_fix {
            return Ok(PushOutcome::Disabled);
        }
        return match commit_and_push_if_changed(req, workdir).await? {
            Some(files) => Ok(PushOutcome::CommittedToPr {
                files,
                branch: req.head_ref.clone(),
            }),
            None => Ok(PushOutcome::NoChanges),
        };
    }
    if !prefs.allow_open_new_prs {
        return Ok(PushOutcome::Disabled);
    }
    create_branch_and_open_pr_if_changed(req, workdir).await
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
    // For PR mentions, check out the PR head. For plain-issue mentions there
    // is no `refs/pull/<n>/head` to fetch — the default branch checked out by
    // `clone` is what the agent should see.
    if req.is_pr {
        let refspec = format!("+refs/pull/{}/head:refs/copilot/pr", req.pr_number);
        git_run(dest, &["fetch", "--quiet", "origin", &refspec]).await?;
        git_run(dest, &["checkout", "--quiet", "refs/copilot/pr"]).await?;
    }
    Ok(())
}

/// The bits of issue / PR context the agent needs to answer mentions sensibly.
/// Fetched via the GitHub API at job-start time using the per-install token.
struct CommentContext {
    title: String,
    body: String,
    state: String,
    /// Base branch name — only set for PRs.
    base_ref: Option<String>,
    /// Earlier comments on the thread (general + inline review comments for
    /// PRs). Sorted oldest-first, capped to the most recent N.
    prior_comments: Vec<PriorComment>,
}

struct PriorComment {
    author: String,
    /// `Some(path:line)` when this came from an inline review comment.
    location: Option<String>,
    body: String,
}

/// Cap to keep verbose issue bodies from blowing up the prompt budget. Truncated
/// bodies still give the agent enough framing to answer the user.
const MAX_CONTEXT_BODY_BYTES: usize = 8 * 1024;
/// Max prior comments to include. Newest are kept.
const MAX_PRIOR_COMMENTS: usize = 20;
/// Per-comment body cap so a single chatty reviewer can't blow the budget.
const MAX_PRIOR_COMMENT_BYTES: usize = 1500;

async fn fetch_comment_context(req: &CopilotCommentRequest) -> Result<CommentContext> {
    #[derive(Deserialize)]
    struct IssueResponse {
        title: Option<String>,
        body: Option<String>,
        state: Option<String>,
        #[serde(default)]
        pull_request: Option<serde_json::Value>,
    }
    let url = format!(
        "https://api.github.com/repos/{}/issues/{}",
        req.repo, req.pr_number
    );
    let res = github_client()
        .get(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .send()
        .await
        .context("fetch issue context")?;
    if !res.status().is_success() {
        bail!(
            "fetch issue context: {} — {}",
            res.status(),
            res.text().await.unwrap_or_default()
        );
    }
    let issue: IssueResponse = res.json().await.context("parse issue context")?;

    let mut body = issue.body.unwrap_or_default();
    if body.len() > MAX_CONTEXT_BODY_BYTES {
        body.truncate(MAX_CONTEXT_BODY_BYTES);
        body.push_str("\n…[truncated]");
    }

    let is_pr = issue.pull_request.is_some();
    // PR-specific: pull the base branch ref so the prompt can suggest the
    // right `git diff base...HEAD` invocation to the agent.
    let base_ref = if is_pr {
        match fetch_pr_base_ref(req).await {
            Ok(r) => Some(r),
            Err(e) => {
                tracing::warn!("[copilot] could not fetch PR base ref: {e:#}");
                None
            }
        }
    } else {
        None
    };
    let prior_comments = fetch_prior_comments(req, is_pr).await.unwrap_or_else(|e| {
        tracing::warn!("[copilot] could not fetch prior comments: {e:#}");
        Vec::new()
    });

    Ok(CommentContext {
        title: issue.title.unwrap_or_default(),
        body,
        state: issue.state.unwrap_or_default(),
        base_ref,
        prior_comments,
    })
}

async fn fetch_prior_comments(
    req: &CopilotCommentRequest,
    is_pr: bool,
) -> Result<Vec<PriorComment>> {
    #[derive(Deserialize)]
    struct UserRef {
        login: Option<String>,
    }
    #[derive(Deserialize)]
    struct RawComment {
        id: u64,
        user: Option<UserRef>,
        body: Option<String>,
        #[serde(default)]
        created_at: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        line: Option<u64>,
    }
    async fn get_page(url: &str, token: &str) -> Result<Vec<RawComment>> {
        let res = github_client()
            .get(url)
            .header("Authorization", format!("token {token}"))
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        if !res.status().is_success() {
            bail!("GET {url} -> {}", res.status());
        }
        res.json().await.with_context(|| format!("parse {url}"))
    }

    let issue_url = format!(
        "https://api.github.com/repos/{}/issues/{}/comments?per_page=100",
        req.repo, req.pr_number
    );
    let mut raw = get_page(&issue_url, &req.github_token)
        .await
        .unwrap_or_default();
    if is_pr {
        let pr_url = format!(
            "https://api.github.com/repos/{}/pulls/{}/comments?per_page=100",
            req.repo, req.pr_number
        );
        match get_page(&pr_url, &req.github_token).await {
            Ok(mut more) => raw.append(&mut more),
            Err(e) => tracing::warn!("[copilot] PR review-comments fetch failed: {e:#}"),
        }
    }

    // Drop the comment we're replying to so we don't echo it back as "prior".
    if let Some(cur) = req.comment_id {
        raw.retain(|c| c.id != cur);
    }
    raw.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    if raw.len() > MAX_PRIOR_COMMENTS {
        let skip = raw.len() - MAX_PRIOR_COMMENTS;
        raw.drain(0..skip);
    }

    Ok(raw
        .into_iter()
        .map(|c| {
            let mut body = c.body.unwrap_or_default();
            if body.len() > MAX_PRIOR_COMMENT_BYTES {
                body.truncate(MAX_PRIOR_COMMENT_BYTES);
                body.push_str("\n…[truncated]");
            }
            let location = match (c.path, c.line) {
                (Some(p), Some(l)) => Some(format!("{p}:{l}")),
                (Some(p), None) => Some(p),
                _ => None,
            };
            PriorComment {
                author: c.user.and_then(|u| u.login).unwrap_or_default(),
                location,
                body,
            }
        })
        .collect())
}

async fn fetch_pr_base_ref(req: &CopilotCommentRequest) -> Result<String> {
    #[derive(Deserialize)]
    struct PrResponse {
        base: BaseRef,
    }
    #[derive(Deserialize)]
    struct BaseRef {
        #[serde(rename = "ref")]
        ref_name: String,
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
        .context("fetch PR base")?;
    if !res.status().is_success() {
        bail!("fetch PR base: {}", res.status());
    }
    let pr: PrResponse = res.json().await.context("parse PR base")?;
    Ok(pr.base.ref_name)
}

fn build_comment_prompt(
    req: &CopilotCommentRequest,
    prefs: &CopilotPrefs,
    context: Option<&CommentContext>,
) -> String {
    // Strip the @goose-copilot mention so the model sees the user's actual ask.
    let cleaned = req
        .comment_body
        .replace("@goose-copilot", "")
        .trim()
        .to_string();
    let (context_kind, context_label) = if req.is_pr {
        ("pull request", "Pull request")
    } else {
        ("issue", "Issue")
    };
    let checkout_clause = if req.is_pr {
        "PR's head commit"
    } else {
        "repository's default branch"
    };
    let commit_clause = if req.is_pr && prefs.allow_commit_on_fix {
        "- Any files you change will be automatically committed and pushed to\n  \
         the PR branch after you finish — do NOT run `git commit` or `git push`\n  \
         yourself."
    } else if req.is_pr {
        "- The repo owner has DISABLED commit push for this bot. Tell the commenter\n  \
         what you would change, but don't expect your edits to land on the PR."
    } else {
        "- This is an issue, not a PR — there is no branch to push to. Reply with\n  \
         analysis, suggestions, or code samples in the comment itself."
    };
    let custom = if prefs.custom_instructions.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\nThe repo owner added these custom instructions — apply them:\n---\n{}\n---\n",
            prefs.custom_instructions.trim()
        )
    };
    let context_block = match context {
        Some(c) => {
            let mut block = format!(
                "\n{label} title: {title}\n{label} state: {state}\n",
                label = context_label,
                title = if c.title.is_empty() {
                    "(no title)"
                } else {
                    c.title.as_str()
                },
                state = c.state,
            );
            if let Some(base) = &c.base_ref {
                block.push_str(&format!(
                    "PR base branch: {base} (you can run `git diff {base}...HEAD` to see what changed)\n"
                ));
            }
            if !c.body.trim().is_empty() {
                block.push_str(&format!(
                    "\n{label} body:\n---\n{body}\n---\n",
                    label = context_label,
                    body = c.body.trim()
                ));
            }
            if !c.prior_comments.is_empty() {
                block.push_str(
                    "\nPrior comments on this thread (oldest first). When the user\n\
                     says \"address the suggested changes\" or \"fix what you flagged,\"\n\
                     they are referring to these — look here, then apply the changes\n\
                     in the working directory:\n",
                );
                for c in &c.prior_comments {
                    let loc = c
                        .location
                        .as_deref()
                        .map(|l| format!(" @ {l}"))
                        .unwrap_or_default();
                    block.push_str(&format!(
                        "---\n@{author}{loc}:\n{body}\n",
                        author = c.author,
                        loc = loc,
                        body = c.body
                    ));
                }
                block.push_str("---\n");
            }
            block
        }
        None => String::new(),
    };
    format!(
        "You are responding to a comment on a GitHub {kind}.\n\
\n\
Repository: {repo}\n\
{label}: #{num} ({url})\n\
Commenter: @{user}\n\
{context}\n\
The repository is checked out at the current working directory at the\n\
{checkout}. You can read AND modify files, run shell commands,\n\
and use any available tools.\n\
\n\
If the commenter asks you to fix, address, or apply changes:\n\
- Make the edits directly with your file-editing tools.\n\
{commit}\n\
- Then reply with a short summary of what you changed.\n\
\n\
If the commenter is asking a question or for analysis only:\n\
- Don't modify any files.\n\
- Reply with a concise answer, referencing files/lines when relevant.\n\
- If the commenter is asking whether something is resolved or addressed,\n  \
   read the {kind} body above for what they want, then check the working\n  \
   directory to see if the code addresses it.\n\
{custom}\n\
The commenter's message (with @goose-copilot stripped):\n\
---\n\
{body}\n\
---\n\
\n\
Reply with a single concise GitHub-flavored markdown response. Do NOT\n\
include a preamble like \"Sure, here's my response.\" Just answer.\n",
        kind = context_kind,
        repo = req.repo,
        label = context_label,
        num = req.pr_number,
        url = req.pr_url,
        user = req.commenter,
        context = context_block,
        checkout = checkout_clause,
        commit = commit_clause,
        custom = custom,
        body = cleaned,
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

/// Swap the bot's reaction on a comment from whatever it currently is to
/// `new_content`. Used to transition from `:eyes:` (added by switchboard on
/// receipt) to `:+1:` / `:confused:` (added by goosed on completion) without
/// leaving two emojis on the same comment.
async fn replace_comment_reaction(
    repo: &str,
    comment_id: u64,
    new_content: &str,
    github_token: &str,
) -> Result<()> {
    #[derive(Deserialize)]
    struct ReactionUser {
        login: String,
    }
    #[derive(Deserialize)]
    struct Reaction {
        id: u64,
        content: String,
        user: Option<ReactionUser>,
    }
    let list_url =
        format!("https://api.github.com/repos/{repo}/issues/comments/{comment_id}/reactions");
    let existing: Vec<Reaction> = github_client()
        .get(&list_url)
        .header("Authorization", format!("token {github_token}"))
        .send()
        .await
        .context("list comment reactions")?
        .error_for_status()
        .context("list comment reactions rejected")?
        .json()
        .await
        .context("parse comment reactions")?;
    for r in existing {
        let is_ours = r
            .user
            .as_ref()
            .map(|u| u.login.starts_with("goose-copilot"))
            .unwrap_or(false);
        if !is_ours || r.content == new_content {
            continue;
        }
        let delete_url = format!(
            "https://api.github.com/repos/{repo}/issues/comments/{comment_id}/reactions/{id}",
            id = r.id
        );
        // Best-effort: a 404 here just means the reaction is already gone.
        if let Err(e) = github_client()
            .delete(&delete_url)
            .header("Authorization", format!("token {github_token}"))
            .send()
            .await
            .and_then(|res| res.error_for_status())
        {
            tracing::warn!("[copilot] failed to delete stale reaction {}: {e:#}", r.id);
        }
    }
    post_comment_reaction(repo, comment_id, new_content, github_token).await
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

/// Issue path: create a fresh branch named `goose-copilot/issue-<n>`, commit
/// the agent's edits, push, and open a PR via the GitHub API.
async fn create_branch_and_open_pr_if_changed(
    req: &CopilotCommentRequest,
    workdir: &Path,
) -> Result<PushOutcome> {
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
        return Ok(PushOutcome::NoChanges);
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

    let head_out = Command::new("git")
        .current_dir(workdir)
        .args(["symbolic-ref", "--short", "HEAD"])
        .stdin(Stdio::null())
        .output()
        .await
        .context("git symbolic-ref")?;
    let default_branch = String::from_utf8_lossy(&head_out.stdout).trim().to_string();
    if default_branch.is_empty() {
        bail!("could not determine default branch from clone");
    }

    let branch = format!("goose-copilot/issue-{}", req.pr_number);
    git_run(workdir, &["checkout", "--quiet", "-b", &branch]).await?;
    git_run(workdir, &["add", "-A"]).await?;
    let commit_msg = format!(
        "Address issue #{} requested by @{}",
        req.pr_number, req.commenter
    );
    git_run(workdir, &["commit", "--quiet", "-m", &commit_msg]).await?;
    let refspec = format!("HEAD:refs/heads/{branch}");
    git_run(workdir, &["push", "--quiet", "origin", &refspec]).await?;

    let title = format!("Address issue #{}", req.pr_number);
    let body = format!(
        "Closes #{issue}.\n\nRequested by @{user} via Goose Copilot.",
        issue = req.pr_number,
        user = req.commenter,
    );
    let url = format!("https://api.github.com/repos/{}/pulls", req.repo);
    let res = github_client()
        .post(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .json(&serde_json::json!({
            "title": title,
            "head": branch,
            "base": default_branch,
            "body": body,
        }))
        .send()
        .await
        .context("POST new PR")?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        bail!("create PR failed ({status}): {detail}");
    }
    let pr: serde_json::Value = res.json().await.context("parse new-PR response")?;
    let html_url = pr
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(PushOutcome::OpenedPr {
        url: html_url,
        files: changed,
    })
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CopilotPrefsResponse {
    pub prefs: CopilotPrefs,
    /// `true` when the routing subset reached the switchboard. `false` is
    /// non-fatal — local persistence still succeeded and the bot will use
    /// the saved values once Desktop manages to push them.
    pub switchboard_synced: bool,
    /// Populated when `switchboard_synced` is `false`. Surface in UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switchboard_error: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CopilotPrefsRequest {
    pub prefs: CopilotPrefs,
}

#[utoipa::path(
    get,
    path = "/copilot/prefs",
    responses(
        (status = 200, description = "Current Copilot preferences", body = CopilotPrefs),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn get_prefs() -> Result<Json<CopilotPrefs>, ErrorResponse> {
    Ok(Json(load_prefs()))
}

#[utoipa::path(
    put,
    path = "/copilot/prefs",
    request_body = CopilotPrefsRequest,
    responses(
        (status = 200, description = "Preferences saved", body = CopilotPrefsResponse),
        (status = 400, description = "Validation error"),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn put_prefs(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CopilotPrefsRequest>,
) -> Result<Json<CopilotPrefsResponse>, ErrorResponse> {
    req.prefs
        .validate()
        .map_err(|e| ErrorResponse::bad_request(e.to_string()))?;

    save_prefs(&req.prefs).map_err(|e| ErrorResponse::internal(e.to_string()))?;

    let sync = forward_routing_prefs(&state, &req.prefs.routing_subset()).await;
    let (switchboard_synced, switchboard_error) = match sync {
        Ok(()) => (true, None),
        Err(e) => {
            tracing::warn!("[copilot] routing prefs sync failed: {e:#}");
            (false, Some(e.to_string()))
        }
    };

    Ok(Json(CopilotPrefsResponse {
        prefs: req.prefs,
        switchboard_synced,
        switchboard_error,
    }))
}

fn load_prefs() -> CopilotPrefs {
    match Config::global().get_param::<serde_json::Value>(PREFS_CONFIG_KEY) {
        Ok(v) => serde_json::from_value(v).unwrap_or_default(),
        Err(_) => CopilotPrefs::default(),
    }
}

fn save_prefs(prefs: &CopilotPrefs) -> Result<()> {
    Config::global()
        .set_param(PREFS_CONFIG_KEY, serde_json::to_value(prefs)?)
        .map_err(|e| anyhow!("persist copilot prefs: {e}"))
}

async fn forward_routing_prefs(state: &AppState, routing: &RoutingPrefs) -> Result<()> {
    let creds = resolve_install_credentials(state).await?;
    let res = Client::new()
        .put(format!("{}/copilot/routing-prefs", switchboard_url()))
        .header("X-Install-Id", creds.installation_id.to_string())
        .header("X-Install-Secret", &creds.tunnel_secret)
        .json(routing)
        .send()
        .await
        .context("switchboard unreachable")?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        bail!("switchboard rejected routing prefs: {status} {detail}");
    }
    Ok(())
}

struct InstallCredentials {
    installation_id: u64,
    tunnel_secret: String,
}

/// Resolve (installation_id, tunnel_secret). `installation_id` falls back to
/// a switchboard `whoami` lookup on cache miss and is then cached locally.
async fn resolve_install_credentials(state: &AppState) -> Result<InstallCredentials> {
    let tunnel_info = state.tunnel_manager.get_info().await;
    let tunnel_secret = if tunnel_info.secret.is_empty() {
        Config::global()
            .get_secret::<String>("tunnel_secret")
            .context("tunnel_secret unavailable; complete setup first")?
    } else {
        tunnel_info.secret.clone()
    };
    let agent_id = extract_agent_id(&tunnel_info.url)
        .context("tunnel URL is missing the agent id; complete setup first")?;

    if let Ok(cached) = Config::global().get_param::<u64>(INSTALLATION_ID_CONFIG_KEY) {
        return Ok(InstallCredentials {
            installation_id: cached,
            tunnel_secret,
        });
    }

    let resolved = resolve_install_id(&agent_id, &tunnel_secret).await?;
    if let Err(e) =
        Config::global().set_param(INSTALLATION_ID_CONFIG_KEY, serde_json::json!(resolved))
    {
        tracing::warn!("[copilot] failed to cache installation_id after whoami: {e}");
    }
    Ok(InstallCredentials {
        installation_id: resolved,
        tunnel_secret,
    })
}

async fn resolve_install_id(agent_id: &str, tunnel_secret: &str) -> Result<u64> {
    #[derive(Deserialize)]
    struct WhoamiResponse {
        installation_id: u64,
    }
    let res = Client::new()
        .post(format!("{}/copilot/whoami", switchboard_url()))
        .json(&serde_json::json!({
            "agent_id": agent_id,
            "tunnel_secret": tunnel_secret,
        }))
        .send()
        .await
        .context("switchboard unreachable")?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        bail!("switchboard whoami rejected: {status} {detail}");
    }
    let body: WhoamiResponse = res.json().await.context("parse whoami response")?;
    Ok(body.installation_id)
}

#[utoipa::path(
    get,
    path = "/copilot/repos",
    responses(
        (status = 200, description = "Repos accessible to the installation", body = CopilotReposResponse),
        (status = 412, description = "Setup not completed"),
        (status = 502, description = "Switchboard / GitHub error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn get_repos(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CopilotReposResponse>, ErrorResponse> {
    let creds = resolve_install_credentials(&state)
        .await
        .map_err(|e| ErrorResponse {
            message: e.to_string(),
            status: axum::http::StatusCode::PRECONDITION_FAILED,
        })?;

    let res = Client::new()
        .get(format!("{}/copilot/repos", switchboard_url()))
        .header("X-Install-Id", creds.installation_id.to_string())
        .header("X-Install-Secret", &creds.tunnel_secret)
        .send()
        .await
        .map_err(|e| ErrorResponse::internal(format!("switchboard unreachable: {e}")))?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        return Err(ErrorResponse::internal(format!(
            "switchboard returned {status}: {detail}"
        )));
    }
    let body: CopilotReposResponse = res
        .json()
        .await
        .map_err(|e| ErrorResponse::internal(format!("parse repos response: {e}")))?;
    Ok(Json(body))
}

#[utoipa::path(
    get,
    path = "/copilot/analytics",
    responses(
        (status = 200, description = "Per-install analytics rollups", body = CopilotAnalytics),
        (status = 412, description = "Setup not completed"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn get_analytics(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CopilotAnalytics>, ErrorResponse> {
    let creds = resolve_install_credentials(&state)
        .await
        .map_err(|e| ErrorResponse {
            message: e.to_string(),
            status: axum::http::StatusCode::PRECONDITION_FAILED,
        })?;
    let res = Client::new()
        .get(format!("{}/copilot/analytics", switchboard_url()))
        .header("X-Install-Id", creds.installation_id.to_string())
        .header("X-Install-Secret", &creds.tunnel_secret)
        .send()
        .await
        .map_err(|e| ErrorResponse::internal(format!("switchboard unreachable: {e}")))?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        return Err(ErrorResponse::internal(format!(
            "switchboard returned {status}: {detail}"
        )));
    }
    let body: CopilotAnalytics = res
        .json()
        .await
        .map_err(|e| ErrorResponse::internal(format!("parse analytics response: {e}")))?;
    Ok(Json(body))
}

/// Fire-and-forget analytics event. Errors are logged but never surface to
/// the caller — analytics are best-effort, not load-bearing.
async fn report_analytics_event(state: &AppState, event: AnalyticsEvent) {
    let creds = match resolve_install_credentials(state).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("[copilot] skipping analytics event — no install credentials: {e}");
            return;
        }
    };
    let res = Client::new()
        .post(format!("{}/copilot/analytics/event", switchboard_url()))
        .header("X-Install-Id", creds.installation_id.to_string())
        .header("X-Install-Secret", &creds.tunnel_secret)
        .json(&event)
        .send()
        .await;
    match res {
        Ok(r) if r.status().is_success() => {}
        Ok(r) => tracing::warn!("[copilot] analytics event rejected: {}", r.status()),
        Err(e) => tracing::warn!("[copilot] analytics event failed: {e}"),
    }
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/copilot/review", post(review))
        .route("/copilot/setup", post(setup))
        .route("/copilot/comment", post(comment))
        .route("/copilot/prefs", get(get_prefs).put(put_prefs))
        .route("/copilot/repos", get(get_repos))
        .route("/copilot/analytics", get(get_analytics))
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
        let payload = build_review_payload(&req, &findings, &ReviewOutputStyle::Both);
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
        let payload = build_review_payload(&req, &findings, &ReviewOutputStyle::Both);
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
        let payload = build_review_payload(&req, &findings, &ReviewOutputStyle::Summary);
        assert!(payload["comments"].as_array().unwrap().is_empty());
        assert!(payload["body"]
            .as_str()
            .unwrap()
            .contains("summary-only mode"));
    }

    #[test]
    fn build_review_payload_inline_only_drops_summary_body() {
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
        let payload = build_review_payload(&req, &findings, &ReviewOutputStyle::Inline);
        assert_eq!(payload["comments"].as_array().unwrap().len(), 1);
        let body = payload["body"].as_str().unwrap();
        assert!(body.contains("1 inline finding(s)"), "got body: {body}");
        assert!(!body.contains("- **high**"));
    }

    #[test]
    fn review_args_defaults() {
        let args = build_goose_review_args("base", "head", &CopilotPrefs::default());
        assert_eq!(
            args,
            vec!["review", "base...head", "--severity", "medium", "--quiet"]
        );
    }

    #[test]
    fn review_args_severity_reflects_pref() {
        use goose::copilot::ReviewSeverity;
        let prefs = CopilotPrefs {
            review_severity: ReviewSeverity::High,
            ..Default::default()
        };
        let args = build_goose_review_args("base", "head", &prefs);
        let pos = args.iter().position(|a| a == "--severity").unwrap();
        assert_eq!(args[pos + 1], "high");
    }

    #[test]
    fn review_args_threads_custom_instructions() {
        let prefs = CopilotPrefs {
            custom_instructions: "Be strict on missing tests.".into(),
            ..Default::default()
        };
        let args = build_goose_review_args("base", "head", &prefs);
        let pos = args.iter().position(|a| a == "--instructions").unwrap();
        assert_eq!(args[pos + 1], "Be strict on missing tests.");
    }

    #[test]
    fn review_args_skip_instructions_when_blank() {
        let prefs = CopilotPrefs {
            custom_instructions: "   \n\t  ".into(),
            ..Default::default()
        };
        let args = build_goose_review_args("base", "head", &prefs);
        assert!(!args.iter().any(|a| a == "--instructions"));
    }

    #[test]
    fn review_args_skip_model_flags_when_choice_is_default() {
        let prefs = CopilotPrefs {
            review_provider: Some("openai".into()),
            review_model: Some("gpt-4o".into()),
            ..Default::default()
        };
        let args = build_goose_review_args("base", "head", &prefs);
        assert!(!args.iter().any(|a| a == "--provider"));
        assert!(!args.iter().any(|a| a == "--model"));
    }

    #[test]
    fn review_args_pass_provider_and_model_when_choice_is_custom() {
        let prefs = CopilotPrefs {
            review_model_choice: ReviewModelChoice::Custom,
            review_provider: Some("anthropic".into()),
            review_model: Some("claude-sonnet-4-6".into()),
            ..Default::default()
        };
        let args = build_goose_review_args("base", "head", &prefs);
        let p = args.iter().position(|a| a == "--provider").unwrap();
        assert_eq!(args[p + 1], "anthropic");
        let m = args.iter().position(|a| a == "--model").unwrap();
        assert_eq!(args[m + 1], "claude-sonnet-4-6");
    }
}
