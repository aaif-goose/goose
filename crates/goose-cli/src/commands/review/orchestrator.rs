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

use super::check::Check;
use super::handler::ReviewOptions;

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
/// 2. If the user picked an explicit `--provider`+`--model` combo on the
///    CLI, prefer it over a per-check `model:` declaration. The per-check
///    model is usually written for a different provider (e.g. a check
///    pinned to `goose-claude-4-sonnet` on Databricks would 404 against
///    Google's API). Honoring the CLI provider here keeps benchmarks and
///    targeted reruns apples-to-apples.
/// 3. Per-check `model:` from frontmatter.
/// 4. `--model` (or the agent default).
fn resolve_check_model(check: &Check, opts: &ReviewOptions) -> Option<String> {
    if let Some(o) = opts.override_model.as_deref() {
        return Some(o.to_string());
    }
    if opts.provider.is_some() && opts.default_model.is_some() {
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
) -> Result<Vec<Finding>> {
    let prompt = build_check_prompt(check, diff, instructions);

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
        .stderr(Stdio::piped());

    if let Some(p) = provider {
        cmd.arg("--provider").arg(p);
    }
    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn check subprocess for '{}'", check.name))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .await
            .with_context(|| format!("write prompt to check '{}' stdin", check.name))?;
        // Closing stdin signals EOF to `goose run -i -`.
        drop(stdin);
    }

    let wait = child.wait_with_output();
    let output = match timeout(Duration::from_secs(CHECK_TIMEOUT_SECS), wait).await {
        Ok(o) => o.with_context(|| format!("wait on check '{}'", check.name))?,
        Err(_) => {
            anyhow::bail!(
                "check '{}' timed out after {}s",
                check.name,
                CHECK_TIMEOUT_SECS
            );
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "check '{}' subprocess exited with status {}: {}",
            check.name,
            output.status,
            truncate(&stderr, 500)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw = parse_findings(&stdout)?;
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
    s.push_str("If you cannot map an issue to a specific line range in the diff, use line_start: 0 and line_end: 0.\n\n");
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
        let after_first_line = after_open.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
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
/// the format the in-process path produces.
pub fn emit_findings(findings: &[Finding]) {
    for f in findings {
        // serde_json::to_string never fails for these owned strings.
        if let Ok(line) = serde_json::to_string(f) {
            println!("{line}");
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
}
