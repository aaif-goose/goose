use anyhow::{Context, Result};
use goose::config::Config;
use goose::conversation::message::Message;
use goose::conversation::Conversation;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use tempfile::Builder;
use tempfile::NamedTempFile;

/// Resolve the editor command from config and environment variables.
/// Checks GOOSE_PROMPT_EDITOR, then $VISUAL, then $EDITOR.
pub fn resolve_editor_command() -> Option<String> {
    let config = Config::global();
    let config_editor = config.get_goose_prompt_editor().ok().flatten();
    let visual = std::env::var("VISUAL").ok();
    let editor_env = std::env::var("EDITOR").ok();
    resolve_editor_from_sources(
        config_editor.as_deref(),
        visual.as_deref(),
        editor_env.as_deref(),
    )
}

fn resolve_editor_from_sources(
    config_editor: Option<&str>,
    visual: Option<&str>,
    editor_env: Option<&str>,
) -> Option<String> {
    for cmd in [config_editor, visual, editor_env].into_iter().flatten() {
        if !cmd.is_empty() {
            return Some(cmd.to_string());
        }
    }
    None
}

/// Resolve the editor command, falling back to vi (or notepad on Windows).
pub fn resolve_editor_or_default() -> String {
    let config = Config::global();
    let config_editor = config.get_goose_prompt_editor().ok().flatten();
    let visual = std::env::var("VISUAL").ok();
    let editor_env = std::env::var("EDITOR").ok();
    resolve_editor_or_default_from_sources(
        config_editor.as_deref(),
        visual.as_deref(),
        editor_env.as_deref(),
    )
}

fn resolve_editor_default() -> String {
    if cfg!(windows) {
        "notepad".to_string()
    } else {
        "vi".to_string()
    }
}

fn resolve_editor_or_default_from_sources(
    config_editor: Option<&str>,
    visual: Option<&str>,
    editor_env: Option<&str>,
) -> String {
    resolve_editor_from_sources(config_editor, visual, editor_env)
        .unwrap_or_else(resolve_editor_default)
}

/// Open a YAML temp file with the user's editor to edit a conversation.
/// Returns the edited conversation, or an error if the editor failed or YAML was invalid.
pub fn edit_conversation(conversation: &Conversation) -> Result<Conversation> {
    let yaml = serde_yaml::to_string(conversation.messages())?;

    let mut tmp = NamedTempFile::with_suffix(".yaml")?;
    tmp.write_all(yaml.as_bytes())?;
    tmp.flush()?;

    let editor = resolve_editor_or_default();
    let path = tmp.path().to_path_buf();

    launch_editor(&editor, &path).with_context(|| format!("failed to launch editor '{editor}'"))?;

    let edited = std::fs::read_to_string(&path)?;
    let messages: Vec<Message> =
        serde_yaml::from_str(&edited).context("invalid YAML — session unchanged")?;

    Ok(Conversation::new_unvalidated(messages))
}

/// Build the markdown template content for the editor prompt.
fn build_template(messages: &[&str], prefill: Option<&str>) -> String {
    let mut content = String::from("# Goose Prompt Editor\n\n");

    content.push_str("# Your prompt:\n\n");
    if let Some(text) = prefill {
        if !text.is_empty() {
            content.push_str(text);
            content.push('\n');
        }
    }

    if !messages.is_empty() {
        content.push_str("# Recent conversation for context (newest first):\n\n");
        for message in messages.iter().rev() {
            content.push_str(&format!("{}\n", message));
        }
        content.push('\n');
    }

    content
}

/// Create temporary markdown file with conversation history and optional prefill text
fn create_temp_file(messages: &[&str], prefill: Option<&str>) -> Result<NamedTempFile> {
    let temp_file = Builder::new()
        .prefix("goose_prompt_")
        .suffix(".md")
        .tempfile()?;

    fs::write(temp_file.path(), build_template(messages, prefill))?;
    Ok(temp_file)
}

/// Split an editor command into program and arguments.
///
/// Uses shell-word splitting only when the command contains quotes, so values like
/// `"/Applications/Sublime Text.app/.../subl" -w` work. Unquoted commands are split on
/// whitespace to avoid shlex stripping backslashes from Windows paths like
/// `C:\Windows\System32\notepad.exe`.
fn split_editor_command(editor_cmd: &str) -> Result<Vec<String>> {
    if editor_cmd.contains(['"', '\'']) {
        shlex::split(editor_cmd).ok_or_else(|| {
            anyhow::anyhow!("Invalid editor command: unmatched quotes in '{editor_cmd}'")
        })
    } else {
        Ok(editor_cmd.split_whitespace().map(String::from).collect())
    }
}

/// Launch editor and wait for completion
fn launch_editor(editor_cmd: &str, file_path: &Path) -> Result<()> {
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

/// Main function to get input from editor
pub fn get_editor_input(
    editor_cmd: &str,
    messages: &[&str],
    prefill: Option<&str>,
) -> Result<(String, bool)> {
    let temp_file = create_temp_file(messages, prefill)?;
    let temp_path = temp_file.path().to_path_buf();

    launch_editor(editor_cmd, &temp_path)?;

    let mut content = String::new();
    let mut file = temp_file.reopen()?;
    file.read_to_string(&mut content)?;

    let user_input = extract_user_input(&content);

    let has_meaningful_content = !user_input.trim().is_empty();

    Ok((user_input, has_meaningful_content))
}

/// Extract only the user's input from the markdown file
fn extract_user_input(content: &str) -> String {
    if let Some(start) = content.find("# Your prompt:") {
        let marker_len = "# Your prompt:".len();
        #[allow(clippy::string_slice)]
        let user_section = &content[start + marker_len..];

        let end_patterns = [
            "# Recent conversation for context",
            "# Recent conversation for context (newest first):",
        ];

        let mut end_pos = None;
        for pattern in &end_patterns {
            if let Some(pos) = user_section.find(pattern) {
                end_pos = Some(pos);
                break;
            }
        }

        let user_input_section = match end_pos {
            Some(pos) =>
            {
                #[allow(clippy::string_slice)]
                &user_section[..pos]
            }
            None => user_section,
        };

        user_input_section.trim().to_string()
    } else {
        content.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_extract_user_input_with_editor_output() {
        let content = r#"# Goose Prompt Editor

# Your prompt:
This is the hardcoded prompt response
# Recent conversation for context (newest first):

## User: Hello
## Assistant: Hi there!
"#;

        let result = extract_user_input(content);

        assert_eq!(result, "This is the hardcoded prompt response");
    }

    #[test]
    fn test_extract_user_input_no_marker() {
        let content = "Just plain text without markers";
        let result = extract_user_input(content);
        assert_eq!(result, "Just plain text without markers");
    }

    #[test]
    fn test_extract_user_input_conversation_history_heading() {
        let content = r#"# Goose Prompt Editor

# Your prompt:
This is the user's input

# Recent conversation for context (newest first):

## User: Previous message
## Assistant: Previous response
"#;

        let result = extract_user_input(content);
        assert_eq!(result, "This is the user's input");
    }

    #[test]
    fn test_create_temp_file_with_messages() {
        let messages = vec!["## User: Hello", "## Assistant: Hi there!"];

        let temp_file = create_temp_file(&messages, None).unwrap();
        let path = temp_file.path();

        assert!(path.exists());
        assert!(path.to_str().unwrap().contains("goose_prompt_"));
        assert!(path.to_str().unwrap().ends_with(".md"));

        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("# Goose Prompt Editor"));
        assert!(content.contains("## User: Hello"));
        assert!(content.contains("## Assistant: Hi there!"));
        assert!(content.contains("# Your prompt:"));
        assert!(content.contains("# Recent conversation for context (newest first):"));
    }

    #[test]
    fn test_create_temp_file_with_prefill() {
        let messages = vec!["## User: Hello"];
        let temp_file = create_temp_file(&messages, Some("fix the login bug")).unwrap();
        let content = fs::read_to_string(temp_file.path()).unwrap();

        assert!(content.contains("# Your prompt:"));
        assert!(content.contains("fix the login bug"));
        // Prefill text should appear before conversation context
        let prefill_pos = content.find("fix the login bug").unwrap();
        let context_pos = content.find("# Recent conversation for context").unwrap();
        assert!(
            prefill_pos < context_pos,
            "Prefill text should appear before conversation context"
        );
    }

    #[test]
    fn test_create_temp_file_without_prefill() {
        let messages = vec!["## User: Hello"];
        let temp_file = create_temp_file(&messages, None).unwrap();
        let content = fs::read_to_string(temp_file.path()).unwrap();

        assert!(content.contains("# Your prompt:"));
        assert!(!content.contains("fix the login bug"));
    }

    #[test]
    fn test_create_temp_file_with_prefix_suffix() {
        let temp_file = Builder::new()
            .prefix("goose_test_")
            .suffix(".md")
            .tempfile()
            .unwrap();

        let name = temp_file.path().file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("goose_test_"));
        assert!(name.ends_with(".md"));
    }

    #[test]
    fn test_extract_user_input() {
        let content = r#"# Goose Prompt Editor

# Recent conversation for context:

# Your prompt:
This is the user's actual input
with multiple lines.
"#;

        let result = extract_user_input(content);
        assert_eq!(
            result,
            "This is the user's actual input\nwith multiple lines."
        );
    }

    #[test]
    fn test_tempfile_cleanup() {
        let path = {
            let temp_file = Builder::new()
                .prefix("goose_cleanup_test_")
                .tempfile()
                .unwrap();
            let path = temp_file.path().to_path_buf();
            assert!(path.exists());
            path
        };

        assert!(!path.exists());
    }

    #[test]
    fn test_message_ordering_newest_first() {
        let messages = vec![
            "## User: First message",
            "## Assistant: First response",
            "## User: Second message",
            "## Assistant: Second response",
            "## User: Third message (newest)",
        ];

        let temp_file = create_temp_file(&messages, None).unwrap();
        let content = fs::read_to_string(temp_file.path()).unwrap();

        let newest_first = [
            "## User: Third message (newest)",
            "## Assistant: Second response",
            "## User: Second message",
            "## Assistant: First response",
            "## User: First message",
        ];

        for expected_msg in &newest_first {
            assert!(
                content.contains(expected_msg),
                "Expected to find message '{}' in content",
                expected_msg
            );
        }

        let newest_pos = content.find("## User: Third message (newest)").unwrap();
        let oldest_pos = content.find("## User: First message").unwrap();
        assert!(
            newest_pos < oldest_pos,
            "Newest message should appear before oldest message"
        );
    }

    #[test]
    fn test_resolve_editor_resolution_priority() {
        assert_eq!(
            resolve_editor_from_sources(Some("config-val"), Some("visual-val"), Some("editor-val")),
            Some("config-val".to_string())
        );

        assert_eq!(
            resolve_editor_from_sources(Some(""), Some("visual-val"), Some("editor-val")),
            Some("visual-val".to_string())
        );

        assert_eq!(
            resolve_editor_from_sources(None, Some(""), Some("editor-val")),
            Some("editor-val".to_string())
        );

        assert_eq!(resolve_editor_from_sources(None, None, None), None);
        assert_eq!(
            resolve_editor_from_sources(Some(""), Some(""), Some("")),
            None
        );

        let default_val = resolve_editor_default();
        assert_eq!(
            resolve_editor_or_default_from_sources(None, None, None),
            default_val
        );
        assert_eq!(
            resolve_editor_or_default_from_sources(Some(""), Some(""), Some("")),
            default_val
        );
    }

    #[test]
    fn test_split_editor_command() {
        assert_eq!(
            split_editor_command("code --wait").unwrap(),
            vec!["code", "--wait"]
        );

        assert_eq!(
            split_editor_command(
                r#""/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl" -w"#
            )
            .unwrap(),
            vec![
                "/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl",
                "-w"
            ]
        );

        assert_eq!(
            split_editor_command(r"C:\Windows\System32\notepad.exe").unwrap(),
            vec![r"C:\Windows\System32\notepad.exe"]
        );

        assert!(split_editor_command(r#"code --wait "unclosed"#).is_err());
    }

    // --- build_template edge case tests ---

    #[test]
    fn test_build_template_empty_prefill_string() {
        let content = build_template(&["## User: Hello"], Some(""));
        assert!(content.contains("# Your prompt:\n\n#"));
        assert!(content.contains("# Recent conversation for context"));
    }

    #[test]
    fn test_build_template_prefill_with_no_messages() {
        let content = build_template(&[], Some("fix the bug"));
        assert!(content.contains("# Your prompt:\n\nfix the bug\n"));
        assert!(!content.contains("# Recent conversation for context"));
    }

    #[test]
    fn test_build_template_no_prefill_no_messages() {
        let content = build_template(&[], None);
        assert_eq!(content, "# Goose Prompt Editor\n\n# Your prompt:\n\n");
    }

    #[test]
    fn test_build_template_prefill_with_messages() {
        let content = build_template(&["## User: Hi", "## Assistant: Hello"], Some("do stuff"));
        assert!(content.contains("do stuff"));
        assert!(content.contains("## User: Hi"));
        let prefill_pos = content.find("do stuff").unwrap();
        let context_pos = content.find("# Recent conversation").unwrap();
        assert!(prefill_pos < context_pos);
    }

    #[test]
    fn test_extract_user_input_with_prefill_kept() {
        let content = build_template(&["## User: Hello"], Some("fix the login bug"));
        let result = extract_user_input(&content);
        assert_eq!(result, "fix the login bug");
    }

    #[test]
    fn test_extract_user_input_with_prefill_edited() {
        let mut content = build_template(&["## User: Hello"], Some("fix the login bug"));
        content = content.replace(
            "fix the login bug",
            "fix the login bug and also the signup flow",
        );
        let result = extract_user_input(&content);
        assert_eq!(result, "fix the login bug and also the signup flow");
    }

    #[test]
    fn test_extract_user_input_prefill_replaced() {
        let mut content = build_template(&["## User: Hello"], Some("fix the login bug"));
        content = content.replace("fix the login bug\n", "completely different prompt\n");
        let result = extract_user_input(&content);
        assert_eq!(result, "completely different prompt");
    }

    #[test]
    fn test_extract_user_input_prefill_cleared() {
        let mut content = build_template(&["## User: Hello"], Some("fix the login bug"));
        content = content.replace("fix the login bug\n", "");
        let result = extract_user_input(&content);
        assert_eq!(result, "");
    }

    #[test]
    fn test_extract_user_input_multiline_with_prefill() {
        let mut content = build_template(&["## User: Hello"], Some("line one"));
        content = content.replace("line one\n", "line one\nline two\nline three\n");
        let result = extract_user_input(&content);
        assert_eq!(result, "line one\nline two\nline three");
    }

    #[test]
    #[cfg(unix)]
    fn test_editor_uses_random_tempfile_and_preserves_cwd() {
        let script = Builder::new()
            .prefix("goose_editor_test_")
            .tempfile()
            .unwrap();
        let path_report = Builder::new()
            .prefix("goose_editor_path_")
            .tempfile()
            .unwrap();
        let cwd_report = Builder::new()
            .prefix("goose_editor_cwd_")
            .tempfile()
            .unwrap();
        let secret = Builder::new()
            .prefix("goose_editor_secret_")
            .tempfile()
            .unwrap();

        for path in [
            script.path(),
            path_report.path(),
            cwd_report.path(),
            secret.path(),
        ] {
            assert!(!path.to_string_lossy().contains(char::is_whitespace));
        }
        fs::write(secret.path(), "TOP_SECRET_EXFIL_abc123").unwrap();

        let predictable_path = Path::new(".goose_prompt_temp.md");
        assert!(!predictable_path.exists());

        fs::write(
            script.path(),
            r#"printf '%s' "$4" > "$1"
pwd > "$2"
printf '# Goose Prompt Editor\n\n# Your prompt:\n\nupdated prompt\n' > "$4"
ln -sf "$3" .goose_prompt_temp.md
"#,
        )
        .unwrap();

        let editor_cmd = format!(
            "sh {} {} {} {}",
            script.path().display(),
            path_report.path().display(),
            cwd_report.path().display(),
            secret.path().display()
        );
        let result = get_editor_input(&editor_cmd, &["## User: previous"], None);

        let malicious_target = fs::read_link(predictable_path).unwrap();
        fs::remove_file(predictable_path).unwrap();
        assert_eq!(malicious_target, secret.path());

        let (input, has_content) = result.unwrap();

        let edited_path = fs::read_to_string(path_report.path()).unwrap();
        let edited_path = Path::new(edited_path.trim());
        assert!(edited_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("goose_prompt_"));
        assert_ne!(edited_path, Path::new(".goose_prompt_temp.md"));
        assert!(!edited_path.exists());
        assert_eq!(
            Path::new(fs::read_to_string(cwd_report.path()).unwrap().trim()),
            std::env::current_dir().unwrap()
        );
        assert_eq!(input, "updated prompt");
        assert!(!input.contains("TOP_SECRET_EXFIL_abc123"));
        assert!(has_content);
    }
}
