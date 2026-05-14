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

// ---------------------------------------------------------------------------
// POST /copilot/setup — local OAuth install flow
// ---------------------------------------------------------------------------

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
    let mut flow = CopilotInstallFlow::new();
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
        "installation_id": callback.installation_id,
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

    Ok(Json(CopilotSetupResponse {
        installation_id: callback.installation_id,
    }))
}

fn switchboard_url() -> String {
    env::var(SWITCHBOARD_URL_ENV).unwrap_or_else(|_| SWITCHBOARD_URL.to_string())
}

fn extract_agent_id(tunnel_url: &str) -> Option<String> {
    tunnel_url
        .rsplit_once("/tunnel/")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or("").to_string())
}

// ---------------------------------------------------------------------------
// POST /copilot/review — webhook-triggered PR review
// ---------------------------------------------------------------------------

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
        if let Err(e) = run_review(req).await {
            tracing::error!("[copilot] review {} failed: {:#}", pr_label, e);
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
                let body = format!(
                    "**{}** ({}) — {}",
                    f.severity.to_ascii_uppercase(),
                    if f.check.is_empty() { "main" } else { &f.check },
                    f.summary
                );
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

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/copilot/review", post(review))
        .route("/copilot/setup", post(setup))
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
        };
        let findings = vec![Finding {
            severity: "high".into(),
            path: "a.rs".into(),
            line_start: 1,
            line_end: 2,
            summary: "bug".into(),
            check: "main".into(),
        }];
        let payload = build_review_payload(&req, &findings, false);
        let comments = payload["comments"].as_array().unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0]["path"], "a.rs");
        assert_eq!(comments[0]["line"], 2);
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
        };
        let findings = vec![Finding {
            severity: "high".into(),
            path: "a.rs".into(),
            line_start: 1,
            line_end: 2,
            summary: "bug".into(),
            check: "main".into(),
        }];
        let payload = build_review_payload(&req, &findings, true);
        assert!(payload["comments"].as_array().unwrap().is_empty());
        assert!(payload["body"]
            .as_str()
            .unwrap()
            .contains("rejected by GitHub"));
    }
}
