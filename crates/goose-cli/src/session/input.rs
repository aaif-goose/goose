use super::completion::GooseCompleter;
use super::{CompletionCache, HintStatus};
use anyhow::Result;
use goose::config::{Config, GooseMode};
use rustyline::Editor;
use shlex;
use std::collections::HashMap;
use std::sync::Arc;
use strum::VariantNames;

#[derive(Debug)]
pub enum InputResult {
    Message(String),
    Exit,
    AddExtension(String),
    AddBuiltin(String),
    ToggleTheme,
    SelectTheme(String),
    Retry,
    ListPrompts(Option<String>),
    PromptCommand(PromptCommandOptions),
    GooseMode(String),
    Plan(PlanCommandOptions),
    EndPlan,
    Clear,
    Recipe(Option<String>),
    Compact,
    ToggleFullToolOutput,
}

#[derive(Debug)]
pub struct PromptCommandOptions {
    pub name: String,
    pub info: bool,
    pub arguments: HashMap<String, String>,
}

#[derive(Debug)]
pub struct PlanCommandOptions {
    pub message_text: String,
}

struct CtrlCHandler {
    completion_cache: Arc<std::sync::RwLock<CompletionCache>>,
}

impl CtrlCHandler {
    fn new(completion_cache: Arc<std::sync::RwLock<CompletionCache>>) -> Self {
        Self { completion_cache }
    }
}

impl rustyline::ConditionalEventHandler for CtrlCHandler {
    /// Handle Ctrl+C to clear the line if text is entered, otherwise check if we should exit.
    fn handle(
        &self,
        _event: &rustyline::Event,
        _n: usize,
        _positive: bool,
        ctx: &rustyline::EventContext,
    ) -> Option<rustyline::Cmd> {
        if !ctx.line().is_empty() {
            // Clear the line if there's text
            let mut cache = self.completion_cache.write().unwrap();
            cache.hint_status = HintStatus::Default;
            Some(rustyline::Cmd::Kill(rustyline::Movement::WholeBuffer))
        } else {
            let mut cache = self.completion_cache.write().unwrap();

            if cache.hint_status == HintStatus::MaybeExit {
                return Some(rustyline::Cmd::Interrupt);
            }

            cache.hint_status = HintStatus::MaybeExit;
            drop(cache);

            Some(rustyline::Cmd::Repaint)
        }
    }
}

pub fn get_newline_key() -> char {
    Config::global()
        .get_param::<String>("GOOSE_CLI_NEWLINE_KEY")
        .ok()
        .and_then(|s| s.chars().next())
        .map(|c| c.to_ascii_lowercase())
        .unwrap_or('j')
}

pub fn get_input(
    editor: &mut Editor<GooseCompleter, rustyline::history::DefaultHistory>,
    conversation_messages: Option<&Vec<String>>,
) -> Result<InputResult> {
    let config = Config::global();
    if let Ok(Some(editor_cmd)) = config.get_goose_prompt_editor() {
        let messages = extract_recent_messages(conversation_messages);
        let message_refs: Vec<&str> = messages.iter().map(|s| s.as_str()).collect();
        let (message, has_meaningful_content) =
            crate::session::editor::get_editor_input(&editor_cmd, &message_refs)?;

        if !has_meaningful_content {
            return get_regular_input(editor);
        }
        editor.add_history_entry(message.as_str())?;
        return Ok(InputResult::Message(message));
    }

    let completion_cache = editor
        .helper()
        .map(|h| h.completion_cache.clone())
        .ok_or_else(|| anyhow::anyhow!("Editor helper not set"))?;

    let newline_key = get_newline_key();
    editor.bind_sequence(
        rustyline::KeyEvent(
            rustyline::KeyCode::Char(newline_key),
            rustyline::Modifiers::CTRL,
        ),
        rustyline::EventHandler::Simple(rustyline::Cmd::Newline),
    );

    editor.bind_sequence(
        rustyline::KeyEvent(rustyline::KeyCode::Char('c'), rustyline::Modifiers::CTRL),
        rustyline::EventHandler::Conditional(Box::new(CtrlCHandler::new(completion_cache))),
    );

    let prompt = get_input_prompt_string();

    let input = match editor.readline(&prompt) {
        Ok(text) => text,
        Err(e) => match e {
            rustyline::error::ReadlineError::Interrupted => return Ok(InputResult::Exit),
            rustyline::error::ReadlineError::Eof => return Ok(InputResult::Exit),
            _ => return Err(e.into()),
        },
    };

    // Add valid input to history (history saving to file is handled in the Session::interactive method)
    if !input.trim().is_empty() {
        editor.add_history_entry(input.as_str())?;
    }

    // Handle non-slash commands first
    if !input.starts_with('/') {
        let trimmed = input.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("exit")
            || trimmed.eq_ignore_ascii_case("quit")
        {
            return Ok(if trimmed.is_empty() {
                InputResult::Retry
            } else {
                InputResult::Exit
            });
        }
        return Ok(InputResult::Message(trimmed.to_string()));
    }

    // Handle slash commands
    match handle_slash_command(&input) {
        Some(result) => Ok(result),
        None => Ok(InputResult::Message(input.trim().to_string())),
    }
}

fn get_regular_input(
    editor: &mut Editor<GooseCompleter, rustyline::history::DefaultHistory>,
) -> Result<InputResult> {
    let completion_cache = editor
        .helper()
        .map(|h| h.completion_cache.clone())
        .ok_or_else(|| anyhow::anyhow!("Editor helper not set"))?;

    let newline_key = get_newline_key();
    editor.bind_sequence(
        rustyline::KeyEvent(
            rustyline::KeyCode::Char(newline_key),
            rustyline::Modifiers::CTRL,
        ),
        rustyline::EventHandler::Simple(rustyline::Cmd::Newline),
    );

    editor.bind_sequence(
        rustyline::KeyEvent(rustyline::KeyCode::Char('c'), rustyline::Modifiers::CTRL),
        rustyline::EventHandler::Conditional(Box::new(CtrlCHandler::new(completion_cache))),
    );

    let prompt = get_input_prompt_string();

    let input = match editor.readline(&prompt) {
        Ok(text) => text,
        Err(e) => match e {
            rustyline::error::ReadlineError::Interrupted => return Ok(InputResult::Exit),
            rustyline::error::ReadlineError::Eof => return Ok(InputResult::Exit),
            _ => return Err(e.into()),
        },
    };

    // Add valid input to history (history saving to file is handled in the Session::interactive method)
    if !input.trim().is_empty() {
        editor.add_history_entry(input.as_str())?;
    }

    // Handle non-slash commands first
    if !input.starts_with('/') {
        let trimmed = input.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("exit")
            || trimmed.eq_ignore_ascii_case("quit")
        {
            return Ok(if trimmed.is_empty() {
                InputResult::Retry
            } else {
                InputResult::Exit
            });
        }
        return Ok(InputResult::Message(trimmed.to_string()));
    }

    // Handle slash commands
    match handle_slash_command(&input) {
        Some(result) => Ok(result),
        None => Ok(InputResult::Message(input.trim().to_string())),
    }
}

fn handle_slash_command(input: &str) -> Option<InputResult> {
    let input = input.trim();

    // Command prefix constants
    const CMD_PROMPTS: &str = "/prompts ";
    const CMD_PROMPT: &str = "/prompt";
    const CMD_PROMPT_WITH_SPACE: &str = "/prompt ";
    const CMD_EXTENSION: &str = "/extension ";
    const CMD_BUILTIN: &str = "/builtin ";
    const CMD_MODE: &str = "/mode ";
    const CMD_PLAN: &str = "/plan";
    const CMD_ENDPLAN: &str = "/endplan";
    const CMD_CLEAR: &str = "/clear";
    const CMD_RECIPE: &str = "/recipe";
    const CMD_COMPACT: &str = "/compact";
    const CMD_SUMMARIZE_DEPRECATED: &str = "/summarize";

    match input {
        "/exit" | "/quit" => Some(InputResult::Exit),
        "/?" | "/help" => {
            print_help();
            print_editor_help();
            Some(InputResult::Retry)
        }
        "/t" => Some(InputResult::ToggleTheme),
        s if s.starts_with("/t ") => {
            let t = s
                .strip_prefix("/t ")
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            if ["light", "dark", "ansi"].contains(&t.as_str()) {
                Some(InputResult::SelectTheme(t))
            } else {
                println!(
                    "Theme Unavailable: {} Available themes are: light, dark, ansi",
                    t
                );
                Some(InputResult::Retry)
            }
        }
        "/prompts" => Some(InputResult::ListPrompts(None)),
        s if s.starts_with(CMD_PROMPTS) => {
            // Parse arguments for /prompts command
            let args = s.strip_prefix(CMD_PROMPTS).unwrap_or_default();
            parse_prompts_command(args)
        }
        s if s.starts_with(CMD_PROMPT) => {
            if s == CMD_PROMPT {
                // No arguments case
                Some(InputResult::PromptCommand(PromptCommandOptions {
                    name: String::new(), // Empty name will trigger the error message in the rendering
                    info: false,
                    arguments: HashMap::new(),
                }))
            } else if let Some(stripped) = s.strip_prefix(CMD_PROMPT_WITH_SPACE) {
                // Has arguments case
                parse_prompt_command(stripped)
            } else {
                // Handle invalid cases like "/promptxyz"
                None
            }
        }
        s if s.starts_with(CMD_EXTENSION) => Some(InputResult::AddExtension(
            s.get(CMD_EXTENSION.len()..).unwrap_or("").to_string(),
        )),
        s if s.starts_with(CMD_BUILTIN) => Some(InputResult::AddBuiltin(
            s.get(CMD_BUILTIN.len()..).unwrap_or("").to_string(),
        )),
        s if s.starts_with(CMD_MODE) => Some(InputResult::GooseMode(
            s.get(CMD_MODE.len()..).unwrap_or("").to_string(),
        )),
        s if s.starts_with(CMD_PLAN) => {
            parse_plan_command(s.get(CMD_PLAN.len()..).unwrap_or("").trim().to_string())
        }
        s if s == CMD_ENDPLAN => Some(InputResult::EndPlan),
        s if s == CMD_CLEAR => Some(InputResult::Clear),
        s if s.starts_with(CMD_RECIPE) => parse_recipe_command(s),
        s if s == CMD_COMPACT => Some(InputResult::Compact),
        s if s == CMD_SUMMARIZE_DEPRECATED => {
            println!("{}", console::style("⚠️  Note: /summarize has been renamed to /compact and will be removed in a future release.").yellow());
            Some(InputResult::Compact)
        }
        "/r" => Some(InputResult::ToggleFullToolOutput),
        _ => None,
    }
}

fn parse_recipe_command(s: &str) -> Option<InputResult> {
    const CMD_RECIPE: &str = "/recipe";

    if s == CMD_RECIPE {
        // No filepath provided, use default
        return Some(InputResult::Recipe(None));
    }

    // Extract the filepath from the command
    let filepath = s.get(CMD_RECIPE.len()..).unwrap_or("").trim();

    if filepath.is_empty() {
        return Some(InputResult::Recipe(None));
    }

    // Validate that the filepath ends with .yaml
    if !filepath.to_lowercase().ends_with(".yaml") {
        println!("{}", console::style("Filepath must end with .yaml").red());
        return Some(InputResult::Retry);
    }

    // Return the filepath for validation in the handler
    Some(InputResult::Recipe(Some(filepath.to_string())))
}

fn parse_prompts_command(args: &str) -> Option<InputResult> {
    let parts: Vec<String> = shlex::split(args).unwrap_or_default();

    // Look for --extension flag
    for i in 0..parts.len() {
        if parts[i] == "--extension" && i + 1 < parts.len() {
            // Return the extension name that follows the flag
            return Some(InputResult::ListPrompts(Some(parts[i + 1].clone())));
        }
    }

    // If we got here, there was no valid --extension flag
    Some(InputResult::ListPrompts(None))
}

fn parse_prompt_command(args: &str) -> Option<InputResult> {
    let parts: Vec<String> = shlex::split(args).unwrap_or_default();

    // set name to empty and error out in the rendering
    let mut options = PromptCommandOptions {
        name: parts.first().cloned().unwrap_or_default(),
        info: false,
        arguments: HashMap::new(),
    };

    // handle info at any point in the command
    if parts.iter().any(|part| part == "--info") {
        options.info = true;
    }

    // Parse remaining arguments
    let mut i = 1;

    while i < parts.len() {
        let part = &parts[i];

        // Skip flag arguments
        if part == "--info" {
            i += 1;
            continue;
        }

        // Process key=value pairs - removed redundant contains check
        if let Some((key, value)) = part.split_once('=') {
            options.arguments.insert(key.to_string(), value.to_string());
        }

        i += 1;
    }

    Some(InputResult::PromptCommand(options))
}

fn parse_plan_command(input: String) -> Option<InputResult> {
    let options = PlanCommandOptions {
        message_text: input.trim().to_string(),
    };

    Some(InputResult::Plan(options))
}

fn get_input_prompt_string() -> String {
    if is_vte_with_broken_emoji_width() {
        return "> ".to_string();
    }
    "🪿 ".to_string()
}

/// VTE < 0.70 renders 🪿 as 1 cell while unicode-width counts 2, causing cursor offset.
fn is_vte_with_broken_emoji_width() -> bool {
    let Ok(vte_version) = std::env::var("VTE_VERSION") else {
        return false;
    };
    let Ok(version) = vte_version.parse::<u32>() else {
        return true;
    };
    version < 7000
}

fn print_help() {
    let newline_key = get_newline_key().to_ascii_uppercase();
    let modes = GooseMode::VARIANTS.join(", ");
    println!(
        "Available commands:
/exit or /quit - Exit the session
/t - Toggle Light/Dark/Ansi theme
/t <name> - Set theme directly (light, dark, ansi)
/r - Toggle full tool output display (show complete tool parameters without truncation)
/extension <command> - Add a stdio extension (format: ENV1=val1 command args...)
/builtin <names> - Add builtin extensions by name (comma-separated)
/prompts [--extension <name>] - List all available prompts, optionally filtered by extension
/prompt <n> [--info] [key=value...] - Get prompt info or execute a prompt
/mode <name> - Set the goose mode to use ({modes})
/plan <message_text> -  Enters 'plan' mode with optional message. Create a plan based on the current messages and asks user if they want to act on it.
                        If user acts on the plan, goose mode is set to 'auto' and returns to 'normal' goose mode.
                        To warm up goose before using '/plan', we recommend setting '/mode approve' & putting appropriate context into goose.
                        The model is used based on $GOOSE_PLANNER_PROVIDER and $GOOSE_PLANNER_MODEL environment variables.
                        If no model is set, the default model is used.
/endplan - Exit plan mode and return to 'normal' goose mode.
/recipe [filepath] - Generate a recipe from the current conversation and save it to the specified filepath (must end with .yaml).
                       If no filepath is provided, it will be saved to ./recipe.yaml.
/compact - Compact the current conversation to reduce context length while preserving key information.
/? or /help - Display this help message
/clear - Clears the current chat history

Navigation:
Ctrl+C - Clear current line if text is entered, otherwise exit the session
Ctrl+{newline_key} - Add a newline (configurable via GOOSE_CLI_NEWLINE_KEY)
Up/Down arrows - Navigate through command history"
    );
}

/// Extract recent messages for editor context
fn extract_recent_messages(conversation_messages: Option<&Vec<String>>) -> Vec<String> {
    match conversation_messages {
        Some(messages) => {
            // Return the messages in reverse chronological order (newest first)
            messages.clone()
        }
        None => Vec::new(),
    }
}

/// Print help information about editor input
fn print_editor_help() {
    println!(
        "Editor Input:
When goose_prompt_editor is configured, prompts will open in your editor instead of the CLI.
Previous conversation is included as markdown headings for context.
Configure with: goose configure set goose_prompt_editor \"vim\""
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_slash_command() {
        // Test exit commands
        assert!(matches!(
            handle_slash_command("/exit"),
            Some(InputResult::Exit)
        ));
        assert!(matches!(
            handle_slash_command("/quit"),
            Some(InputResult::Exit)
        ));

        // Test help commands
        assert!(matches!(
            handle_slash_command("/help"),
            Some(InputResult::Retry)
        ));
        assert!(matches!(
            handle_slash_command("/?"),
            Some(InputResult::Retry)
        ));

        // Test theme toggle
        assert!(matches!(
            handle_slash_command("/t"),
            Some(InputResult::ToggleTheme)
        ));

        // Test full tool output toggle
        assert!(matches!(
            handle_slash_command("/r"),
            Some(InputResult::ToggleFullToolOutput)
        ));

        // Test extension command
        if let Some(InputResult::AddExtension(cmd)) = handle_slash_command("/extension foo bar") {
            assert_eq!(cmd, "foo bar");
        } else {
            panic!("Expected AddExtension");
        }

        // Test builtin command
        if let Some(InputResult::AddBuiltin(names)) = handle_slash_command("/builtin dev,git") {
            assert_eq!(names, "dev,git");
        } else {
            panic!("Expected AddBuiltin");
        }

        // Test unknown commands
        assert!(handle_slash_command("/unknown").is_none());
    }

    #[test]
    fn test_prompts_command() {
        // Test basic prompts command
        if let Some(InputResult::ListPrompts(extension)) = handle_slash_command("/prompts") {
            assert!(extension.is_none());
        } else {
            panic!("Expected ListPrompts");
        }

        // Test prompts with extension filter
        if let Some(InputResult::ListPrompts(extension)) =
            handle_slash_command("/prompts --extension test")
        {
            assert_eq!(extension, Some("test".to_string()));
        } else {
            panic!("Expected ListPrompts with extension");
        }
    }

    #[test]
    fn test_prompt_command() {
        // Test basic prompt info command
        if let Some(InputResult::PromptCommand(opts)) =
            handle_slash_command("/prompt test-prompt --info")
        {
            assert_eq!(opts.name, "test-prompt");
            assert!(opts.info);
            assert!(opts.arguments.is_empty());
        } else {
            panic!("Expected PromptCommand");
        }

        // Test prompt with arguments
        if let Some(InputResult::PromptCommand(opts)) =
            handle_slash_command("/prompt test-prompt arg1=val1 arg2=val2")
        {
            assert_eq!(opts.name, "test-prompt");
            assert!(!opts.info);
            assert_eq!(opts.arguments.len(), 2);
            assert_eq!(opts.arguments.get("arg1"), Some(&"val1".to_string()));
            assert_eq!(opts.arguments.get("arg2"), Some(&"val2".to_string()));
        } else {
            panic!("Expected PromptCommand");
        }
    }

    // Test whitespace handling
    #[test]
    fn test_whitespace_handling() {
        // Leading/trailing whitespace in extension command
        if let Some(InputResult::AddExtension(cmd)) = handle_slash_command("  /extension foo bar  ")
        {
            assert_eq!(cmd, "foo bar");
        } else {
            panic!("Expected AddExtension");
        }

        // Leading/trailing whitespace in builtin command
        if let Some(InputResult::AddBuiltin(names)) = handle_slash_command("  /builtin dev,git  ") {
            assert_eq!(names, "dev,git");
        } else {
            panic!("Expected AddBuiltin");
        }
    }

    // Test prompt with no arguments
    #[test]
    fn test_prompt_no_args() {
        // Test just "/prompt" with no arguments
        if let Some(InputResult::PromptCommand(opts)) = handle_slash_command("/prompt") {
            assert_eq!(opts.name, "");
            assert!(!opts.info);
            assert!(opts.arguments.is_empty());
        } else {
            panic!("Expected PromptCommand");
        }

        // Test invalid prompt command
        assert!(handle_slash_command("/promptxyz").is_none());
    }

    // Test quoted arguments
    #[test]
    fn test_quoted_arguments() {
        // Test prompt with quoted arguments
        if let Some(InputResult::PromptCommand(opts)) = handle_slash_command(
            r#"/prompt test-prompt arg1="value with spaces" arg2="another value""#,
        ) {
            assert_eq!(opts.name, "test-prompt");
            assert_eq!(opts.arguments.len(), 2);
            assert_eq!(
                opts.arguments.get("arg1"),
                Some(&"value with spaces".to_string())
            );
            assert_eq!(
                opts.arguments.get("arg2"),
                Some(&"another value".to_string())
            );
        } else {
            panic!("Expected PromptCommand");
        }

        // Test prompt with mixed quoted and unquoted arguments
        if let Some(InputResult::PromptCommand(opts)) = handle_slash_command(
            r#"/prompt test-prompt simple=value quoted="value with \"nested\" quotes""#,
        ) {
            assert_eq!(opts.name, "test-prompt");
            assert_eq!(opts.arguments.len(), 2);
            assert_eq!(opts.arguments.get("simple"), Some(&"value".to_string()));
            assert_eq!(
                opts.arguments.get("quoted"),
                Some(&r#"value with "nested" quotes"#.to_string())
            );
        } else {
            panic!("Expected PromptCommand");
        }
    }

    // Test invalid arguments
    #[test]
    fn test_invalid_arguments() {
        // Test prompt with invalid arguments
        if let Some(InputResult::PromptCommand(opts)) =
            handle_slash_command(r#"/prompt test-prompt valid=value invalid_arg another_invalid"#)
        {
            assert_eq!(opts.name, "test-prompt");
            assert_eq!(opts.arguments.len(), 1);
            assert_eq!(opts.arguments.get("valid"), Some(&"value".to_string()));
            // Invalid arguments are ignored but logged
        } else {
            panic!("Expected PromptCommand");
        }
    }

    #[test]
    fn test_plan_mode() {
        // Test plan mode with no text
        let result = handle_slash_command("/plan");
        assert!(result.is_some());

        // Test plan mode with text
        let result = handle_slash_command("/plan hello world");
        assert!(result.is_some());
        let options = result.unwrap();
        match options {
            InputResult::Plan(options) => {
                assert_eq!(options.message_text, "hello world");
            }
            _ => panic!("Expected Plan"),
        }
    }

    #[test]
    fn test_recipe_command() {
        // Test recipe with no filepath
        if let Some(InputResult::Recipe(filepath)) = handle_slash_command("/recipe") {
            assert!(filepath.is_none());
        } else {
            panic!("Expected Recipe");
        }

        // Test recipe with filepath
        if let Some(InputResult::Recipe(filepath)) =
            handle_slash_command("/recipe /path/to/file.yaml")
        {
            assert_eq!(filepath, Some("/path/to/file.yaml".to_string()));
        } else {
            panic!("Expected recipe with filepath");
        }

        // Test recipe with invalid extension
        let result = handle_slash_command("/recipe /path/to/file.txt");
        assert!(matches!(result, Some(InputResult::Retry)));
    }
}
