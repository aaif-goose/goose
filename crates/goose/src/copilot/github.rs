use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Result};
use reqwest::Client;
use serde::Deserialize;
use tokio::process::Command;

use super::review::{build_review_payload, Finding, ReviewPublishContext};
use super::types::CopilotReviewRequest;
use super::{CopilotPrefs, ReviewOutputStyle};

const USER_AGENT: &str = concat!("goose-copilot/", env!("CARGO_PKG_VERSION"));

pub fn client() -> Client {
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

pub async fn git_run(cwd: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

pub async fn fetch_pr_base_sha(req: &CopilotReviewRequest) -> Result<String> {
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
    let res = client()
        .get(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .send()
        .await?;
    if !res.status().is_success() {
        bail!(
            "fetch PR metadata: {} — {}",
            res.status(),
            res.text().await.unwrap_or_default()
        );
    }
    let pr: PrResponse = res.json().await?;
    Ok(pr.base.sha)
}

pub async fn git_clone_and_checkout_pr(req: &CopilotReviewRequest, dest: &Path) -> Result<()> {
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

pub async fn post_review(
    req: &CopilotReviewRequest,
    findings: &[Finding],
    prefs: &CopilotPrefs,
) -> Result<()> {
    let url = format!(
        "https://api.github.com/repos/{}/pulls/{}/reviews",
        req.repo, req.pr_number
    );
    let style = &prefs.review_output_style;
    let ctx = ReviewPublishContext {
        pr_url: &req.pr_url,
        head_sha: &req.head_sha,
    };
    let payload = build_review_payload(ctx, findings, style);

    let res = client()
        .post(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .json(&payload)
        .send()
        .await?;

    if res.status().is_success() {
        return Ok(());
    }

    let status = res.status();
    let detail = res.text().await.unwrap_or_default();
    tracing::debug!(
        "[copilot] inline review rejected ({status}): {} — falling back to summary-only",
        detail.chars().take(300).collect::<String>()
    );

    let fallback = build_review_payload(ctx, findings, &ReviewOutputStyle::Summary);
    client()
        .post(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .json(&fallback)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn complete_check_run(
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
    client()
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
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn post_comment_reaction(
    repo: &str,
    comment_id: u64,
    content: &str,
    github_token: &str,
) -> Result<()> {
    let url = format!("https://api.github.com/repos/{repo}/issues/comments/{comment_id}/reactions");
    client()
        .post(&url)
        .header("Authorization", format!("token {github_token}"))
        .json(&serde_json::json!({ "content": content }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn replace_comment_reaction(
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
    let existing: Vec<Reaction> = client()
        .get(&list_url)
        .header("Authorization", format!("token {github_token}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
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
        let _ = client()
            .delete(&delete_url)
            .header("Authorization", format!("token {github_token}"))
            .send()
            .await
            .and_then(|res| res.error_for_status());
    }
    post_comment_reaction(repo, comment_id, new_content, github_token).await
}

pub async fn configure_bot_git_identity(workdir: &Path) -> Result<()> {
    git_run(
        workdir,
        &[
            "config",
            "user.email",
            "goose-copilot[bot]@users.noreply.github.com",
        ],
    )
    .await?;
    git_run(workdir, &["config", "user.name", "goose-copilot[bot]"]).await
}
