use std::fmt::Write;

use goose::checks::{Check, DiscoveredReview};

/// The default review prompt embedded in the binary.
pub const DEFAULT_REVIEW_PROMPT: &str = include_str!("default_review_prompt.md");

/// Build the full prompt sent to the main review agent.
///
/// Layout:
///
/// ```text
/// <base prompt>
///
/// ## Checks
/// <check table + per-check bodies>
///
/// ## Diff
/// ```
///
/// `base_prompt` is either the embedded [`DEFAULT_REVIEW_PROMPT`] or a
/// caller-supplied prompt loaded from `--prompt`. Findings derived from a
/// `**/.agents/REVIEW.md` file appear here as virtual `repo-rules`-prefixed
/// checks so the agent can attribute them via the `check` field on each
/// JSON finding.
pub fn build_review_prompt(
    base_prompt: &str,
    discovered: &DiscoveredReview,
    diff: &str,
    default_model: Option<&str>,
    override_model: Option<&str>,
    default_turn_limit: Option<usize>,
) -> String {
    let mut out = String::new();
    out.push_str(base_prompt.trim_end());
    out.push_str("\n\n");

    if !discovered.checks.is_empty() {
        out.push_str("## Checks\n\n");
        out.push_str("Dispatch one subagent per check below. ");
        out.push_str(
            "Use the `model`, `turn_limit`, and `tools` columns when invoking each subagent. ",
        );
        out.push_str("`tools = *` means the subagent inherits the agent's full toolset. ");
        out.push_str(
            "Set the `check` field on each finding to the check's `name` so the originating \
             rule can be identified in the output.\n\n",
        );
        out.push_str(
            "| name | scope | model | turn_limit | tools | severity_default | description |\n",
        );
        out.push_str(
            "|------|-------|-------|------------|-------|------------------|-------------|\n",
        );
        for check in &discovered.checks {
            let scope = if check.scope_dir.is_empty() {
                "<root>".to_string()
            } else {
                check.scope_dir.clone()
            };
            let model = check
                .resolved_model(default_model, override_model)
                .unwrap_or("<agent default>");
            let turn_limit = check.resolved_turn_limit(default_turn_limit);
            let tools = match check.tools.as_ref() {
                Some(t) if !t.is_empty() => t.join(", "),
                _ => "*".to_string(),
            };
            let severity = check.severity_default.as_deref().unwrap_or("");
            let description = check.description.as_deref().unwrap_or("");
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} | {} |",
                escape_pipe(&check.name),
                escape_pipe(&scope),
                escape_pipe(model),
                turn_limit,
                escape_pipe(&tools),
                escape_pipe(severity),
                escape_pipe(description),
            );
        }
        out.push('\n');
        for check in &discovered.checks {
            append_check_body(&mut out, check);
        }
    }

    out.push_str("## Diff\n\n");
    out.push_str("```diff\n");
    out.push_str(diff.trim_end_matches('\n'));
    out.push_str("\n```\n");
    out
}

fn append_check_body(out: &mut String, check: &Check) {
    let scope = if check.scope_dir.is_empty() {
        "<root>".to_string()
    } else {
        check.scope_dir.clone()
    };
    let _ = writeln!(out, "### Check: {} (scope: {})", check.name, scope);
    out.push_str(check.body.trim());
    out.push_str("\n\n");
}

fn escape_pipe(s: &str) -> String {
    s.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn check(name: &str, scope: &str, model: Option<&str>, turn_limit: Option<usize>) -> Check {
        Check {
            name: name.to_string(),
            description: Some(format!("desc-{name}")),
            model: model.map(str::to_string),
            turn_limit,
            tools: None,
            severity_default: None,
            path: PathBuf::from(format!("/r/{scope}/.agents/checks/{name}.md")),
            scope_dir: scope.to_string(),
            body: format!("body-{name}"),
        }
    }

    #[test]
    fn renders_checks_with_resolved_model_and_turn_limit() {
        let discovered = DiscoveredReview {
            checks: vec![
                check("perf", "", None, None),
                check("auth", "api", Some("m1"), Some(7)),
            ],
        };

        let prompt = build_review_prompt(
            "BASE",
            &discovered,
            "diff content",
            Some("default-model"),
            None,
            Some(20),
        );

        assert!(prompt.contains("| auth | api | m1 | 7 | * |"));
        assert!(prompt.contains("| perf | <root> | default-model | 20 | * |"));
        assert!(prompt.contains("body-auth"));
        assert!(prompt.contains("```diff\ndiff content\n```"));
    }

    #[test]
    fn override_model_wins_per_check() {
        let discovered = DiscoveredReview {
            checks: vec![check("perf", "", Some("per-check"), None)],
        };
        let prompt = build_review_prompt(
            "BASE",
            &discovered,
            "",
            Some("default"),
            Some("OVERRIDE"),
            None,
        );
        assert!(prompt.contains("| perf | <root> | OVERRIDE |"));
    }

    #[test]
    fn renders_tool_allowlist_and_severity_when_present() {
        let mut perf = check("perf", "", None, None);
        perf.tools = Some(vec!["read".to_string(), "grep".to_string()]);
        perf.severity_default = Some("high".into());
        let discovered = DiscoveredReview { checks: vec![perf] };
        let prompt = build_review_prompt("BASE", &discovered, "", None, None, None);
        assert!(prompt.contains("| perf | <root> | <agent default> | 25 | read, grep | high |"));
    }

    #[test]
    fn instructs_agent_to_attribute_findings_via_check_field() {
        let discovered = DiscoveredReview {
            checks: vec![check("perf", "", None, None)],
        };
        let prompt = build_review_prompt("BASE", &discovered, "", None, None, None);
        assert!(prompt.contains("Set the `check` field"));
    }

    #[test]
    fn omits_checks_section_when_empty() {
        let discovered = DiscoveredReview::default();
        let prompt = build_review_prompt("BASE", &discovered, "diff", None, None, None);
        assert!(!prompt.contains("## Checks"));
        assert!(prompt.contains("## Diff"));
    }
}
