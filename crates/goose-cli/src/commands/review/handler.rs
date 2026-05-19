use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::session::{build_session, SessionBuilderConfig};

use goose::checks::{discover, DiscoveredReview};

use super::orchestrator::{
    emit_findings, run_checks_in_parallel, run_main_pass_in_parallel, Severity,
};
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

    // Validate `--severity` once, up front, so a bogus value fails fast
    // regardless of which orchestration path we end up taking.
    let sev_str = if opts.severity.is_empty() {
        "medium"
    } else {
        opts.severity.as_str()
    };
    let min_sev: Severity = sev_str
        .parse()
        .map_err(|e: String| anyhow!("--severity: {e}"))?;

    let mut touched = touched_files(&repo_root, opts.range.as_deref(), &opts.files)?;
    let mut diff = collect_diff(&repo_root, opts.range.as_deref(), &opts.files)?;

    // Without an explicit `--range`, `git diff HEAD` excludes untracked
    // files entirely — brand-new files would silently miss the review.
    // Synthesize a `new file` diff for each so the main pass and the
    // checks see them.
    if opts.range.is_none() {
        let untracked = untracked_files(&repo_root, &opts.files)?;
        if !untracked.is_empty() {
            let untracked_diff = synthesize_untracked_diff(&repo_root, &untracked)?;
            diff.push_str(&untracked_diff);
            for u in untracked {
                if !touched.contains(&u) {
                    touched.push(u);
                }
            }
        }
    }

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
    // `touched` is repo-relative; rebase to discovery_root so candidate
    // scope walking doesn't double-prefix `<scope>/api/...` for files
    // already living under the scope.
    let discovery_touched = rebase_touched_to_scope(&repo_root, discovery_root, &touched);
    let discovered = discover(discovery_root, &discovery_touched)?;
    let discovered = filter_checks(discovered, &opts.check_filter);
    if !opts.quiet {
        print_discovered_summary(&discovered);
    }

    let base_prompt = match &opts.prompt_file {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("read --prompt file {}", path.display()))?,
        None => DEFAULT_REVIEW_PROMPT.to_string(),
    };

    let use_orchestrator = !opts.no_orchestrate;

    // Reviewer instructions are also injected into every per-file
    // main-pass subprocess and every per-check subprocess. To avoid
    // duplicating them, only prepend to the base prompt for the legacy
    // single-prompt (`--no-orchestrate`) path.
    let base_prompt = if use_orchestrator {
        base_prompt
    } else {
        prepend_instructions(&base_prompt, opts.instructions.as_deref())
    };

    // In orchestrator mode, the main pass runs as N parallel subprocesses
    // (one per touched file) — checks run as parallel subprocesses too —
    // so the assembled prompt only matters for the legacy in-process path.
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
            println!("# orchestrator: main pass would fan out one subprocess per touched file");
        }
        return Ok(());
    }

    if !use_orchestrator {
        // Legacy in-process path (--no-orchestrate). Useful for comparing
        // against orchestrated wall clock and for models that handle
        // delegation reliably on their own.
        if opts.checks_only {
            // The legacy path runs everything as a single agent prompt,
            // so it has no way to "skip the main pass". Fall back to the
            // orchestrator's check-runner (which IS able to run checks
            // in isolation) instead of silently no-op'ing.
            let check_results = run_checks_in_parallel(&discovered.checks, &diff, &opts).await;
            let mut total_emitted = 0usize;
            let mut total_seen = 0usize;
            for findings in &check_results {
                total_seen += findings.len();
                total_emitted += emit_findings(findings, min_sev);
            }
            if !opts.quiet {
                let suppressed = total_seen.saturating_sub(total_emitted);
                eprintln!(
                    "goose review: emitted {total_emitted} finding(s) from {} check(s) ({suppressed} hidden below severity={:?})",
                    discovered.checks.len(),
                    min_sev
                );
            }
            return Ok(());
        }
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
        return session.headless(prompt).await;
    }

    // Orchestrated mode: run the main correctness pass (per-file
    // parallel subprocesses) and the discovered checks (one subprocess
    // each, capped at MAX_WORKERS) concurrently. Wall clock is bounded
    // by `max(slowest_main_file, slowest_check)` instead of scaling
    // with diff size or check count.
    let main_findings_fut = async {
        if opts.checks_only {
            Vec::new()
        } else {
            run_main_pass_in_parallel(&diff, &base_prompt, &opts).await
        }
    };
    let checks_fut = run_checks_in_parallel(&discovered.checks, &diff, &opts);
    let (main_findings, check_results) = tokio::join!(main_findings_fut, checks_fut);

    let mut total_emitted = 0usize;
    let mut total_seen = main_findings.len();
    total_emitted += emit_findings(&main_findings, min_sev);
    for findings in &check_results {
        total_seen += findings.len();
        total_emitted += emit_findings(findings, min_sev);
    }
    if !opts.quiet {
        let suppressed = total_seen.saturating_sub(total_emitted);
        let main_pass_label = if opts.checks_only { "skipped" } else { "ran" };
        if suppressed == 0 {
            eprintln!(
                "goose review: orchestrator emitted {total_emitted} finding(s) from {} check(s) (main: {main_pass_label}, {} finding(s))",
                discovered.checks.len(),
                main_findings.len()
            );
        } else {
            eprintln!(
                "goose review: orchestrator emitted {total_emitted} finding(s) from {} check(s) (main: {main_pass_label}, {} finding(s); {suppressed} hidden below severity={:?})",
                discovered.checks.len(),
                main_findings.len(),
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

/// Configure a `git` Command to disable quoting of non-ASCII paths.
/// Without this, paths containing non-ASCII bytes come back as quoted
/// C-style escapes (`"dir/\303\251.txt"`), which downstream parsers
/// would have to round-trip-decode just to spell the filename. We turn
/// it off everywhere so callers always get clean UTF-8 paths.
fn git_command(repo_root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_root)
        .args(["-c", "core.quotePath=off"]);
    cmd
}

fn touched_files(repo_root: &Path, range: Option<&str>, files: &[String]) -> Result<Vec<String>> {
    let mut cmd = git_command(repo_root);
    cmd.arg("diff").arg("--name-only");
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
    let mut cmd = git_command(repo_root);
    cmd.arg("diff");
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

fn collect_diff_stat(repo_root: &Path, range: Option<&str>, files: &[String]) -> Result<String> {
    let mut cmd = git_command(repo_root);
    cmd.arg("diff").arg("--stat");
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

/// List untracked-but-not-ignored files in `repo_root`. Used to expose
/// brand-new files to the review when no `--range` is given (default
/// `git diff HEAD` would silently drop them).
fn untracked_files(repo_root: &Path, files: &[String]) -> Result<Vec<String>> {
    let mut cmd = git_command(repo_root);
    cmd.args(["ls-files", "--others", "--exclude-standard"]);
    if !files.is_empty() {
        cmd.arg("--");
        for f in files {
            cmd.arg(f);
        }
    }
    let out = cmd.output().context("git ls-files failed")?;
    if !out.status.success() {
        bail!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8(out.stdout)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect())
}

/// Synthesize a unified `new file` diff for each untracked path so
/// downstream parsers and the review prompt can treat them as
/// additions. Binary or unreadable files are skipped (we cannot
/// produce a meaningful textual diff for them).
fn synthesize_untracked_diff(repo_root: &Path, paths: &[String]) -> Result<String> {
    let mut out = String::new();
    for path in paths {
        let abs = repo_root.join(path);
        let content = match fs::read_to_string(&abs) {
            Ok(c) => c,
            Err(_) => continue,
        };
        out.push_str(&format!("diff --git a/{path} b/{path}\n"));
        out.push_str("new file mode 100644\n");
        out.push_str("--- /dev/null\n");
        out.push_str(&format!("+++ b/{path}\n"));
        let trailing_newline = content.ends_with('\n');
        let line_count = if content.is_empty() {
            0
        } else if trailing_newline {
            content.matches('\n').count()
        } else {
            content.matches('\n').count() + 1
        };
        if line_count > 0 {
            out.push_str(&format!("@@ -0,0 +1,{line_count} @@\n"));
            for line in content.split_inclusive('\n') {
                let body = line.strip_suffix('\n').unwrap_or(line);
                out.push('+');
                out.push_str(body);
                out.push('\n');
            }
            if !trailing_newline {
                out.push_str("\\ No newline at end of file\n");
            }
        }
    }
    Ok(out)
}

/// Convert repo-relative `touched` paths into paths relative to
/// `discovery_root` so [`goose::checks::discover`] doesn't double-
/// prefix `<scope>/api/...` when `--check-scope` points at a subtree.
/// Files outside the scope are dropped — they cannot affect any
/// scoped check inside `discovery_root`.
fn rebase_touched_to_scope(
    repo_root: &Path,
    discovery_root: &Path,
    touched: &[String],
) -> Vec<String> {
    if discovery_root == repo_root {
        return touched.to_vec();
    }
    let prefix = match discovery_root.strip_prefix(repo_root) {
        Ok(p) => p,
        Err(_) => return touched.to_vec(),
    };
    let prefix_str = prefix.to_string_lossy().replace('\\', "/");
    if prefix_str.is_empty() {
        return touched.to_vec();
    }
    let prefix_with_slash = format!("{prefix_str}/");
    touched
        .iter()
        .filter_map(|p| p.strip_prefix(&prefix_with_slash).map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose::checks::Check;
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

    #[test]
    fn synthesize_untracked_diff_emits_new_file_chunk_with_added_lines() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = root.join("new/file.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "alpha\nbeta\ngamma\n").unwrap();

        let diff = synthesize_untracked_diff(root, &["new/file.txt".to_string()]).unwrap();
        assert!(diff.contains("diff --git a/new/file.txt b/new/file.txt"));
        assert!(diff.contains("new file mode 100644"));
        assert!(diff.contains("--- /dev/null"));
        assert!(diff.contains("+++ b/new/file.txt"));
        assert!(diff.contains("@@ -0,0 +1,3 @@"));
        assert!(diff.contains("+alpha\n+beta\n+gamma\n"));
        assert!(!diff.contains("\\ No newline at end of file"));
    }

    #[test]
    fn synthesize_untracked_diff_marks_missing_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.txt"), "no-newline").unwrap();

        let diff = synthesize_untracked_diff(root, &["a.txt".to_string()]).unwrap();
        assert!(diff.contains("@@ -0,0 +1,1 @@"));
        assert!(diff.contains("+no-newline\n"));
        assert!(diff.contains("\\ No newline at end of file"));
    }

    #[test]
    fn rebase_touched_to_scope_strips_scope_prefix() {
        let repo = PathBuf::from("/repo");
        let scope = PathBuf::from("/repo/api/v2");
        let touched = vec![
            "api/v2/foo.rs".to_string(),
            "api/v2/bar.rs".to_string(),
            "frontend/main.tsx".to_string(),
        ];
        let out = rebase_touched_to_scope(&repo, &scope, &touched);
        assert_eq!(out, vec!["foo.rs", "bar.rs"]);
    }

    #[test]
    fn rebase_touched_to_scope_passes_through_when_scope_equals_repo() {
        let repo = PathBuf::from("/repo");
        let touched = vec!["a.rs".to_string()];
        let out = rebase_touched_to_scope(&repo, &repo, &touched);
        assert_eq!(out, touched);
    }
}
