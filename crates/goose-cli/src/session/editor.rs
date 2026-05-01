use anyhow::Result;
use goose::config::Config;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
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

/// Inner resolution logic, separated for testability.
/// Checks sources in priority order: config, VISUAL, EDITOR.
/// Skips empty strings at each level.
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

/// RAII guard to ensure symlink is cleaned up even on panic
struct SymlinkCleanup {
    symlink_path: PathBuf,
}

impl SymlinkCleanup {
    fn new(symlink_path: PathBuf) -> Self {
        Self { symlink_path }
    }
}

impl Drop for SymlinkCleanup {
    fn drop(&mut self) {
        // Always try to clean up the symlink, ignoring any errors
        let _ = std::fs::remove_file(&self.symlink_path);
    }
}

/// Launch editor and wait for completion
fn launch_editor(editor_cmd: &str, file_path: &PathBuf) -> Result<()> {
    use std::process::Stdio;

    let parts: Vec<&str> = editor_cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty editor command"));
    }

    let mut cmd = Command::new(parts[0]);
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

    let symlink_path = PathBuf::from(".goose_prompt_temp.md");

    if symlink_path.exists() {
        std::fs::remove_file(&symlink_path)?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(&temp_path, &symlink_path)?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&temp_path, &symlink_path)?;

    let _cleanup_guard = SymlinkCleanup::new(symlink_path.clone());

    let _original_template = build_template(messages, prefill);

    launch_editor(editor_cmd, &symlink_path)?;

    let mut content = String::new();
    let mut file = std::fs::File::open(&symlink_path)?;
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
    #[cfg(unix)]
    fn test_symlink_raii_cleanup_on_panic() {
        use std::os::unix::fs;
        use std::panic;

        let messages = vec!["## User: Test message for panic cleanup"];
        let temp_file = create_temp_file(&messages, None).unwrap();
        let temp_path = temp_file.path().to_path_buf();

        let symlink_path = PathBuf::from(format!("test_panic_cleanup_{}.md", std::process::id()));

        if symlink_path.exists() {
            let _ = std::fs::remove_file(&symlink_path);
        }

        assert!(
            !symlink_path.exists(),
            "Symlink should not exist before test"
        );

        #[cfg(unix)]
        fs::symlink(&temp_path, &symlink_path).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&temp_path, &symlink_path).unwrap();

        assert!(symlink_path.exists(), "Symlink should exist after creation");

        let cleanup_guard = SymlinkCleanup::new(symlink_path.clone());

        let result = panic::catch_unwind(|| {
            let _guard = cleanup_guard;
            panic!("Simulating a panic to test cleanup");
        });

        assert!(result.is_err(), "Panic should have been caught");

        assert!(
            !symlink_path.exists(),
            "Symlink should be cleaned up even after panic"
        );
    }

    // --- resolve_editor_from_sources tests ---

    #[test]
    fn test_resolve_editor_returns_config_when_set() {
        let result = resolve_editor_from_sources(Some("code"), Some("vim"), Some("nano"));
        assert_eq!(result.as_deref(), Some("code"));
    }

    #[test]
    fn test_resolve_editor_falls_back_to_visual() {
        let result = resolve_editor_from_sources(None, Some("vim"), Some("nano"));
        assert_eq!(result.as_deref(), Some("vim"));
    }

    #[test]
    fn test_resolve_editor_falls_back_to_editor_env() {
        let result = resolve_editor_from_sources(None, None, Some("nano"));
        assert_eq!(result.as_deref(), Some("nano"));
    }

    #[test]
    fn test_resolve_editor_returns_none_when_nothing_set() {
        let result = resolve_editor_from_sources(None, None, None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_editor_skips_empty_config() {
        let result = resolve_editor_from_sources(Some(""), Some("vim"), None);
        assert_eq!(result.as_deref(), Some("vim"));
    }

    #[test]
    fn test_resolve_editor_skips_empty_visual() {
        let result = resolve_editor_from_sources(None, Some(""), Some("nano"));
        assert_eq!(result.as_deref(), Some("nano"));
    }

    #[test]
    fn test_resolve_editor_skips_all_empty() {
        let result = resolve_editor_from_sources(Some(""), Some(""), Some(""));
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_editor_skips_empty_config_and_visual() {
        let result = resolve_editor_from_sources(Some(""), Some(""), Some("emacs"));
        assert_eq!(result.as_deref(), Some("emacs"));
    }

    // --- build_template edge case tests ---

    #[test]
    fn test_build_template_empty_prefill_string() {
        let content = build_template(&["## User: Hello"], Some(""));
        // Empty prefill should not appear in content
        assert!(content.contains("# Your prompt:\n\n#"));
        // Should go directly to conversation context
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

    // --- extract_user_input with prefilled content tests ---

    #[test]
    fn test_extract_user_input_with_prefill_kept() {
        // Simulates a user who opened the editor with prefill and kept it unchanged
        let content = build_template(&["## User: Hello"], Some("fix the login bug"));
        let result = extract_user_input(&content);
        assert_eq!(result, "fix the login bug");
    }

    #[test]
    fn test_extract_user_input_with_prefill_edited() {
        // Simulates a user who edited the prefill text
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
        // Simulates a user who deleted the prefill and wrote something new
        let mut content = build_template(&["## User: Hello"], Some("fix the login bug"));
        content = content.replace("fix the login bug\n", "completely different prompt\n");
        let result = extract_user_input(&content);
        assert_eq!(result, "completely different prompt");
    }

    #[test]
    fn test_extract_user_input_prefill_cleared() {
        // Simulates a user who deleted the prefill and left nothing
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
    fn test_symlink_creation_and_cleanup() {
        use std::os::unix::fs;

        let messages = vec!["## User: Test message"];
        let temp_file = create_temp_file(&messages, None).unwrap();
        let temp_path = temp_file.path().to_path_buf();

        let symlink_path = PathBuf::from(format!("test_symlink_cleanup_{}.md", std::process::id()));

        if symlink_path.exists() {
            let _ = std::fs::remove_file(&symlink_path);
        }

        assert!(
            !symlink_path.exists(),
            "Symlink should be removed before creating new one"
        );

        #[cfg(unix)]
        fs::symlink(&temp_path, &symlink_path).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&temp_path, &symlink_path).unwrap();

        assert!(symlink_path.exists());

        let content = std::fs::read_to_string(&symlink_path).unwrap();
        assert!(content.contains("## User: Test message"));

        #[cfg(unix)]
        {
            let read_link = std::fs::read_link(&symlink_path).unwrap();
            assert_eq!(read_link, temp_path);
        }

        #[cfg(windows)]
        {
            assert!(temp_path.exists());
            let temp_content = std::fs::read_to_string(&temp_path).unwrap();
            assert_eq!(content, temp_content);
        }

        let _ = std::fs::remove_file(&symlink_path);
        assert!(!symlink_path.exists());
    }
}
