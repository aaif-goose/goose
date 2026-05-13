use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Default maximum number of turns a check subagent may take.
///
/// Mirrors `goose::agents::subagent_task_config::DEFAULT_SUBAGENT_MAX_TURNS`,
/// duplicated here to keep the review module self-contained for parsing.
pub const DEFAULT_CHECK_TURN_LIMIT: usize = 25;

/// A parsed check definition from `**/.agents/checks/*.md`.
///
/// Each check is a Markdown file with YAML frontmatter and a body of
/// natural-language instructions for the subagent reviewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Identifier for the check. Must match the file's basename (without `.md`).
    pub name: String,
    /// Brief description shown when listing checks.
    pub description: Option<String>,
    /// Per-check model override. Resolved against CLI flags by [`Check::resolved_model`].
    pub model: Option<String>,
    /// Per-check turn limit. Resolved against the global default by [`Check::resolved_turn_limit`].
    pub turn_limit: Option<usize>,
    /// Optional allowlist of tool names the check subagent may call.
    /// `None` (the default) means the subagent inherits the agent's full toolset.
    /// Mirrors Amp's `tools:` field for parity with `.agents/checks/*.md`.
    pub tools: Option<Vec<String>>,
    /// Optional default severity for findings emitted by this check
    /// (`low`, `medium`, `high`, `critical`). Recognized for parity with
    /// Amp's `severity-default:` field; the agent is told to use this
    /// when the check's body does not specify one.
    pub severity_default: Option<String>,
    /// Absolute path to the check file on disk.
    pub path: PathBuf,
    /// Repo-relative scope directory the check applies to. Empty string means
    /// the repo root or a global location.
    pub scope_dir: String,
    /// Markdown content after the closing `---`.
    pub body: String,
}

/// Find the `---` line that closes the YAML frontmatter, tolerating CRLF
/// line endings produced by editors on Windows. Returns `(frontmatter, body)`.
//
// All index arithmetic here is on ASCII bytes (`-`, `\n`, `\r`), which
// always land on UTF-8 character boundaries, so direct byte slicing is
// safe even when the body contains multi-byte characters.
#[allow(clippy::string_slice)]
fn split_frontmatter(after_open: &str) -> Option<(&str, &str)> {
    let mut search_from = 0;
    while let Some(rel) = after_open[search_from..].find("---") {
        let pos = search_from + rel;
        let preceded_by_lf = pos > 0 && after_open.as_bytes()[pos - 1] == b'\n';
        let preceded_by_crlf = pos >= 2
            && after_open.as_bytes()[pos - 2] == b'\r'
            && after_open.as_bytes()[pos - 1] == b'\n';
        if preceded_by_lf || preceded_by_crlf {
            let frontmatter_end = if preceded_by_crlf { pos - 2 } else { pos - 1 };
            let body_start = pos + 3;
            return Some((&after_open[..frontmatter_end], &after_open[body_start..]));
        }
        search_from = pos + 3;
    }
    None
}

#[derive(Debug, Deserialize, Default)]
struct CheckFrontmatter {
    name: Option<String>,
    description: Option<String>,
    model: Option<String>,
    #[serde(rename = "turn-limit")]
    turn_limit: Option<usize>,
    tools: Option<Vec<String>>,
    #[serde(rename = "severity-default")]
    severity_default: Option<String>,
}

impl Check {
    /// Read and parse a check file from disk.
    pub fn from_path(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("read check file: {}", path.display()))?;
        Self::parse(&content, path)
    }

    /// Parse a check from raw content.
    pub fn parse(content: &str, path: &Path) -> Result<Self> {
        let trimmed = content.trim_start();
        let after_open = trimmed
            .strip_prefix("---")
            .ok_or_else(|| {
                anyhow!(
                    "check {}: missing frontmatter (must start with ---)",
                    path.display()
                )
            })?
            .trim_start_matches(['\r', '\n']);

        let (frontmatter_raw, body_raw) = split_frontmatter(after_open).ok_or_else(|| {
            anyhow!(
                "check {}: missing closing --- in frontmatter",
                path.display()
            )
        })?;
        let body = body_raw.trim().to_string();

        let frontmatter: CheckFrontmatter = serde_yaml::from_str(frontmatter_raw)
            .with_context(|| format!("check {}: invalid frontmatter YAML", path.display()))?;

        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("check {}: invalid filename", path.display()))?
            .to_string();

        let name = match frontmatter.name {
            Some(declared) if !declared.is_empty() => declared,
            _ => file_stem,
        };

        Ok(Check {
            name,
            description: frontmatter.description,
            model: frontmatter.model,
            turn_limit: frontmatter.turn_limit,
            tools: frontmatter.tools,
            severity_default: frontmatter.severity_default,
            path: path.to_path_buf(),
            scope_dir: String::new(),
            body,
        })
    }

    /// Verify the check's `name` matches its filename stem (e.g. `perf` for
    /// `perf.md`). Repo-local checks are required to satisfy this rule so
    /// authors get a clear error early; checks loaded from global directories
    /// are allowed to drift for compatibility with cross-tool conventions.
    pub fn validate_name_matches_filename(&self) -> Result<()> {
        let stem = self
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("check {}: invalid filename", self.path.display()))?;
        if self.name != stem {
            bail!(
                "check {}: name '{}' must match filename '{}'",
                self.path.display(),
                self.name,
                stem
            );
        }
        Ok(())
    }

    /// Resolve which model this check should use.
    ///
    /// `override_model` (CLI `--override-model`) wins over everything; otherwise
    /// the per-check `model` wins; otherwise `default_model` (CLI `--model`).
    pub fn resolved_model<'a>(
        &'a self,
        default_model: Option<&'a str>,
        override_model: Option<&'a str>,
    ) -> Option<&'a str> {
        if let Some(m) = override_model {
            return Some(m);
        }
        if let Some(m) = self.model.as_deref() {
            return Some(m);
        }
        default_model
    }

    /// Resolve the turn limit for this check.
    pub fn resolved_turn_limit(&self, default_turn_limit: Option<usize>) -> usize {
        self.turn_limit
            .or(default_turn_limit)
            .unwrap_or(DEFAULT_CHECK_TURN_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn parses_full_frontmatter() {
        let content = r#"---
name: perf
description: Flag perf regressions
model: claude-sonnet-4
turn-limit: 40
tools:
  - read
  - grep
---
Look for N+1 queries.
"#;
        let check = Check::parse(content, &p("/r/.agents/checks/perf.md")).unwrap();
        assert_eq!(check.name, "perf");
        assert_eq!(check.description.as_deref(), Some("Flag perf regressions"));
        assert_eq!(check.model.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(check.turn_limit, Some(40));
        assert_eq!(
            check.tools.as_deref(),
            Some(["read".to_string(), "grep".to_string()].as_slice())
        );
        assert_eq!(check.body, "Look for N+1 queries.");
    }

    #[test]
    fn tools_field_is_optional_for_backward_compatibility() {
        let content = "---\nname: legacy\n---\nbody";
        let check = Check::parse(content, &p("/r/.agents/checks/legacy.md")).unwrap();
        assert!(check.tools.is_none());
        assert!(check.severity_default.is_none());
    }

    #[test]
    fn parses_extended_frontmatter() {
        // Frontmatter using the optional `severity-default` and `tools` fields.
        let content = r#"---
name: untrusted-pr
description: Reviews PRs as untrusted input
severity-default: high
tools: [Bash, Read, Grep]
---

## Purpose
"#;
        let check = Check::parse(content, &p("/r/.agents/checks/untrusted-pr.md")).unwrap();
        assert_eq!(check.name, "untrusted-pr");
        assert_eq!(check.severity_default.as_deref(), Some("high"));
        assert_eq!(
            check.tools.as_deref(),
            Some(["Bash".to_string(), "Read".to_string(), "Grep".to_string()].as_slice())
        );
        assert!(check.model.is_none());
        assert!(check.turn_limit.is_none());
    }

    #[test]
    fn defaults_name_to_filename_stem() {
        let content = "---\ndescription: foo\n---\nbody";
        let check = Check::parse(content, &p("/r/.agents/checks/sql-safety.md")).unwrap();
        assert_eq!(check.name, "sql-safety");
    }

    #[test]
    fn parse_keeps_declared_name_for_global_compat() {
        let content = "---\nname: meta-review\n---\nbody";
        let check = Check::parse(content, &p("/global/checks/review.md")).unwrap();
        assert_eq!(check.name, "meta-review");
    }

    #[test]
    fn validate_name_matches_filename_rejects_mismatch() {
        let content = "---\nname: other\n---\nbody";
        let check = Check::parse(content, &p("/r/.agents/checks/perf.md")).unwrap();
        let err = check.validate_name_matches_filename().unwrap_err();
        assert!(err.to_string().contains("must match filename"));
    }

    #[test]
    fn validate_name_matches_filename_accepts_match() {
        let content = "---\nname: perf\n---\nbody";
        let check = Check::parse(content, &p("/r/.agents/checks/perf.md")).unwrap();
        check.validate_name_matches_filename().unwrap();
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let err = Check::parse("just a body", &p("/r/.agents/checks/x.md")).unwrap_err();
        assert!(err.to_string().contains("missing frontmatter"));
    }

    #[test]
    fn rejects_unclosed_frontmatter() {
        let err = Check::parse("---\nname: x\nno close", &p("/r/.agents/checks/x.md")).unwrap_err();
        assert!(err.to_string().contains("missing closing"));
    }

    #[test]
    fn parses_crlf_frontmatter() {
        let content = "---\r\nname: perf\r\ndescription: ok\r\n---\r\nbody line\r\n";
        let check = Check::parse(content, &p("/r/.agents/checks/perf.md")).unwrap();
        assert_eq!(check.name, "perf");
        assert_eq!(check.description.as_deref(), Some("ok"));
        assert_eq!(check.body, "body line");
    }

    #[test]
    fn resolves_model_precedence() {
        let mut check = Check::parse(
            "---\nname: x\nmodel: per-check\n---\n",
            &p("/r/.agents/checks/x.md"),
        )
        .unwrap();
        assert_eq!(
            check.resolved_model(Some("default"), None),
            Some("per-check")
        );
        assert_eq!(
            check.resolved_model(Some("default"), Some("override")),
            Some("override")
        );
        check.model = None;
        assert_eq!(check.resolved_model(Some("default"), None), Some("default"));
        assert_eq!(check.resolved_model(None, None), None);
    }

    #[test]
    fn resolves_turn_limit() {
        let mut check = Check::parse(
            "---\nname: x\nturn-limit: 7\n---\n",
            &p("/r/.agents/checks/x.md"),
        )
        .unwrap();
        assert_eq!(check.resolved_turn_limit(None), 7);
        assert_eq!(check.resolved_turn_limit(Some(99)), 7);
        check.turn_limit = None;
        assert_eq!(check.resolved_turn_limit(Some(99)), 99);
        assert_eq!(check.resolved_turn_limit(None), DEFAULT_CHECK_TURN_LIMIT);
    }
}
