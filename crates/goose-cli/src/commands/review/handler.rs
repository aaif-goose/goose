use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::session::{build_session, SessionBuilderConfig};

use super::discover::{discover, DiscoveredReview};
use super::prompt::{build_review_prompt, DEFAULT_REVIEW_PROMPT};

/// Options for `goose review`.
#[derive(Debug, Clone, Default)]
pub struct ReviewOptions {
    /// Diff range to review (e.g. `main...HEAD`). When `None`, falls back to
    /// the working tree vs. the inferred merge base / default branch.
    pub range: Option<String>,
    /// Path to a markdown file with a custom base review prompt. Overrides the
    /// embedded default prompt entirely.
    pub prompt_file: Option<PathBuf>,
    /// Default model used for the main review agent and for any check that
    /// does not declare its own `model:`.
    pub default_model: Option<String>,
    /// Provider for the main review agent.
    pub provider: Option<String>,
    /// Force every discovered check to run with this model, regardless of
    /// the check's own `model:` field.
    pub override_model: Option<String>,
    /// Default `turn-limit` applied to checks that do not declare their own.
    pub default_turn_limit: Option<usize>,
    /// Print the assembled prompt and discovered checks instead of dispatching
    /// the review.
    pub dry_run: bool,
    /// Suppress non-result output from the underlying agent.
    pub quiet: bool,
}

/// Entry point for the `goose review` subcommand.
pub async fn handle_review(opts: ReviewOptions) -> Result<()> {
    let repo_root = find_repo_root().context("not inside a git repository")?;
    let touched = touched_files(&repo_root, opts.range.as_deref())?;
    let diff = collect_diff(&repo_root, opts.range.as_deref())?;

    if diff.trim().is_empty() {
        eprintln!("goose review: no changes to review");
        return Ok(());
    }

    let discovered = discover(&repo_root, &touched)?;
    print_discovered_summary(&discovered);

    let base_prompt = match &opts.prompt_file {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("read --prompt file {}", path.display()))?,
        None => DEFAULT_REVIEW_PROMPT.to_string(),
    };

    let prompt = build_review_prompt(
        &base_prompt,
        &discovered,
        &diff,
        opts.default_model.as_deref(),
        opts.override_model.as_deref(),
        opts.default_turn_limit,
    );

    if opts.dry_run {
        println!("{}", prompt);
        return Ok(());
    }

    // Review only needs file inspection (developer) + parallel subagent
    // dispatch (summon, which exposes `delegate`/`load`). Skip the user's
    // configured extension profile so we don't pay the latency / token cost
    // of loading github, blockcell, MCPs, etc. that play no role in review.
    let mut session = build_session(SessionBuilderConfig {
        session_id: None,
        no_session: true,
        no_profile: true,
        builtins: vec!["developer".to_string(), "summon".to_string()],
        provider: opts.provider,
        model: opts.default_model,
        quiet: opts.quiet,
        output_format: "text".to_string(),
        ..SessionBuilderConfig::default()
    })
    .await;

    session.headless(prompt).await
}

fn print_discovered_summary(d: &DiscoveredReview) {
    if d.checks.is_empty() {
        eprintln!("goose review: no checks or REVIEW.md rules discovered");
        return;
    }
    eprintln!("goose review: discovered {} check(s):", d.checks.len());
    for c in &d.checks {
        let scope = if c.scope_dir.is_empty() {
            "<root>"
        } else {
            &c.scope_dir
        };
        eprintln!("  - {} (scope: {})", c.name, scope);
    }
}

fn find_repo_root() -> Result<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("failed to invoke git")?;
    if !out.status.success() {
        bail!(
            "git rev-parse --show-toplevel failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let path = String::from_utf8(out.stdout)?.trim().to_string();
    Ok(PathBuf::from(path))
}

fn touched_files(repo_root: &Path, range: Option<&str>) -> Result<Vec<String>> {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_root).arg("diff").arg("--name-only");
    match range {
        Some(r) => {
            cmd.arg(r);
        }
        None => {
            cmd.arg("HEAD");
        }
    }
    let out = cmd.output().context("git diff --name-only failed")?;
    if !out.status.success() {
        bail!(
            "git diff --name-only failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8(out.stdout)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect())
}

fn collect_diff(repo_root: &Path, range: Option<&str>) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_root).arg("diff");
    match range {
        Some(r) => {
            cmd.arg(r);
        }
        None => {
            cmd.arg("HEAD");
        }
    }
    let out = cmd.output().context("git diff failed")?;
    if !out.status.success() {
        bail!("git diff failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    String::from_utf8(out.stdout).map_err(|e| anyhow!("git diff returned non-UTF8 output: {e}"))
}
