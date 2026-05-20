use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Result};
use serde::Deserialize;
use tokio::process::Command;

use super::github::{client, configure_bot_git_identity, git_run};
use super::runner::run_goose_for_reply;
use super::store::load_prefs;
use super::types::CopilotCommentRequest;
use super::CopilotPrefs;

pub async fn run_comment_reply(req: CopilotCommentRequest) -> Result<bool> {
    let prefs = load_prefs();
    let workdir = tempfile::tempdir()?;
    git_clone_and_checkout_for_comment(&req, workdir.path()).await?;
    let context = fetch_comment_context(&req).await.ok();
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
    NoChanges,
    Disabled,
    CommittedToPr { files: usize, branch: String },
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
    if req.is_pr {
        let refspec = format!("+refs/pull/{}/head:refs/copilot/pr", req.pr_number);
        git_run(dest, &["fetch", "--quiet", "origin", &refspec]).await?;
        git_run(dest, &["checkout", "--quiet", "refs/copilot/pr"]).await?;
    }
    Ok(())
}

struct CommentContext {
    title: String,
    body: String,
    state: String,
    base_ref: Option<String>,
    prior_comments: Vec<PriorComment>,
}

struct PriorComment {
    author: String,
    location: Option<String>,
    body: String,
}

const MAX_CONTEXT_BODY_BYTES: usize = 8 * 1024;
const MAX_PRIOR_COMMENTS: usize = 20;
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
    let res = client()
        .get(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .send()
        .await?;
    if !res.status().is_success() {
        bail!(
            "fetch issue context: {} — {}",
            res.status(),
            res.text().await.unwrap_or_default()
        );
    }
    let issue: IssueResponse = res.json().await?;

    let mut body = issue.body.unwrap_or_default();
    if body.len() > MAX_CONTEXT_BODY_BYTES {
        body.truncate(MAX_CONTEXT_BODY_BYTES);
        body.push_str("\n…[truncated]");
    }

    let is_pr = issue.pull_request.is_some();
    let base_ref = if is_pr {
        fetch_pr_base_ref(req).await.ok()
    } else {
        None
    };
    let prior_comments = fetch_prior_comments(req, is_pr).await.unwrap_or_default();

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
        let res = client()
            .get(url)
            .header("Authorization", format!("token {token}"))
            .send()
            .await?;
        if !res.status().is_success() {
            bail!("GET {url} -> {}", res.status());
        }
        res.json().await.map_err(Into::into)
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
        if let Ok(mut more) = get_page(&pr_url, &req.github_token).await {
            raw.append(&mut more);
        }
    }

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
    let res = client()
        .get(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .send()
        .await?;
    if !res.status().is_success() {
        bail!("fetch PR base: {}", res.status());
    }
    let pr: PrResponse = res.json().await?;
    Ok(pr.base.ref_name)
}

fn build_comment_prompt(
    req: &CopilotCommentRequest,
    prefs: &CopilotPrefs,
    context: Option<&CommentContext>,
) -> String {
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

async fn post_pr_comment(req: &CopilotCommentRequest, reply: &str) -> Result<()> {
    let url = format!(
        "https://api.github.com/repos/{}/issues/{}/comments",
        req.repo, req.pr_number
    );
    let body = format!("@{} {}", req.commenter, reply);
    client()
        .post(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .json(&serde_json::json!({ "body": body }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

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
        .await?;
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

    configure_bot_git_identity(workdir).await?;
    git_run(workdir, &["add", "-A"]).await?;
    let commit_msg = format!("Apply changes requested by @{}", req.commenter);
    git_run(workdir, &["commit", "--quiet", "-m", &commit_msg]).await?;
    let refspec = format!("HEAD:refs/heads/{}", req.head_ref);
    git_run(workdir, &["push", "--quiet", "origin", &refspec]).await?;
    Ok(Some(changed))
}

async fn create_branch_and_open_pr_if_changed(
    req: &CopilotCommentRequest,
    workdir: &Path,
) -> Result<PushOutcome> {
    let status = Command::new("git")
        .current_dir(workdir)
        .args(["status", "--porcelain"])
        .stdin(Stdio::null())
        .output()
        .await?;
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

    configure_bot_git_identity(workdir).await?;

    let head_out = Command::new("git")
        .current_dir(workdir)
        .args(["symbolic-ref", "--short", "HEAD"])
        .stdin(Stdio::null())
        .output()
        .await?;
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
    let res = client()
        .post(&url)
        .header("Authorization", format!("token {}", req.github_token))
        .json(&serde_json::json!({
            "title": title,
            "head": branch,
            "base": default_branch,
            "body": body,
        }))
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        bail!("create PR failed ({status}): {detail}");
    }
    let pr: serde_json::Value = res.json().await?;
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
