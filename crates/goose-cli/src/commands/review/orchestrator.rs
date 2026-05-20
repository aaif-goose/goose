//! Deterministic, Rust-driven orchestration for `goose review`.
//!
//! The default in-process review path lets the LLM decide whether to
//! dispatch each check as a real subagent (`delegate(... async: true)`)
//! or to inline the work itself. That decision is non-deterministic and
//! is the dominant source of variance we see between runs (16s in the
//! best case, 60s+ when the model dispatches everything as a separate
//! subagent).
//!
//! This module sidesteps that variance by orchestrating checks
//! deterministically from Rust:
//!
//! - One subprocess per check (`goose run -q -t <prompt>`)
//! - Concurrency capped at [`MAX_WORKERS`] via a Tokio semaphore
//! - Per-check timeout of [`CHECK_TIMEOUT_SECS`]
//! - Each check is given a strict, tool-free prompt and is required to
//!   return only `{"findings": [...]}` JSON
//! - Findings are tagged with the originating `check` name in Rust, not
//!   by the model
//!
//! Wall-clock for the orchestrated phase is therefore
//! `max(check_latency)` — bounded by the slowest single check — rather
//! than the sum of model-driven dispatch overhead.
//!
//! The main correctness pass still runs in-process via the existing
//! `session.headless()` path; the two phases are awaited concurrently
//! so the user sees both their findings as soon as the slower of the
//! two completes.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::timeout;

use super::handler::ReviewOptions;
use goose::checks::Check;

/// Maximum number of check subprocesses we run concurrently. 4 is
/// empirically the sweet spot before LLM-side rate limits and local
/// resource contention start hurting wall-clock.
pub const MAX_WORKERS: usize = 4;

/// Hard wall-clock cap for a single check subprocess. A check that
/// takes longer than this is almost always stuck in a tool-call loop
/// or a retry storm; we'd rather surface the timeout than block the
/// whole review.
pub const CHECK_TIMEOUT_SECS: u64 = 5 * 60;

/// One review finding emitted by a check or by the main correctness
/// pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: String,
    pub path: String,
    pub line_start: i64,
    pub line_end: i64,
    pub summary: String,
    pub check: String,
}

/// Schema the check subprocess is required to emit.
#[derive(Debug, Deserialize)]
struct FindingsResponse {
    findings: Vec<RawFinding>,
}

#[derive(Debug, Deserialize)]
struct RawFinding {
    #[serde(default)]
    severity: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    line_start: Option<i64>,
    #[serde(default)]
    line_end: Option<i64>,
    #[serde(default)]
    summary: Option<String>,
}

/// Run all discovered checks concurrently as `goose run` subprocesses.
///
/// Returns one `Vec<Finding>` per check, in the same order as `checks`.
/// A failed check (subprocess error, timeout, malformed JSON) yields an
/// empty findings list and a warning on stderr; a single broken check
/// must never block the rest of the review.
pub async fn run_checks_in_parallel(
    checks: &[Check],
    diff: &str,
    opts: &ReviewOptions,
) -> Vec<Vec<Finding>> {
    let semaphore = Arc::new(Semaphore::new(MAX_WORKERS));
    let mut set = JoinSet::new();

    for (idx, check) in checks.iter().enumerate() {
        let sem = semaphore.clone();
        let check = check.clone();
        let diff = diff.to_string();
        let provider = opts.provider.clone();
        let model = resolve_check_model(&check, opts);
        let max_turns = check.resolved_turn_limit(opts.default_turn_limit);
        let quiet = opts.quiet;
        let instructions = opts.instructions.clone();

        set.spawn(async move {
            // Bounded concurrency: drop the permit only after the
            // subprocess completes.
            let _permit = sem.acquire().await.expect("semaphore is never closed");
            let result = run_single_check_subprocess(
                &check,
                &diff,
                provider.as_deref(),
                model.as_deref(),
                instructions.as_deref(),
                Some(max_turns),
            )
            .await;
            (idx, check, result, quiet)
        });
    }

    // Pre-allocate so we can write results in source order.
    let mut results: Vec<Vec<Finding>> = vec![Vec::new(); checks.len()];

    while let Some(joined) = set.join_next().await {
        let (idx, check, result, quiet) = match joined {
            Ok(v) => v,
            Err(e) => {
                eprintln!("goose review: check task panicked: {e}");
                continue;
            }
        };

        match result {
            Ok(findings) => {
                if !quiet {
                    eprintln!(
                        "goose review: check '{}' completed: {} finding(s)",
                        check.name,
                        findings.len()
                    );
                }
                results[idx] = findings;
            }
            Err(e) => {
                // Per-check failure must never abort the review — emit a
                // warning and continue with empty findings for this check.
                eprintln!("goose review: check '{}' failed: {e}", check.name);
                results[idx] = Vec::new();
            }
        }
    }

    results
}

/// Resolve which model a check should run on.
///
/// Precedence (most specific wins):
/// 1. `--override-model` always wins.
/// 2. If the user picked an explicit `--provider` on the CLI, drop the
///    per-check `model:` declaration entirely. The per-check model is
///    almost always pinned to a specific provider (e.g. a check that
///    asks for `goose-claude-4-sonnet` would 404 against Google's API),
///    so silently inheriting it across providers makes targeted reruns
///    fail. Use `--model` if set, otherwise fall through to the
///    selected provider's default.
/// 3. Per-check `model:` from frontmatter.
/// 4. `--model` (or the agent default).
fn resolve_check_model(check: &Check, opts: &ReviewOptions) -> Option<String> {
    if let Some(o) = opts.override_model.as_deref() {
        return Some(o.to_string());
    }
    if opts.provider.is_some() {
        return opts.default_model.clone();
    }
    if let Some(m) = check.model.as_deref() {
        return Some(m.to_string());
    }
    opts.default_model.clone()
}

/// Spawn a single `goose run` subprocess for one check and parse its
/// output into [`Finding`]s.
async fn run_single_check_subprocess(
    check: &Check,
    diff: &str,
    provider: Option<&str>,
    model: Option<&str>,
    instructions: Option<&str>,
    max_turns: Option<usize>,
) -> Result<Vec<Finding>> {
    let prompt = build_check_prompt(check, diff, instructions);
    let raw = run_subprocess_for_findings(
        &prompt,
        &format!("check '{}'", check.name),
        provider,
        model,
        max_turns,
    )
    .await?;
    let default_sev = check.severity_default.as_deref().unwrap_or("medium");
    Ok(raw
        .into_iter()
        .map(|r| Finding {
            severity: r.severity.unwrap_or_else(|| default_sev.to_string()),
            path: r.path.unwrap_or_default(),
            line_start: r.line_start.unwrap_or(0),
            line_end: r.line_end.unwrap_or(0),
            summary: r.summary.unwrap_or_default(),
            check: check.name.clone(),
        })
        .collect())
}

/// Generic `goose run` subprocess that hands a prompt to the model
/// and parses `{"findings": [...]}` JSON out of the response. Shared
/// by the per-check and per-file main-pass orchestrators so both get
/// the same robust JSON extraction, timeout handling, and error
/// reporting.
async fn run_subprocess_for_findings(
    prompt: &str,
    label: &str,
    provider: Option<&str>,
    model: Option<&str>,
    max_turns: Option<usize>,
) -> Result<Vec<RawFinding>> {
    let goose_bin = std::env::current_exe().context("locate current goose binary")?;

    let mut cmd = Command::new(&goose_bin);
    cmd.arg("run")
        .arg("--no-session")
        .arg("--quiet")
        .arg("--no-profile")
        .arg("-i")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Drop-on-cancel safety: when the outer `timeout` fires it drops
        // this future, which drops the Child handle. Without
        // kill_on_drop, the Tokio runtime leaves the subprocess running
        // (and racking up tokens) in the background — kill_on_drop sends
        // SIGKILL on Drop instead.
        .kill_on_drop(true);

    if let Some(p) = provider {
        cmd.arg("--provider").arg(p);
    }
    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }
    if let Some(t) = max_turns {
        cmd.arg("--max-turns").arg(t.to_string());
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn subprocess for {label}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .await
            .with_context(|| format!("write prompt to {label} stdin"))?;
        // Closing stdin signals EOF to `goose run -i -`.
        drop(stdin);
    }

    let wait = child.wait_with_output();
    let output = match timeout(Duration::from_secs(CHECK_TIMEOUT_SECS), wait).await {
        Ok(o) => o.with_context(|| format!("wait on {label}"))?,
        Err(_) => {
            anyhow::bail!("{label} timed out after {}s", CHECK_TIMEOUT_SECS);
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "{label} subprocess exited with status {}: {}",
            output.status,
            truncate(&stderr, 500)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_findings(&stdout)
}

/// Run the main correctness pass as N parallel subprocesses, one per
/// touched file. This replaces the older in-process `session.headless()`
/// path which:
///
/// 1. Streamed text-mode chatter to stdout (not JSONL) so findings were
///    sometimes lost in interleaved output.
/// 2. Sent the entire diff in a single prompt — large diffs (1000+
///    lines) reliably caused Gemini 3.x to short-circuit with `[]`
///    after ~30s instead of doing the work.
///
/// File-by-file fan-out keeps each subprocess context small enough that
/// the model actually walks every change, and runs them concurrently
/// so total wall clock stays close to the slowest single file rather
/// than scaling with diff size. Failures on one file never block the
/// others.
pub async fn run_main_pass_in_parallel(
    diff: &str,
    base_prompt: &str,
    opts: &ReviewOptions,
) -> Vec<Finding> {
    let per_file = split_diff_by_file(diff);
    if per_file.is_empty() {
        return Vec::new();
    }

    let semaphore = Arc::new(Semaphore::new(MAX_WORKERS));
    let mut set: JoinSet<(usize, String, Result<Vec<RawFinding>>, bool)> = JoinSet::new();

    for (idx, (path, file_diff)) in per_file.iter().enumerate() {
        let sem = semaphore.clone();
        let path = path.clone();
        let file_diff = file_diff.clone();
        let provider = opts.provider.clone();
        let model = opts.default_model.clone();
        let quiet = opts.quiet;
        let instructions = opts.instructions.clone();
        let base_prompt = base_prompt.to_string();

        set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore is never closed");
            let prompt =
                build_main_pass_prompt(&path, &file_diff, &base_prompt, instructions.as_deref());
            let label = format!("main:{path}");
            let result = run_subprocess_for_findings(
                &prompt,
                &label,
                provider.as_deref(),
                model.as_deref(),
                None,
            )
            .await;
            (idx, path, result, quiet)
        });
    }

    let mut per_file_results: Vec<Vec<Finding>> = vec![Vec::new(); per_file.len()];
    while let Some(joined) = set.join_next().await {
        let (idx, path, result, quiet) = match joined {
            Ok(v) => v,
            Err(e) => {
                eprintln!("goose review: main-pass task panicked: {e}");
                continue;
            }
        };
        match result {
            Ok(raw) => {
                let findings: Vec<Finding> = raw
                    .into_iter()
                    .map(|r| Finding {
                        severity: r.severity.unwrap_or_else(|| "medium".to_string()),
                        path: r.path.unwrap_or_else(|| path.clone()),
                        line_start: r.line_start.unwrap_or(0),
                        line_end: r.line_end.unwrap_or(0),
                        summary: r.summary.unwrap_or_default(),
                        check: "main".to_string(),
                    })
                    .collect();
                if !quiet {
                    eprintln!(
                        "goose review: main pass on '{}' completed: {} finding(s)",
                        path,
                        findings.len()
                    );
                }
                per_file_results[idx] = findings;
            }
            Err(e) => {
                // A single broken file must not abort the entire main
                // pass; surface a warning and continue.
                eprintln!("goose review: main pass on '{}' failed: {e}", path);
                per_file_results[idx] = Vec::new();
            }
        }
    }

    per_file_results.into_iter().flatten().collect()
}

/// Split a unified `git diff` into one chunk per file. Each chunk
/// starts at its `diff --git a/... b/...` header and runs to the
/// next file boundary. Returns `(repo_relative_path, file_diff)`
/// pairs in source order.
pub fn split_diff_by_file(diff: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_chunk = String::new();

    for line in diff.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            // Flush previous file.
            if let Some(p) = current_path.take() {
                if !current_chunk.is_empty() {
                    out.push((p, std::mem::take(&mut current_chunk)));
                } else {
                    current_chunk.clear();
                }
            }
            current_chunk.clear();
            current_chunk.push_str(line);
            // Header is `a/<path> b/<path>`. Pull `b/` (post-image)
            // because that's the path the model should reference for
            // line numbers.
            current_path = parse_diff_header_path(rest);
        } else {
            current_chunk.push_str(line);
        }
    }
    if let Some(p) = current_path {
        if !current_chunk.is_empty() {
            out.push((p, current_chunk));
        }
    }
    out
}

/// Parse the `a/<path> b/<path>` portion of a `diff --git` header line.
/// Returns the post-image path (`b/...`) when present; falls back to
/// the pre-image path (`a/...`) for deletions.
///
/// Also handles git's quoted form for paths with non-ASCII bytes,
/// spaces, or special chars (e.g. `"a/dir/\303\251.txt" "b/dir/\303\251.txt"`,
/// emitted whenever `core.quotePath` is on or the path contains
/// whitespace). Without quote handling, files under those paths are
/// silently dropped from the per-file main pass.
#[allow(clippy::string_slice)]
fn parse_diff_header_path(rest: &str) -> Option<String> {
    let trimmed = rest.trim_end_matches('\n').trim();

    if trimmed.starts_with('"') {
        let (a_quoted, after_a) = take_quoted(trimmed)?;
        let after_a = after_a.trim_start();
        let post = if after_a.starts_with('"') {
            let (b_quoted, _) = take_quoted(after_a)?;
            b_quoted
        } else {
            // Mixed form: quoted a/, unquoted b/.
            after_a.to_string()
        };
        return Some(
            strip_diff_prefix(&post)
                .unwrap_or_else(|| strip_diff_prefix(&a_quoted).unwrap_or(post)),
        );
    }

    if let Some(idx) = trimmed.find(" b/") {
        let post = &trimmed[idx + 3..];
        // The post-image path may itself be quoted in mixed-form
        // headers like `a/foo.txt "b/with space.txt"`.
        if post.starts_with('"') {
            let (q, _) = take_quoted(post)?;
            return Some(strip_diff_prefix(&q).unwrap_or(q));
        }
        return Some(post.to_string());
    }
    if let Some(stripped) = trimmed.strip_prefix("a/") {
        return Some(stripped.to_string());
    }
    None
}

/// Strip the leading `a/` or `b/` prefix that git puts on diff header
/// paths. Returns `None` if no such prefix exists.
fn strip_diff_prefix(s: &str) -> Option<String> {
    s.strip_prefix("a/")
        .or_else(|| s.strip_prefix("b/"))
        .map(str::to_string)
}

/// Pull a single C-style quoted token off the start of `s` and return
/// `(decoded, remainder)`. Decodes the escape sequences git uses when
/// `core.quotePath` is on: `\\`, `\"`, `\a`, `\b`, `\t`, `\n`, `\v`,
/// `\f`, `\r`, and octal `\NNN` byte escapes (for non-ASCII paths).
fn take_quoted(s: &str) -> Option<(String, &str)> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut decoded: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let rest = std::str::from_utf8(&bytes[i + 1..]).ok()?;
                let s = String::from_utf8(decoded).ok()?;
                return Some((s, rest));
            }
            b'\\' => {
                if i + 1 >= bytes.len() {
                    return None;
                }
                match bytes[i + 1] {
                    b'\\' => {
                        decoded.push(b'\\');
                        i += 2;
                    }
                    b'"' => {
                        decoded.push(b'"');
                        i += 2;
                    }
                    b'a' => {
                        decoded.push(0x07);
                        i += 2;
                    }
                    b'b' => {
                        decoded.push(0x08);
                        i += 2;
                    }
                    b't' => {
                        decoded.push(b'\t');
                        i += 2;
                    }
                    b'n' => {
                        decoded.push(b'\n');
                        i += 2;
                    }
                    b'v' => {
                        decoded.push(0x0b);
                        i += 2;
                    }
                    b'f' => {
                        decoded.push(0x0c);
                        i += 2;
                    }
                    b'r' => {
                        decoded.push(b'\r');
                        i += 2;
                    }
                    c if (b'0'..=b'7').contains(&c) => {
                        if i + 3 >= bytes.len() {
                            return None;
                        }
                        let octal = &bytes[i + 1..i + 4];
                        if !octal.iter().all(|b| (b'0'..=b'7').contains(b)) {
                            return None;
                        }
                        let val = ((octal[0] - b'0') as u16) * 64
                            + ((octal[1] - b'0') as u16) * 8
                            + (octal[2] - b'0') as u16;
                        decoded.push(val as u8);
                        i += 4;
                    }
                    _ => return None,
                }
            }
            b => {
                decoded.push(b);
                i += 1;
            }
        }
    }
    None
}

/// Build the strict, JSON-only prompt sent to one main-pass
/// subprocess. The base prompt (custom or
/// [`DEFAULT_REVIEW_PROMPT`]) supplies the reviewer voice; we then
/// pin the file under review and force a `{"findings": [...]}`
/// response so the orchestrator's parser can pick it up reliably.
fn build_main_pass_prompt(
    path: &str,
    file_diff: &str,
    base_prompt: &str,
    instructions: Option<&str>,
) -> String {
    let mut s = String::new();
    s.push_str(base_prompt.trim_end());
    s.push_str("\n\n");
    if let Some(text) = instructions {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            s.push_str("## Reviewer instructions\n\n");
            s.push_str(trimmed);
            s.push_str("\n\n");
        }
    }
    s.push_str("## File under review\n\n");
    s.push_str(&format!("Path: `{path}`\n\n"));
    s.push_str(
        "Review ONLY the changes in this file. Walk every added/modified line. \
         Do not flag pre-existing code shown for context (lines beginning with a space). \
         Use post-change line numbers from the diff.\n\n",
    );
    s.push_str(
        "## Output\n\nReturn ONLY valid JSON with this exact schema:\n\n\
{\n  \"findings\": [\n    {\n      \"severity\": \"low|medium|high|critical\",\n      \"path\": \"relative/path/to/file\",\n      \"line_start\": 10,\n      \"line_end\": 12,\n      \"summary\": \"One-paragraph actionable explanation of the issue and the fix\"\n    }\n  ]\n}\n\nIf there are no real issues, return:\n{\"findings\":[]}\n\nDo NOT include any text before or after the JSON. Do NOT wrap the JSON in code fences.\n\n",
    );
    s.push_str("## Diff\n\n```diff\n");
    s.push_str(file_diff.trim_end_matches('\n'));
    s.push_str("\n```\n");
    s
}

/// Build the strict, tool-free prompt sent to one check subprocess.
///
/// Shape matches the prompt format Amp-authored checks already expect,
/// so a check written for `amp review` runs the same way under
/// `goose review`.
fn build_check_prompt(check: &Check, diff: &str, instructions: Option<&str>) -> String {
    let mut s = String::new();
    s.push_str("You are running an automated code review check.\n\n");
    s.push_str(&format!("Check name: {}\n", check.name));
    if let Some(d) = check.description.as_deref() {
        if !d.is_empty() {
            s.push_str(&format!("Description: {}\n", d));
        }
    }
    if let Some(sev) = check.severity_default.as_deref() {
        if !sev.is_empty() {
            s.push_str(&format!("Default severity: {}\n", sev));
        }
    }
    if let Some(text) = instructions {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            s.push_str("\nReviewer instructions:\n");
            s.push_str(trimmed);
            s.push('\n');
        }
    }
    s.push_str("\nReview ONLY the git diff provided below.\n");
    s.push_str("Do not ask for missing context.\n");
    s.push_str("Use repo-relative file paths.\n");
    s.push_str("Use post-change line numbers from the diff.\n");
    s.push_str("If you cannot map an issue to a specific line range in the diff, use line_start: 0 and line_end: 0.\n");
    // Amp's check prompt emphasizes this twice; without it, models
    // routinely flag pre-existing code that just happens to appear in
    // the diff context (lines starting with a space, not `+`).
    s.push_str(
        "Search for patterns described above ONLY in the changed lines (lines beginning with `+` in the diff).\n",
    );
    s.push_str("Report issues ONLY for code that was added or modified in this diff.\n");
    s.push_str("Do NOT report issues for unchanged/pre-existing code shown for context.\n\n");
    s.push_str(
        "Return ONLY valid JSON with this exact schema:\n\
{\n  \"findings\": [\n    {\n      \"severity\": \"low|medium|high|critical\",\n      \"path\": \"relative/path/to/file\",\n      \"line_start\": 10,\n      \"line_end\": 12,\n      \"summary\": \"One-sentence actionable issue\"\n    }\n  ]\n}\n\nIf there are no issues, return:\n{\"findings\":[]}\n\nDo NOT include any text before or after the JSON. Do NOT wrap the JSON in code fences.\n\n",
    );
    s.push_str("Check instructions:\n\n");
    s.push_str(check.body.trim());
    s.push_str("\n\nDiff:\n\n```diff\n");
    s.push_str(diff.trim_end_matches('\n'));
    s.push_str("\n```\n");
    s
}

/// Pull the `findings` array out of an LLM response, tolerating code
/// fences and stray text the model occasionally inserts.
fn parse_findings(output: &str) -> Result<Vec<RawFinding>> {
    let stripped = strip_code_fences(output.trim());
    let json = extract_json_object(&stripped).unwrap_or(stripped);
    let resp: FindingsResponse = serde_json::from_str(&json)
        .with_context(|| format!("parse check JSON: {}", truncate(&json, 500)))?;
    Ok(resp.findings)
}

fn strip_code_fences(s: &str) -> String {
    let s = s.trim();
    if let Some(after_open) = s.strip_prefix("```") {
        let after_first_line = after_open
            .split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or("");
        let trimmed_close = after_first_line
            .rsplit_once("```")
            .map(|(before, _)| before)
            .unwrap_or(after_first_line);
        return trimmed_close.trim().to_string();
    }
    s.to_string()
}

#[allow(clippy::string_slice)]
fn extract_json_object(s: &str) -> Option<String> {
    // The structural characters we scan for (`"`, `\`, `{`, `}`) are
    // single-byte ASCII, so iterating char-by-char with byte offsets via
    // `char_indices` is safe even when the LLM's chatter around the JSON
    // contains multi-byte characters. The two slice operations below
    // (`s[start..]` and `s[start..=abs]`) only ever land on UTF-8 char
    // boundaries because `start` is from `find('{')` and `abs` is from
    // `char_indices`, both of which yield boundary offsets.
    let start = s.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in s[start..].char_indices() {
        let abs = start + i;
        if escaped {
            escaped = false;
            continue;
        }
        if in_string {
            if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=abs].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

#[allow(clippy::string_slice)]
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    // Find the largest char boundary at or before `max` so we never
    // bisect a multi-byte UTF-8 sequence (this string is usually a model
    // error / response excerpt).
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &s[..cut])
}

/// Emit findings as JSONL (one object per line) to stdout, matching
/// the format the in-process path produces. Findings whose severity
/// ranks below `min_severity` are suppressed; this mirrors Amp's
/// behavior of hiding `low` from the review output by default.
pub fn emit_findings(findings: &[Finding], min_severity: Severity) -> usize {
    let mut emitted = 0usize;
    for f in findings {
        if Severity::parse(&f.severity) < min_severity {
            continue;
        }
        // serde_json::to_string never fails for these owned strings.
        if let Ok(line) = serde_json::to_string(f) {
            println!("{line}");
            emitted += 1;
        }
    }
    emitted
}

/// Severity floor for finding display. Mirrors Amp's CLI behavior of
/// hiding `low` by default; pass `--severity low` to surface them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

impl Severity {
    /// Parse a severity string from a finding (`low`/`medium`/`high`/
    /// `critical`). Unrecognized strings are treated as `Medium` so
    /// odd-but-non-trivial findings still surface.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" | "info" | "note" => Severity::Low,
            "high" => Severity::High,
            "critical" | "crit" | "blocker" => Severity::Critical,
            _ => Severity::Medium,
        }
    }
}

impl std::str::FromStr for Severity {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Ok(Severity::Low),
            "medium" | "med" => Ok(Severity::Medium),
            "high" => Ok(Severity::High),
            "critical" => Ok(Severity::Critical),
            other => Err(format!(
                "unknown severity '{other}' (expected one of: low, medium, high, critical)"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ck(name: &str) -> Check {
        Check {
            name: name.to_string(),
            description: Some("desc".into()),
            model: None,
            turn_limit: None,
            tools: None,
            severity_default: None,
            path: PathBuf::from(format!("/.agents/checks/{name}.md")),
            scope_dir: String::new(),
            body: "look for bugs".into(),
        }
    }

    #[test]
    fn check_prompt_is_strict_and_diff_aware() {
        let p = build_check_prompt(&ck("perf"), "diff content", None);
        assert!(p.contains("automated code review check"));
        assert!(p.contains("Check name: perf"));
        assert!(p.contains("```diff\ndiff content\n```"));
        assert!(p.contains("Return ONLY valid JSON"));
        assert!(p.contains("look for bugs"));
        assert!(!p.contains("Reviewer instructions"));
    }

    #[test]
    fn check_prompt_restricts_findings_to_added_or_modified_lines() {
        // Mirrors Amp's prompt language; without these the model
        // happily flags pre-existing code shown for context.
        let p = build_check_prompt(&ck("perf"), "diff content", None);
        assert!(p.contains("ONLY in the changed lines"));
        assert!(p.contains("lines beginning with `+`"));
        assert!(p.contains("ONLY for code that was added or modified"));
        assert!(p.contains("Do NOT report issues for unchanged"));
    }

    #[test]
    fn check_prompt_includes_reviewer_instructions_when_provided() {
        let p = build_check_prompt(
            &ck("perf"),
            "diff content",
            Some("This is a refactor; flag any behavior change."),
        );
        assert!(p.contains("Reviewer instructions:"));
        assert!(p.contains("flag any behavior change"));
    }

    #[test]
    fn check_prompt_skips_blank_reviewer_instructions() {
        let p = build_check_prompt(&ck("perf"), "diff content", Some("   \n  "));
        assert!(!p.contains("Reviewer instructions"));
    }

    #[test]
    fn parse_findings_accepts_bare_json() {
        let raw = r#"{"findings":[{"severity":"high","path":"a.py","line_start":1,"line_end":2,"summary":"bad"}]}"#;
        let f = parse_findings(raw).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].path.as_deref(), Some("a.py"));
    }

    #[test]
    fn parse_findings_strips_code_fences() {
        let raw = "```json\n{\"findings\":[]}\n```";
        let f = parse_findings(raw).unwrap();
        assert!(f.is_empty());
    }

    #[test]
    fn parse_findings_extracts_object_when_model_adds_chatter() {
        let raw =
            "Sure, here are the findings:\n{\"findings\":[]}\n\nLet me know if you need more.";
        let f = parse_findings(raw).unwrap();
        assert!(f.is_empty());
    }

    #[test]
    fn extract_json_object_respects_string_braces() {
        let raw = r#"{"a": "value with } brace", "b": 1}"#;
        let extracted = extract_json_object(raw).unwrap();
        assert_eq!(extracted, raw);
    }

    #[test]
    fn resolve_check_model_prefers_override() {
        let check = ck("perf");
        let mut c = check.clone();
        c.model = Some("per-check".into());
        let opts = ReviewOptions {
            override_model: Some("OVERRIDE".into()),
            default_model: Some("default".into()),
            ..ReviewOptions::default()
        };
        assert_eq!(resolve_check_model(&c, &opts).as_deref(), Some("OVERRIDE"));
    }

    #[test]
    fn resolve_check_model_falls_through_to_per_check_then_default() {
        let mut c = ck("perf");
        c.model = Some("per-check".into());
        let opts = ReviewOptions {
            default_model: Some("default".into()),
            ..ReviewOptions::default()
        };
        assert_eq!(resolve_check_model(&c, &opts).as_deref(), Some("per-check"));

        let c = ck("perf");
        let opts = ReviewOptions {
            default_model: Some("default".into()),
            ..ReviewOptions::default()
        };
        assert_eq!(resolve_check_model(&c, &opts).as_deref(), Some("default"));
    }

    #[test]
    fn severity_orders_correctly() {
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
        assert!(Severity::High < Severity::Critical);
    }

    #[test]
    fn severity_parse_from_finding_string_is_lenient() {
        assert_eq!(Severity::parse("low"), Severity::Low);
        assert_eq!(Severity::parse("LOW"), Severity::Low);
        assert_eq!(Severity::parse("info"), Severity::Low);
        assert_eq!(Severity::parse("note"), Severity::Low);
        assert_eq!(Severity::parse("medium"), Severity::Medium);
        assert_eq!(Severity::parse("high"), Severity::High);
        assert_eq!(Severity::parse("critical"), Severity::Critical);
        assert_eq!(Severity::parse("blocker"), Severity::Critical);
        // Unknown strings default to Medium so they still surface.
        assert_eq!(Severity::parse("weird"), Severity::Medium);
        assert_eq!(Severity::parse(""), Severity::Medium);
    }

    #[test]
    fn severity_from_str_strict_for_cli() {
        use std::str::FromStr;
        assert_eq!(Severity::from_str("low").unwrap(), Severity::Low);
        assert_eq!(Severity::from_str("MEDIUM").unwrap(), Severity::Medium);
        assert_eq!(Severity::from_str("med").unwrap(), Severity::Medium);
        assert_eq!(Severity::from_str("high").unwrap(), Severity::High);
        assert_eq!(Severity::from_str("critical").unwrap(), Severity::Critical);
        assert!(Severity::from_str("info").is_err());
        assert!(Severity::from_str("").is_err());
    }

    #[test]
    fn resolve_check_model_cli_provider_wins_over_per_check_model() {
        let mut c = ck("perf");
        c.model = Some("goose-claude-4-sonnet".into()); // wrong provider
        let opts = ReviewOptions {
            provider: Some("google".into()),
            default_model: Some("gemini-3.1-pro-preview".into()),
            ..ReviewOptions::default()
        };
        assert_eq!(
            resolve_check_model(&c, &opts).as_deref(),
            Some("gemini-3.1-pro-preview")
        );
    }

    #[test]
    fn resolve_check_model_cli_provider_alone_drops_per_check_model() {
        // `--provider google` without `--model`: a per-check model pinned
        // to a Claude/Databricks model would 404 against Google. Drop it.
        let mut c = ck("perf");
        c.model = Some("goose-claude-4-sonnet".into());
        let opts = ReviewOptions {
            provider: Some("google".into()),
            default_model: None,
            ..ReviewOptions::default()
        };
        assert_eq!(resolve_check_model(&c, &opts), None);
    }

    #[test]
    fn split_diff_by_file_separates_files_in_source_order() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
index 1111..2222 100644
--- a/foo.rs
+++ b/foo.rs
@@ -1 +1 @@
-old foo
+new foo
diff --git a/bar/baz.go b/bar/baz.go
index 3333..4444 100644
--- a/bar/baz.go
+++ b/bar/baz.go
@@ -10 +10 @@
-old baz
+new baz
";
        let chunks = split_diff_by_file(diff);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].0, "foo.rs");
        assert!(chunks[0].1.starts_with("diff --git a/foo.rs b/foo.rs"));
        assert!(chunks[0].1.contains("+new foo"));
        // Each chunk must end at the next `diff --git` boundary, never
        // leak into the following file's body.
        assert!(!chunks[0].1.contains("baz.go"));
        assert_eq!(chunks[1].0, "bar/baz.go");
        assert!(chunks[1].1.contains("+new baz"));
    }

    #[test]
    fn split_diff_by_file_handles_single_file() {
        let diff = "\
diff --git a/only.py b/only.py
index aaa..bbb 100644
--- a/only.py
+++ b/only.py
@@ -1 +1 @@
-x = 1
+x = 2
";
        let chunks = split_diff_by_file(diff);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "only.py");
    }

    #[test]
    fn split_diff_by_file_returns_empty_for_empty_input() {
        assert!(split_diff_by_file("").is_empty());
        assert!(split_diff_by_file("\n").is_empty());
    }

    #[test]
    fn split_diff_by_file_handles_quoted_headers_with_octal_escapes() {
        // git emits `"a/dir/\303\251.txt" "b/dir/\303\251.txt"` for paths
        // containing non-ASCII bytes when core.quotePath is on. Without
        // quote handling the chunk gets dropped from the per-file pass.
        let diff = "\
diff --git \"a/dir/\\303\\251.txt\" \"b/dir/\\303\\251.txt\"
index 1111..2222 100644
--- \"a/dir/\\303\\251.txt\"
+++ \"b/dir/\\303\\251.txt\"
@@ -1 +1 @@
-old
+new
";
        let chunks = split_diff_by_file(diff);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "dir/\u{e9}.txt");
    }

    #[test]
    fn split_diff_by_file_handles_quoted_header_with_space() {
        let diff = "\
diff --git \"a/with space.txt\" \"b/with space.txt\"
index aaa..bbb 100644
--- \"a/with space.txt\"
+++ \"b/with space.txt\"
@@ -1 +1 @@
-old
+new
";
        let chunks = split_diff_by_file(diff);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "with space.txt");
    }

    #[test]
    fn split_diff_by_file_picks_post_image_path_for_renames() {
        let diff = "\
diff --git a/old/name.rs b/new/name.rs
similarity index 100%
rename from old/name.rs
rename to new/name.rs
";
        let chunks = split_diff_by_file(diff);
        assert_eq!(chunks.len(), 1);
        // We use the post-image path (b/) so the model references the
        // file under its new name when emitting line numbers.
        assert_eq!(chunks[0].0, "new/name.rs");
    }

    #[test]
    fn main_pass_prompt_pins_file_and_demands_strict_json() {
        let p = build_main_pass_prompt(
            "src/foo.rs",
            "diff --git a/src/foo.rs b/src/foo.rs\n@@ -1 +1 @@\n-old\n+new\n",
            "BASE PROMPT",
            None,
        );
        assert!(p.starts_with("BASE PROMPT"));
        assert!(p.contains("Path: `src/foo.rs`"));
        assert!(p.contains("Walk every added/modified line"));
        assert!(p.contains("\"findings\""));
        assert!(p.contains("Return ONLY valid JSON"));
        assert!(p.contains("Do NOT include any text before or after the JSON"));
        assert!(p.contains("```diff\ndiff --git a/src/foo.rs"));
        assert!(!p.contains("Reviewer instructions"));
    }

    #[test]
    fn main_pass_prompt_includes_reviewer_instructions_when_provided() {
        let p = build_main_pass_prompt(
            "src/foo.rs",
            "diff body",
            "BASE",
            Some("PR is a refactor; flag behavior changes."),
        );
        assert!(p.contains("## Reviewer instructions"));
        assert!(p.contains("flag behavior changes"));
    }

    #[test]
    fn main_pass_prompt_skips_blank_reviewer_instructions() {
        let p = build_main_pass_prompt("src/foo.rs", "diff body", "BASE", Some("   \n  \t\n"));
        assert!(!p.contains("Reviewer instructions"));
    }
}
