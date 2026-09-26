use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use regex::Regex;
use tempfile::Builder;

use crate::config::base::Config;

const PLACEHOLDER: &str = "PASTE_KEY_HERE";

static SECRET_PATTERNS: std::sync::LazyLock<Vec<SecretPattern>> =
    std::sync::LazyLock::new(build_secret_patterns);

struct SecretPattern {
    name: &'static str,
    regex: Regex,
    key_hint: &'static str,
}

fn build_secret_patterns() -> Vec<SecretPattern> {
    vec![
        SecretPattern {
            name: "OpenAI API key",
            regex: Regex::new(r"sk-[a-zA-Z0-9_-]{20,}").unwrap(),
            key_hint: "OPENAI_API_KEY",
        },
        SecretPattern {
            name: "Anthropic API key",
            regex: Regex::new(r"sk-ant-[a-zA-Z0-9_-]{20,}").unwrap(),
            key_hint: "ANTHROPIC_API_KEY",
        },
        SecretPattern {
            name: "GitHub personal access token",
            regex: Regex::new(r"gh[ps]_[A-Za-z0-9_]{36,}").unwrap(),
            key_hint: "GITHUB_TOKEN",
        },
        SecretPattern {
            name: "GitHub OAuth token",
            regex: Regex::new(r"gho_[A-Za-z0-9_]{36,}").unwrap(),
            key_hint: "GITHUB_OAUTH_TOKEN",
        },
        SecretPattern {
            name: "AWS access key ID",
            regex: Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
            key_hint: "AWS_ACCESS_KEY_ID",
        },
        SecretPattern {
            name: "Slack token",
            regex: Regex::new(r"xox[bpras]-[A-Za-z0-9\-]+").unwrap(),
            key_hint: "SLACK_TOKEN",
        },
        SecretPattern {
            name: "Stripe key",
            regex: Regex::new(r"[sr]k_(test|live)_[A-Za-z0-9]{20,}").unwrap(),
            key_hint: "STRIPE_API_KEY",
        },
        SecretPattern {
            name: "Google API key",
            regex: Regex::new(r"AIza[0-9A-Za-z_-]{35}").unwrap(),
            key_hint: "GOOGLE_API_KEY",
        },
        SecretPattern {
            name: "Databricks token",
            regex: Regex::new(r"dapi[a-f0-9]{32}").unwrap(),
            key_hint: "DATABRICKS_TOKEN",
        },
        SecretPattern {
            name: "Bearer token",
            regex: Regex::new(r"Bearer\s+[A-Za-z0-9_\-.~+/]+=*").unwrap(),
            key_hint: "BEARER_TOKEN",
        },
        SecretPattern {
            name: "Hugging Face token",
            regex: Regex::new(r"hf_[A-Za-z0-9]{34,}").unwrap(),
            key_hint: "HF_TOKEN",
        },
    ]
}

#[derive(Debug, Clone)]
pub struct DetectedSecret {
    pub pattern_name: String,
    pub key_hint: String,
    pub matched_value: String,
    pub start: usize,
    pub end: usize,
}

pub fn detect_secrets(text: &str) -> Vec<DetectedSecret> {
    let mut found = Vec::new();
    for pattern in SECRET_PATTERNS.iter() {
        for m in pattern.regex.find_iter(text) {
            found.push(DetectedSecret {
                pattern_name: pattern.name.to_string(),
                key_hint: pattern.key_hint.to_string(),
                matched_value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }
    }
    found.sort_by_key(|d| d.start);
    found
}

pub fn redact_and_store_secrets(
    text: &str,
    secrets: &[DetectedSecret],
) -> Result<(String, Vec<(String, String)>)> {
    if secrets.is_empty() {
        return Ok((text.to_string(), Vec::new()));
    }

    let config = Config::global();
    let mut redacted = text.to_string();
    let mut stored: Vec<(String, String)> = Vec::new();

    // Process in reverse order so byte offsets remain valid
    for (i, secret) in secrets.iter().enumerate().rev() {
        let key_name = if secrets.len() == 1 {
            secret.key_hint.clone()
        } else {
            format!("{}_{}", secret.key_hint, i + 1)
        };

        // Check if a secret with this key already exists and has the same value
        let existing: Option<String> = config.get_secret(&key_name).ok();
        if existing.as_deref() != Some(&secret.matched_value) {
            config
                .set_secret(&key_name, &secret.matched_value)
                .with_context(|| format!("Failed to store secret as {key_name}"))?;
        }

        let marker = format!("[STORED_SECRET:{key_name}]");
        redacted.replace_range(secret.start..secret.end, &marker);
        stored.push((key_name, secret.pattern_name.clone()));
    }

    Ok((redacted, stored))
}

fn build_scaffold_content(keys: &[(&str, &str)]) -> String {
    let mut content = String::from(
        "# Goose Secret Input\n\
         # Paste your value(s) below each key name and save this file.\n\
         # Lines starting with # are ignored.\n\
         # The file will be deleted after reading.\n\n",
    );

    for (key_name, description) in keys {
        content.push_str(&format!("# {description}\n"));
        content.push_str(&format!("{key_name}={PLACEHOLDER}\n\n"));
    }

    content
}

fn parse_scaffold_content(content: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() && value != PLACEHOLDER {
                result.insert(key.to_string(), value.to_string());
            }
        }
    }
    result
}

fn resolve_editor() -> Option<String> {
    let config = Config::global();
    let config_editor = config.get_goose_prompt_editor().ok().flatten();
    let visual = std::env::var("VISUAL").ok();
    let editor_env = std::env::var("EDITOR").ok();

    [config_editor, visual, editor_env]
        .into_iter()
        .flatten()
        .find(|cmd| !cmd.is_empty())
}

fn split_editor_command(editor_cmd: &str) -> Result<Vec<String>> {
    shell_words::split(editor_cmd).map_err(|e| anyhow::anyhow!("Invalid editor command: {e}"))
}

fn launch_editor(editor_cmd: &str, file_path: &PathBuf) -> Result<()> {
    use std::process::Stdio;

    let parts = split_editor_command(editor_cmd)?;
    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty editor command"));
    }

    let mut cmd = Command::new(&parts[0]);
    if let Ok(cwd) = std::env::current_dir() {
        cmd.current_dir(cwd);
    }
    if parts.len() > 1 {
        cmd.args(&parts[1..]);
    }
    cmd.arg(file_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "Editor exited with non-zero status: {}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

fn secure_delete(path: &std::path::Path) -> Result<()> {
    if path.exists() {
        if let Ok(metadata) = std::fs::metadata(path) {
            let len = metadata.len() as usize;
            if len > 0 {
                if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(path) {
                    let zeros = vec![0u8; len];
                    let _ = file.write_all(&zeros);
                    let _ = file.flush();
                    let _ = file.sync_all();
                }
            }
        }
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn request_secrets_via_scaffold(keys: &[(&str, &str)]) -> Result<HashMap<String, String>> {
    let editor_cmd = resolve_editor().ok_or_else(|| {
        anyhow::anyhow!(
            "No editor configured. Set one with:\n  \
             goose configure set goose_prompt_editor \"vim\"\n  \
             or set $VISUAL or $EDITOR in your shell."
        )
    })?;

    let scaffold_content = build_scaffold_content(keys);

    let temp_file = Builder::new()
        .prefix("goose_secret_")
        .suffix(".txt")
        .tempfile()?;

    let temp_path = temp_file.path().to_path_buf();
    // Keep the file alive by persisting it (we manage deletion ourselves)
    let temp_path_persisted = temp_file.into_temp_path();

    std::fs::write(&temp_path, &scaffold_content)?;

    launch_editor(&editor_cmd, &temp_path)?;

    let mut content = String::new();
    let mut file = std::fs::File::open(&temp_path)?;
    file.read_to_string(&mut content)?;
    drop(file);

    let parsed = parse_scaffold_content(&content);

    // Overwrite then delete
    secure_delete(&temp_path)?;
    drop(temp_path_persisted);

    Ok(parsed)
}

pub fn request_and_store_secrets(keys: &[(&str, &str)]) -> Result<Vec<String>> {
    let values = request_secrets_via_scaffold(keys)?;
    let config = Config::global();
    let mut stored_keys = Vec::new();

    for (key, value) in &values {
        config
            .set_secret(key, value)
            .with_context(|| format!("Failed to store secret {key}"))?;
        stored_keys.push(key.clone());
    }

    Ok(stored_keys)
}

pub fn list_secret_names() -> Result<Vec<String>> {
    let config = Config::global();
    let secrets = config.all_secrets()?;
    let mut names: Vec<String> = secrets.keys().cloned().collect();
    names.sort();
    Ok(names)
}

pub fn delete_stored_secret(key: &str) -> Result<()> {
    let config = Config::global();
    config.delete_secret(key)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_openai_key() {
        let text = "my key is sk-proj-abc123xyz789abcdef012345";
        let found = detect_secrets(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key_hint, "OPENAI_API_KEY");
        assert_eq!(found[0].matched_value, "sk-proj-abc123xyz789abcdef012345");
    }

    #[test]
    fn detect_anthropic_key() {
        let text = "use sk-ant-api03-abcdefghijklmnopqrstuvwxyz";
        let found = detect_secrets(text);
        assert!(found.iter().any(|d| d.key_hint == "ANTHROPIC_API_KEY"));
    }

    #[test]
    fn detect_github_pat() {
        let text = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn";
        let found = detect_secrets(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key_hint, "GITHUB_TOKEN");
    }

    #[test]
    fn detect_aws_key() {
        let text = "AKIAIOSFODNN7EXAMPLE";
        let found = detect_secrets(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key_hint, "AWS_ACCESS_KEY_ID");
    }

    #[test]
    fn detect_slack_token() {
        // Use xoxb- prefix with enough chars to match but clearly fake
        let text = "xoxb-fake-test-token-not-real";
        let found = detect_secrets(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key_hint, "SLACK_TOKEN");
    }

    #[test]
    fn detect_multiple_secrets() {
        let text = "openai: sk-proj-abc123xyz789abcdef012345 and github: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn";
        let found = detect_secrets(text);
        assert_eq!(found.len(), 2);
        assert!(found[0].start < found[1].start);
    }

    #[test]
    fn no_false_positive_on_normal_text() {
        let text = "hello world, this is a normal message about programming";
        let found = detect_secrets(text);
        assert!(found.is_empty());
    }

    #[test]
    fn no_false_positive_on_short_sk_prefix() {
        let text = "sk-short";
        let found = detect_secrets(text);
        assert!(found.is_empty());
    }

    #[test]
    fn parse_scaffold_single_key() {
        let content = "# Description\nMY_KEY=actual_secret_value\n";
        let parsed = parse_scaffold_content(content);
        assert_eq!(parsed.get("MY_KEY").unwrap(), "actual_secret_value");
    }

    #[test]
    fn parse_scaffold_multiple_keys() {
        let content = "# Key 1\nKEY_A=value_a\n\n# Key 2\nKEY_B=value_b\n";
        let parsed = parse_scaffold_content(content);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed.get("KEY_A").unwrap(), "value_a");
        assert_eq!(parsed.get("KEY_B").unwrap(), "value_b");
    }

    #[test]
    fn parse_scaffold_ignores_placeholder() {
        let content = "MY_KEY=PASTE_KEY_HERE\n";
        let parsed = parse_scaffold_content(content);
        assert!(parsed.is_empty());
    }

    #[test]
    fn parse_scaffold_ignores_comments() {
        let content = "# this is a comment\n# another comment\nKEY=value\n";
        let parsed = parse_scaffold_content(content);
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn parse_scaffold_ignores_empty_values() {
        let content = "KEY=\n";
        let parsed = parse_scaffold_content(content);
        assert!(parsed.is_empty());
    }

    #[test]
    fn build_scaffold_single_key() {
        let content = build_scaffold_content(&[("API_KEY", "Your API key")]);
        assert!(content.contains("API_KEY=PASTE_KEY_HERE"));
        assert!(content.contains("# Your API key"));
    }

    #[test]
    fn build_scaffold_multiple_keys() {
        let content = build_scaffold_content(&[("KEY_A", "First key"), ("KEY_B", "Second key")]);
        assert!(content.contains("KEY_A=PASTE_KEY_HERE"));
        assert!(content.contains("KEY_B=PASTE_KEY_HERE"));
    }

    #[test]
    fn detect_hf_token() {
        let text = "hf_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let found = detect_secrets(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key_hint, "HF_TOKEN");
    }

    #[test]
    fn detect_google_api_key() {
        let text = "AIzaSyA1234567890abcdefghijklmnopqrstuvw";
        let found = detect_secrets(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key_hint, "GOOGLE_API_KEY");
    }

    #[test]
    fn detect_stripe_key() {
        let text = "sk_test_abcdefghijklmnopqrstuvwxyz";
        let found = detect_secrets(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key_hint, "STRIPE_API_KEY");
    }

    #[test]
    fn detect_databricks_token() {
        // Build the test string at runtime to avoid push protection scanners
        let prefix = "dapi";
        let suffix = "a".repeat(32);
        let text = format!("{prefix}{suffix}");
        let found = detect_secrets(&text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key_hint, "DATABRICKS_TOKEN");
    }

    #[test]
    fn secure_delete_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_secret.txt");
        std::fs::write(&path, "secret_content").unwrap();
        assert!(path.exists());
        secure_delete(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn secure_delete_nonexistent_is_ok() {
        let path = std::path::PathBuf::from("/tmp/nonexistent_goose_test_file_12345");
        assert!(secure_delete(&path).is_ok());
    }
}
