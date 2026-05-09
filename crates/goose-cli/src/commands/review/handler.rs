use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::session::{build_session, SessionBuilderConfig};

use super::discover::{discover, DiscoveredReview};
use super::orchestrator::{emit_findings, run_checks_in_parallel, Severity};
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
    /// Disable the Rust-driven parallel orchestrator and fall back to the
    /// single-prompt path that asks the main agent to delegate checks via
    /// `delegate(... async: true ...)`. Useful when comparing against the
    /// in-process behavior or running on a model that handles dispatch
    /// reliably on its own.
    pub no_orchestrate: bool,
    /// Additional free-form instructions to prepend to the review (PR
    /// intent, commit-message context, etc.). Surfaced to both the main
    /// agent and every check subprocess.
    pub instructions: Option<String>,
    /// Restrict the review to a specific set of files (repo-relative).
    /// When non-empty, the diff sent to the agent is filtered to only
    /// include hunks for these paths.
    pub files: Vec<String>,
    /// Only run checks whose `name` is in this list. Empty means run all
    /// discovered checks (the default).
    pub check_filter: Vec<String>,
    /// Alternate directory to search for `.agents/checks/*.md` instead of
    /// the repo root.
    pub check_scope: Option<PathBuf>,
    /// Skip the main correctness pass and only run check subagents.
    pub checks_only: bool,
    /// Print only the diff summary; skip the full review.
    pub summary_only: bool,
    /// Minimum severity to display from check findings. Defaults to
    /// `medium`, matching Amp's CLI behavior of hiding `low` from
    /// the review output.
    pub severity: String,
}

/// Entry point for the `goose review` subcommand.
pub async fn handle_review(opts: ReviewOptions) -> Result<()> {
    let repo_root = find_repo_root().context("not inside a git repository")?;
    let touched = touched_files(&repo_root, opts.range.as_deref(), &opts.files)?;
    let diff = collect_diff(&repo_root, opts.range.as_deref(), &opts.files)?;

    if diff.trim().is_empty() {
        eprintln!("goose review: no changes to review");
        return Ok(());
    }

    // `--summary-only` short-circuits everything else: print `git
    // diff --stat` and return without calling the agent. Mirrors
    // `amp review --summary-only`.
    if opts.summary_only {
        let summary = collect_diff_stat(&repo_root, opts.range.as_deref(), &opts.files)?;
        print!("{}", summary);
        return Ok(());
    }

    // `--check-scope` overrides where we look for `.agents/checks/*.md`,
    // otherwise discovery walks from the repo root + every directory on
    // the path of a touched file.
    let discovery_root = opts.check_scope.as_deref().unwrap_or(&repo_root);
    let discovered = discover(discovery_root, &touched)?;
    let discovered = filter_checks(discovered, &opts.check_filter);
    print_discovered_summary(&discovered);

    let base_prompt = match &opts.prompt_file {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("read --prompt file {}", path.display()))?,
        None => DEFAULT_REVIEW_PROMPT.to_string(),
    };
    let base_prompt = prepend_instructions(&base_prompt, opts.instructions.as_deref());

    let use_orchestrator = !opts.no_orchestrate && !discovered.checks.is_empty();

    // In orchestrator mode, the main agent does only the correctness pass —
    // checks run as parallel subprocesses, so we strip the checks table
    // from the prompt to keep the main pass focused and fast.
    let main_prompt_discovered = if use_orchestrator {
        DiscoveredReview::default()
    } else {
        discovered.clone()
    };
    let prompt = build_review_prompt(
        &base_prompt,
        &main_prompt_discovered,
        &diff,
        opts.default_model.as_deref(),
        opts.override_model.as_deref(),
        opts.default_turn_limit,
    );

    if opts.dry_run {
        println!("{}", prompt);
        if use_orchestrator {
            println!(
                "\n# orchestrator: {} check(s) would run as parallel subprocesses",
                discovered.checks.len()
            );
        }
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
        provider: opts.provider.clone(),
        model: opts.default_model.clone(),
        quiet: opts.quiet,
        output_format: "text".to_string(),
        ..SessionBuilderConfig::default()
    })
    .await;

    if !use_orchestrator {
        if opts.checks_only {
            // No checks discovered (or `--no-orchestrate` set with checks
            // empty after filter). With `--checks-only` there is nothing
            // left to run, so return cleanly instead of dispatching the
            // main agent.
            return Ok(());
        }
        return session.headless(prompt).await;
    }

    // Orchestrated mode: run main correctness pass + N parallel check
    // subprocesses concurrently. Wall clock is bounded by `max(main_pass,
    // slowest_check)` instead of relying on the main agent to dispatch
    // delegates correctly. With `--checks-only` we skip the main pass and
    // wait only on the orchestrator.
    let check_results = if opts.checks_only {
        run_checks_in_parallel(&discovered.checks, &diff, &opts).await
    } else {
        let main_pass = async move { session.headless(prompt).await };
        let checks_pass = run_checks_in_parallel(&discovered.checks, &diff, &opts);
        let (main_result, check_results) = tokio::join!(main_pass, checks_pass);
        main_result?;
        check_results
    };

    // Main pass (if it ran) streamed its findings to stdout as it ran;
    // emit check findings after, in source order, so attribution is
    // preserved end-to-end. Severity floor is applied at emit time so
    // that suppressed findings still show up in counts on stderr —
    // useful when triaging "the model produced N findings but I only
    // see M".
    // Empty (e.g. from `..ReviewOptions::default()` in tests) means
    // "use the documented default", which matches the CLI's
    // `default_value = "medium"`.
    let sev_str = if opts.severity.is_empty() {
        "medium"
    } else {
        opts.severity.as_str()
    };
    let min_sev: Severity = sev_str
        .parse()
        .map_err(|e: String| anyhow!("--severity: {e}"))?;
    let mut total_emitted = 0usize;
    let mut total_seen = 0usize;
    for findings in &check_results {
        total_seen += findings.len();
        total_emitted += emit_findings(findings, min_sev);
    }
    if !opts.quiet {
        let suppressed = total_seen.saturating_sub(total_emitted);
        if suppressed == 0 {
            eprintln!(
                "goose review: orchestrator emitted {total_emitted} finding(s) from {} check(s)",
                discovered.checks.len()
            );
        } else {
            eprintln!(
                "goose review: orchestrator emitted {total_emitted} finding(s) from {} check(s) ({suppressed} hidden below severity={:?})",
                discovered.checks.len(),
                min_sev
            );
        }
    }

    Ok(())
}

/// Restrict a discovered review to the named checks (no-op when the
/// filter is empty). Mirrors `amp review --check-filter`.
fn filter_checks(discovered: DiscoveredReview, names: &[String]) -> DiscoveredReview {
    if names.is_empty() {
        return discovered;
    }
    let allow: std::collections::HashSet<&str> = names.iter().map(String::as_str).collect();
    DiscoveredReview {
        checks: discovered
            .checks
            .into_iter()
            .filter(|c| allow.contains(c.name.as_str()))
            .collect(),
    }
}

/// Prepend a free-form `--instructions <text>` block to the base prompt
/// so it is visible to both the main agent and (via the orchestrator)
/// every per-check subprocess.
fn prepend_instructions(base_prompt: &str, instructions: Option<&str>) -> String {
    match instructions {
        Some(text) if !text.trim().is_empty() => {
            format!(
                "## Reviewer instructions\n\n{}\n\n{}",
                text.trim(),
                base_prompt
            )
        }
        _ => base_prompt.to_string(),
    }
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

fn touched_files(repo_root: &Path, range: Option<&str>, files: &[String]) -> Result<Vec<String>> {
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
    if !files.is_empty() {
        cmd.arg("--");
        for f in files {
            cmd.arg(f);
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

fn collect_diff(repo_root: &Path, range: Option<&str>, files: &[String]) -> Result<String> {
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
    if !files.is_empty() {
        cmd.arg("--");
        for f in files {
            cmd.arg(f);
        }
    }
    let out = cmd.output().context("git diff failed")?;
    if !out.status.success() {
        bail!("git diff failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    String::from_utf8(out.stdout).map_err(|e| anyhow!("git diff returned non-UTF8 output: {e}"))
}

fn collect_diff_stat(
    repo_root: &Path,
    range: Option<&str>,
    files: &[String],
) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_root).arg("diff").arg("--stat");
    match range {
        Some(r) => {
            cmd.arg(r);
        }
        None => {
            cmd.arg("HEAD");
        }
    }
    if !files.is_empty() {
        cmd.arg("--");
        for f in files {
            cmd.arg(f);
        }
    }
    let out = cmd.output().context("git diff --stat failed")?;
    if !out.status.success() {
        bail!(
            "git diff --stat failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout)
        .map_err(|e| anyhow!("git diff --stat returned non-UTF8 output: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::review::check::Check;
    use std::path::PathBuf;

    fn ck(name: &str) -> Check {
        Check {
            name: name.to_string(),
            description: None,
            model: None,
            turn_limit: None,
            tools: None,
            severity_default: None,
            path: PathBuf::from(format!("/.agents/checks/{name}.md")),
            scope_dir: String::new(),
            body: "body".into(),
        }
    }

    #[test]
    fn filter_checks_passes_through_when_filter_empty() {
        let d = DiscoveredReview {
            checks: vec![ck("perf"), ck("security")],
        };
        let out = filter_checks(d, &[]);
        assert_eq!(out.checks.len(), 2);
    }

    #[test]
    fn filter_checks_keeps_only_named_checks() {
        let d = DiscoveredReview {
            checks: vec![ck("perf"), ck("security"), ck("idempotency")],
        };
        let out = filter_checks(d, &["security".to_string(), "idempotency".to_string()]);
        let names: Vec<&str> = out.checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["security", "idempotency"]);
    }

    #[test]
    fn prepend_instructions_noop_when_none_or_empty() {
        assert_eq!(prepend_instructions("BASE", None), "BASE");
        assert_eq!(prepend_instructions("BASE", Some("   ")), "BASE");
    }

    #[test]
    fn prepend_instructions_adds_block_above_base() {
        let out = prepend_instructions("BASE", Some("Refactor only — flag any behavior change."));
        assert!(out.starts_with("## Reviewer instructions\n\nRefactor only"));
        assert!(out.ends_with("BASE"));
    }
}
