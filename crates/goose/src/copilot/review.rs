use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use super::{CopilotPrefs, ReviewModelChoice, ReviewOutputStyle};

#[derive(Debug, Deserialize, Clone)]
pub struct Finding {
    pub severity: String,
    pub path: String,
    #[serde(default)]
    pub line_start: i64,
    #[serde(default)]
    pub line_end: i64,
    pub summary: String,
    #[serde(default)]
    pub check: String,
    #[serde(default)]
    pub suggestion: Option<String>,
}

#[derive(Clone, Copy)]
pub struct ReviewPublishContext<'a> {
    pub pr_url: &'a str,
    pub head_sha: &'a str,
}

pub fn parse_findings(stdout: &str) -> Result<Vec<Finding>> {
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

pub fn build_goose_review_args(base: &str, head: &str, prefs: &CopilotPrefs) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "review".into(),
        format!("{base}...{head}"),
        "--severity".into(),
        prefs.review_severity.as_cli_flag().into(),
        "--quiet".into(),
    ];
    if matches!(prefs.review_model_choice, ReviewModelChoice::Custom) {
        if let Some(provider) = prefs.review_provider.as_deref().filter(|s| !s.is_empty()) {
            args.push("--provider".into());
            args.push(provider.to_string());
        }
        if let Some(model) = prefs.review_model.as_deref().filter(|s| !s.is_empty()) {
            args.push("--model".into());
            args.push(model.to_string());
        }
    }
    let instructions = prefs.custom_instructions.trim();
    if !instructions.is_empty() {
        args.push("--instructions".into());
        args.push(instructions.to_string());
    }
    args
}

pub fn build_review_payload(
    ctx: ReviewPublishContext<'_>,
    findings: &[Finding],
    style: &ReviewOutputStyle,
) -> serde_json::Value {
    let include_inline = matches!(style, ReviewOutputStyle::Inline | ReviewOutputStyle::Both);
    let include_summary = matches!(style, ReviewOutputStyle::Summary | ReviewOutputStyle::Both);

    let comments: Vec<serde_json::Value> = if include_inline {
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
                let mut body = format!(
                    "**{}** ({}) — {}",
                    f.severity.to_ascii_uppercase(),
                    if f.check.is_empty() { "main" } else { &f.check },
                    f.summary
                );
                if let Some(code) = f
                    .suggestion
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    body.push_str("\n\n```suggestion\n");
                    body.push_str(code);
                    body.push_str("\n```");
                }
                serde_json::json!({
                    "path": f.path,
                    "line": line,
                    "side": "RIGHT",
                    "body": body,
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    let summary = if include_summary {
        let mut s = format!(
            "**[goose Copilot]({})** — {} finding(s)",
            ctx.pr_url,
            findings.len()
        );
        if !findings.is_empty() {
            s.push_str(":\n\n");
            for f in findings.iter().take(10) {
                s.push_str(&format!(
                    "- **{}** in `{}`: {}\n",
                    f.severity, f.path, f.summary
                ));
            }
            if findings.len() > 10 {
                s.push_str(&format!("…and {} more.\n", findings.len() - 10));
            }
        }
        if !include_inline {
            s.push_str("\n_(summary-only mode; inline annotations disabled.)_");
        }
        s
    } else {
        format!(
            "[goose Copilot]({}) — {} inline finding(s).",
            ctx.pr_url,
            findings.len()
        )
    };

    serde_json::json!({
        "commit_id": ctx.head_sha,
        "body": summary,
        "event": "COMMENT",
        "comments": comments,
    })
}

pub fn extract_final_assistant_text(stdout: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct RunOutput {
        messages: Vec<RunMessage>,
    }
    #[derive(Deserialize)]
    struct RunMessage {
        role: String,
        #[serde(default)]
        content: Vec<serde_json::Value>,
    }

    let parsed: RunOutput = serde_json::from_str(stdout)?;
    let last = parsed
        .messages
        .iter()
        .rev()
        .find(|m| m.role.eq_ignore_ascii_case("assistant"))
        .ok_or_else(|| anyhow!("no assistant message in goose run output"))?;
    let text: String = last
        .content
        .iter()
        .filter_map(|c| c.get("text").and_then(|t| t.as_str()).map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.trim().is_empty() {
        bail!("assistant message had no text content");
    }
    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ctx() -> ReviewPublishContext<'static> {
        ReviewPublishContext {
            pr_url: "https://github.com/o/r/pull/1",
            head_sha: "h",
        }
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
        let findings = vec![Finding {
            severity: "high".into(),
            path: "a.rs".into(),
            line_start: 1,
            line_end: 2,
            summary: "bug".into(),
            check: "main".into(),
            suggestion: None,
        }];
        let payload = build_review_payload(sample_ctx(), &findings, &ReviewOutputStyle::Both);
        let comments = payload["comments"].as_array().unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0]["path"], "a.rs");
        assert!(!comments[0]["body"]
            .as_str()
            .unwrap()
            .contains("```suggestion"));
    }

    #[test]
    fn build_review_payload_inline_emits_suggestion_block() {
        let findings = vec![Finding {
            severity: "medium".into(),
            path: "a.py".into(),
            line_start: 4,
            line_end: 4,
            summary: "mutable default arg".into(),
            check: "main".into(),
            suggestion: Some("def add_tag(tag, tags=None):".into()),
        }];
        let payload = build_review_payload(sample_ctx(), &findings, &ReviewOutputStyle::Both);
        let body = payload["comments"][0]["body"].as_str().unwrap();
        assert!(body.contains("```suggestion\ndef add_tag(tag, tags=None):\n```"));
    }

    #[test]
    fn build_review_payload_summary_only_drops_comments() {
        let findings = vec![Finding {
            severity: "high".into(),
            path: "a.rs".into(),
            line_start: 1,
            line_end: 2,
            summary: "bug".into(),
            check: "main".into(),
            suggestion: None,
        }];
        let payload = build_review_payload(sample_ctx(), &findings, &ReviewOutputStyle::Summary);
        assert!(payload["comments"].as_array().unwrap().is_empty());
        assert!(payload["body"]
            .as_str()
            .unwrap()
            .contains("summary-only mode"));
    }

    #[test]
    fn build_review_payload_inline_only_drops_summary_body() {
        let findings = vec![Finding {
            severity: "high".into(),
            path: "a.rs".into(),
            line_start: 1,
            line_end: 2,
            summary: "bug".into(),
            check: "main".into(),
            suggestion: None,
        }];
        let payload = build_review_payload(sample_ctx(), &findings, &ReviewOutputStyle::Inline);
        assert_eq!(payload["comments"].as_array().unwrap().len(), 1);
        let body = payload["body"].as_str().unwrap();
        assert!(body.contains("1 inline finding(s)"), "got body: {body}");
        assert!(!body.contains("- **high**"));
    }

    #[test]
    fn review_args_defaults() {
        let args = build_goose_review_args("base", "head", &CopilotPrefs::default());
        assert_eq!(
            args,
            vec!["review", "base...head", "--severity", "medium", "--quiet"]
        );
    }

    #[test]
    fn review_args_severity_reflects_pref() {
        use crate::copilot::ReviewSeverity;
        let prefs = CopilotPrefs {
            review_severity: ReviewSeverity::High,
            ..Default::default()
        };
        let args = build_goose_review_args("base", "head", &prefs);
        let pos = args.iter().position(|a| a == "--severity").unwrap();
        assert_eq!(args[pos + 1], "high");
    }

    #[test]
    fn review_args_threads_custom_instructions() {
        let prefs = CopilotPrefs {
            custom_instructions: "Be strict on missing tests.".into(),
            ..Default::default()
        };
        let args = build_goose_review_args("base", "head", &prefs);
        let pos = args.iter().position(|a| a == "--instructions").unwrap();
        assert_eq!(args[pos + 1], "Be strict on missing tests.");
    }

    #[test]
    fn extract_final_assistant_text_picks_last_assistant() {
        let stdout = r#"{
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": [{"type": "tool_use", "name": "shell"}]},
                {"role": "tool", "content": [{"type": "text", "text": "tool output"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "final reply"}]}
            ]
        }"#;
        let text = extract_final_assistant_text(stdout).unwrap();
        assert_eq!(text, "final reply");
    }
}
