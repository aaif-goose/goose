//! Tool call emulation for models without native tool-calling support.
//!
//! The model is prompted to emit shell commands as `$ command` on a new line and
//! code blocks as `` ```execute `` fenced blocks. A streaming parser detects these
//! patterns and converts them into tool-call messages.
//!
//! # Known false-positive scenarios
//!
//! Because detection is purely text-based, the parser can misinterpret model output:
//!
//! - **`$` at line start in explanatory text.** If the model writes a line starting
//!   with `$` as an example (e.g. "$ is the jQuery selector"), it will be treated as
//!   a shell command. Mid-sentence `$` (e.g. "costs $50") is safe — only `\n$` or
//!   `$` at the very start of output triggers command detection.
//!
//! - **`` ```execute `` in explanatory code fences.** If the model uses this exact
//!   fence tag in prose, the content will be executed. Standard `` ```js `` or
//!   `` ```python `` fences are not affected.
//!
//! These are inherent to text-based tool emulation. Models with native tool-calling
//! support should use the `inference_native_tools` path instead.

use crate::conversation::message::{Message, MessageContent};
use crate::providers::errors::ProviderError;
use llama_cpp_2::model::AddBos;
use rmcp::model::{CallToolRequestParams, Tool};
use serde_json::json;
use std::borrow::Cow;
use uuid::Uuid;

use super::inference_engine::{
    create_and_prefill_context, generation_loop, validate_and_compute_context, GenerationContext,
    TokenAction,
};
use super::{finalize_usage, StreamSender, CODE_EXECUTION_TOOL, SHELL_TOOL};

const HOLD_BACK_CODE_MODE: usize = " ```execute_typescript\n".len();
const HOLD_BACK_SHELL_ONLY: usize = "\n$".len();

pub(super) fn load_tiny_model_prompt() -> String {
    use std::env;

    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };

    let working_directory = env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    let context = json!({
        "os": os,
        "working_directory": working_directory,
        "shell": shell,
    });

    crate::prompt_template::render_template("tiny_model_system.md", &context).unwrap_or_else(|e| {
        tracing::warn!("Failed to load tiny_model_system.md: {:?}", e);
        "You are Goose, an AI assistant. You can execute shell commands by starting lines with $."
            .to_string()
    })
}

pub(super) fn build_emulator_tool_description(tools: &[Tool], code_mode_enabled: bool) -> String {
    let mut tool_desc = String::new();

    if code_mode_enabled {
        tool_desc.push_str("\n\n# Running Code\n\n");
        tool_desc.push_str(
            "You can call tools by writing code in a ```execute block. \
             The code runs immediately — do not explain it, just run it.\n\n",
        );
        tool_desc.push_str("Example — counting files in /tmp:\n\n");
        tool_desc.push_str("```execute_typescript\nasync function run() {\n");
        tool_desc.push_str(
            "  const result = await Developer.shell({ command: \"ls -1 /tmp | wc -l\" });\n",
        );
        tool_desc.push_str("  return result;\n}\n```\n\n");
        tool_desc.push_str("Rules:\n");
        tool_desc.push_str("- Code MUST define async function run() and return a result\n");
        tool_desc.push_str("- All function calls are async — use await\n");
        tool_desc.push_str("- Use ```execute for tool calls, $ for simple shell one-liners\n\n");
        tool_desc.push_str("Available functions:\n\n");

        for tool in tools {
            if tool.name.starts_with("code_execution__") {
                continue;
            }
            let parts: Vec<&str> = tool.name.splitn(2, "__").collect();
            if parts.len() == 2 {
                let namespace = {
                    let mut c = parts[0].chars();
                    match c.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().chain(c).collect::<String>(),
                    }
                };
                let camel_name: String = parts[1]
                    .split('_')
                    .enumerate()
                    .map(|(i, part)| {
                        if i == 0 {
                            part.to_string()
                        } else {
                            let mut c = part.chars();
                            match c.next() {
                                None => String::new(),
                                Some(first) => first.to_uppercase().chain(c).collect(),
                            }
                        }
                    })
                    .collect();
                let desc = tool.description.as_ref().map(|d| d.as_ref()).unwrap_or("");
                tool_desc.push_str(&format!("- {namespace}.{camel_name}(): {desc}\n"));
            }
        }
    } else {
        tool_desc.push_str("\n\n# Tools\n\nYou have access to the following tools:\n\n");
        for tool in tools {
            let desc = tool
                .description
                .as_ref()
                .map(|d| d.as_ref())
                .unwrap_or("No description");
            tool_desc.push_str(&format!("- {}: {}\n", tool.name, desc));
        }
    }

    tool_desc
}

enum EmulatorAction {
    Text(String),
    ShellCommand(String),
    ExecuteCode(String),
}

enum ParserState {
    Normal,
    InCommand,
    InExecuteBlock,
}

struct StreamingEmulatorParser {
    buffer: String,
    state: ParserState,
    code_mode_enabled: bool,
}

impl StreamingEmulatorParser {
    fn new(code_mode_enabled: bool) -> Self {
        Self {
            buffer: String::new(),
            state: ParserState::Normal,
            code_mode_enabled,
        }
    }

    fn process_chunk(&mut self, chunk: &str) -> Vec<EmulatorAction> {
        self.buffer.push_str(chunk);
        let mut results = Vec::new();

        loop {
            match self.state {
                ParserState::InCommand => {
                    if let Some((command_line, rest)) = self.buffer.split_once('\n') {
                        if let Some(command) = command_line.strip_prefix('$') {
                            let command = command.trim();
                            if !command.is_empty() {
                                results.push(EmulatorAction::ShellCommand(command.to_string()));
                            }
                        }
                        self.buffer = rest.to_string();
                        self.state = ParserState::Normal;
                    } else {
                        break;
                    }
                }
                ParserState::InExecuteBlock => {
                    // Look for closing ``` to end the execute block
                    if let Some(end_idx) = self.buffer.find("\n```") {
                        #[allow(clippy::string_slice)]
                        let code = self.buffer[..end_idx].to_string();
                        // Skip past the closing ``` and any trailing newline
                        #[allow(clippy::string_slice)]
                        let rest = &self.buffer[end_idx + 4..];
                        let rest = rest.strip_prefix('\n').unwrap_or(rest);
                        self.buffer = rest.to_string();
                        self.state = ParserState::Normal;
                        if !code.trim().is_empty() {
                            results.push(EmulatorAction::ExecuteCode(code));
                        }
                    } else {
                        // Still accumulating code — wait for closing fence
                        break;
                    }
                }
                ParserState::Normal => {
                    // Check for ```execute block (code mode)
                    if self.code_mode_enabled {
                        if let Some((before, after)) =
                            self.buffer.split_once("```execute_typescript\n")
                        {
                            if !before.trim().is_empty() {
                                results.push(EmulatorAction::Text(before.to_string()));
                            }
                            self.buffer = after.to_string();
                            self.state = ParserState::InExecuteBlock;
                            continue;
                        }
                        // Also handle without newline after tag (accumulating)
                        if self.buffer.ends_with("```execute_typescript") {
                            let before = self.buffer.trim_end_matches("```execute_typescript");
                            if !before.trim().is_empty() {
                                results.push(EmulatorAction::Text(before.to_string()));
                            }
                            self.buffer.clear();
                            self.state = ParserState::InExecuteBlock;
                            continue;
                        }
                    }

                    // Check for $ command
                    if let Some((before_dollar, from_dollar)) = self.buffer.split_once("\n$") {
                        let text = format!("{}\n", before_dollar);
                        if !text.trim().is_empty() {
                            results.push(EmulatorAction::Text(text));
                        }
                        self.buffer = format!("${}", from_dollar);
                        self.state = ParserState::InCommand;
                    } else if self.buffer.starts_with('$') && self.buffer.len() == chunk.len() {
                        self.state = ParserState::InCommand;
                    } else {
                        let hold_back = if self.code_mode_enabled {
                            HOLD_BACK_CODE_MODE
                        } else {
                            HOLD_BACK_SHELL_ONLY
                        };
                        let char_count = self.buffer.chars().count();
                        if char_count > hold_back && !self.buffer.ends_with('\n') {
                            let mut chars = self.buffer.chars();
                            let emit_count = char_count - hold_back;
                            let emit_text: String = chars.by_ref().take(emit_count).collect();
                            let keep_text: String = chars.collect();
                            if !emit_text.is_empty() {
                                results.push(EmulatorAction::Text(emit_text));
                            }
                            self.buffer = keep_text;
                        }
                        break;
                    }
                }
            }
        }

        results
    }

    fn flush(&mut self) -> Vec<EmulatorAction> {
        let mut results = Vec::new();

        if !self.buffer.is_empty() {
            match self.state {
                ParserState::InCommand => {
                    let command_line = self.buffer.trim();
                    if let Some(command) = command_line.strip_prefix('$') {
                        let command = command.trim();
                        if !command.is_empty() {
                            results.push(EmulatorAction::ShellCommand(command.to_string()));
                        }
                    } else if !command_line.is_empty() {
                        results.push(EmulatorAction::Text(self.buffer.clone()));
                    }
                }
                ParserState::InExecuteBlock => {
                    let code = self.buffer.trim();
                    if !code.is_empty() {
                        results.push(EmulatorAction::ExecuteCode(code.to_string()));
                    }
                }
                ParserState::Normal => {
                    results.push(EmulatorAction::Text(self.buffer.clone()));
                }
            }
            self.buffer.clear();
            self.state = ParserState::Normal;
        }

        results
    }
}

fn send_emulator_action(
    action: &EmulatorAction,
    message_id: &str,
    tx: &StreamSender,
) -> Result<bool, ()> {
    match action {
        EmulatorAction::Text(text) => {
            let mut message = Message::assistant().with_text(text);
            message.id = Some(message_id.to_string());
            tx.blocking_send(Ok((Some(message), None)))
                .map_err(|_| ())?;
            Ok(false)
        }
        EmulatorAction::ShellCommand(command) => {
            let tool_id = Uuid::new_v4().to_string();
            let mut args = serde_json::Map::new();
            args.insert("command".to_string(), json!(command));
            let tool_call =
                CallToolRequestParams::new(Cow::Borrowed(SHELL_TOOL)).with_arguments(args);
            let mut message = Message::assistant();
            message
                .content
                .push(MessageContent::tool_request(tool_id, Ok(tool_call)));
            message.id = Some(message_id.to_string());
            tx.blocking_send(Ok((Some(message), None)))
                .map_err(|_| ())?;
            Ok(true)
        }
        EmulatorAction::ExecuteCode(code) => {
            let tool_id = Uuid::new_v4().to_string();
            let wrapped = if code.contains("async function run()") {
                code.clone()
            } else {
                format!("async function run() {{\n{}\n}}", code)
            };
            let mut args = serde_json::Map::new();
            args.insert("code".to_string(), json!(wrapped));
            let tool_call =
                CallToolRequestParams::new(Cow::Borrowed(CODE_EXECUTION_TOOL)).with_arguments(args);
            let mut message = Message::assistant();
            message
                .content
                .push(MessageContent::tool_request(tool_id, Ok(tool_call)));
            message.id = Some(message_id.to_string());
            tx.blocking_send(Ok((Some(message), None)))
                .map_err(|_| ())?;
            Ok(true)
        }
    }
}

pub(super) fn generate_with_emulated_tools(
    ctx: &mut GenerationContext<'_>,
    code_mode_enabled: bool,
) -> Result<(), ProviderError> {
    // Use oaicompat variant — its C++ wrapper catches exceptions that would
    // otherwise abort the process when other native libs disturb the C++ ABI.
    let prompt = ctx
        .loaded
        .model
        .apply_chat_template_with_tools_oaicompat(
            &ctx.loaded.template,
            ctx.chat_messages,
            None, // no tools for emulated path
            None, // no json_schema
            true, // add_generation_prompt
        )
        .map(|r| r.prompt)
        .map_err(|e| {
            ProviderError::ExecutionError(format!("Failed to apply chat template: {}", e))
        })?;

    let tokens = ctx
        .loaded
        .model
        .str_to_token(&prompt, AddBos::Never)
        .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;

    let (prompt_token_count, effective_ctx) = validate_and_compute_context(
        ctx.loaded,
        ctx.runtime,
        tokens.len(),
        ctx.context_limit,
        ctx.settings,
    )?;
    let mut llama_ctx = create_and_prefill_context(
        ctx.loaded,
        ctx.runtime,
        &tokens,
        effective_ctx,
        ctx.settings,
    )?;

    let message_id = ctx.message_id;
    let tx = ctx.tx;
    let mut emulator_parser = StreamingEmulatorParser::new(code_mode_enabled);
    let mut tool_call_emitted = false;
    let mut send_failed = false;

    let output_token_count = generation_loop(
        &ctx.loaded.model,
        &mut llama_ctx,
        ctx.settings,
        prompt_token_count,
        effective_ctx,
        |piece| {
            let actions = emulator_parser.process_chunk(piece);
            for action in actions {
                match send_emulator_action(&action, message_id, tx) {
                    Ok(is_tool) => {
                        if is_tool {
                            tool_call_emitted = true;
                        }
                    }
                    Err(_) => {
                        send_failed = true;
                        return Ok(TokenAction::Stop);
                    }
                }
            }
            if tool_call_emitted {
                Ok(TokenAction::Stop)
            } else {
                Ok(TokenAction::Continue)
            }
        },
    )?;

    if !send_failed {
        for action in emulator_parser.flush() {
            if send_emulator_action(&action, message_id, tx).is_err() {
                break;
            }
        }
    }

    let provider_usage = finalize_usage(
        ctx.log,
        std::mem::take(&mut ctx.model_name),
        "emulator",
        prompt_token_count,
        output_token_count,
        None,
    );
    let _ = ctx.tx.blocking_send(Ok((None, Some(provider_usage))));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collect all actions from feeding chunks through the parser, then flushing.
    fn parse_chunks(chunks: &[&str], code_mode: bool) -> Vec<EmulatorAction> {
        let mut parser = StreamingEmulatorParser::new(code_mode);
        let mut actions = Vec::new();
        for chunk in chunks {
            actions.extend(parser.process_chunk(chunk));
        }
        actions.extend(parser.flush());
        actions
    }

    fn parse_all(input: &str, code_mode: bool) -> Vec<EmulatorAction> {
        parse_chunks(&[input], code_mode)
    }

    fn assert_text(action: &EmulatorAction, expected: &str) {
        match action {
            EmulatorAction::Text(t) => assert_eq!(t.trim(), expected.trim(), "text mismatch"),
            other => panic!("expected Text, got {:?}", action_label(other)),
        }
    }

    fn assert_shell(action: &EmulatorAction, expected: &str) {
        match action {
            EmulatorAction::ShellCommand(cmd) => {
                assert_eq!(cmd, expected, "shell command mismatch")
            }
            other => panic!("expected ShellCommand, got {:?}", action_label(other)),
        }
    }

    fn assert_execute(action: &EmulatorAction, expected: &str) {
        match action {
            EmulatorAction::ExecuteCode(code) => {
                assert_eq!(code.trim(), expected.trim(), "execute code mismatch")
            }
            other => panic!("expected ExecuteCode, got {:?}", action_label(other)),
        }
    }

    fn action_label(a: &EmulatorAction) -> &'static str {
        match a {
            EmulatorAction::Text(_) => "Text",
            EmulatorAction::ShellCommand(_) => "ShellCommand",
            EmulatorAction::ExecuteCode(_) => "ExecuteCode",
        }
    }

    #[test]
    fn plain_text_no_tools() {
        let actions = parse_all("Hello, world!", false);
        // Hold-back may split text across actions; concatenate all text
        let all_text: String = actions
            .iter()
            .map(|a| match a {
                EmulatorAction::Text(t) => t.as_str(),
                _ => panic!("expected only Text actions"),
            })
            .collect();
        assert_eq!(all_text.trim(), "Hello, world!");
    }

    #[test]
    fn single_shell_command() {
        let actions = parse_all("$ ls -la\n", false);
        assert_eq!(actions.len(), 1);
        assert_shell(&actions[0], "ls -la");
    }

    #[test]
    fn text_then_shell_command() {
        let actions = parse_all("Let me check:\n$ ls -la\n", false);
        assert!(actions.len() >= 2);
        assert_text(&actions[0], "Let me check:");
        assert_shell(&actions[actions.len() - 1], "ls -la");
    }

    #[test]
    fn shell_command_at_start_of_output() {
        let actions = parse_all("$ whoami\n", false);
        assert_eq!(actions.len(), 1);
        assert_shell(&actions[0], "whoami");
    }

    #[test]
    fn shell_command_without_trailing_newline() {
        // Flush should handle unterminated command
        let actions = parse_all("$ whoami", false);
        assert_eq!(actions.len(), 1);
        assert_shell(&actions[0], "whoami");
    }

    #[test]
    fn dollar_sign_mid_sentence_is_not_command() {
        let actions = parse_all("It costs $50 per month", false);
        for action in &actions {
            assert!(
                matches!(action, EmulatorAction::Text(_)),
                "mid-sentence $ should not trigger a shell command"
            );
        }
        let all_text: String = actions
            .iter()
            .filter_map(|a| match a {
                EmulatorAction::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(all_text.trim(), "It costs $50 per month");
    }

    #[test]
    fn execute_block() {
        let input = "Here's the code:\n```execute_typescript\nconsole.log('hi');\n```\n";
        let actions = parse_all(input, true);
        assert!(actions.len() >= 2);
        assert_text(&actions[0], "Here's the code:");
        assert_execute(&actions[actions.len() - 1], "console.log('hi');");
    }

    #[test]
    fn execute_block_not_detected_without_code_mode() {
        let input = "```execute_typescript\nconsole.log('hi');\n```\n";
        let actions = parse_all(input, false);
        // Should be treated as plain text
        for action in &actions {
            assert!(matches!(action, EmulatorAction::Text(_)));
        }
    }

    #[test]
    fn dollar_split_across_chunks() {
        // The \n and $ arrive in separate chunks
        let actions = parse_chunks(&["Let me check\n", "$ ls -la\n"], false);
        let shells: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, EmulatorAction::ShellCommand(_)))
            .collect();
        assert_eq!(shells.len(), 1);
        assert_shell(shells[0], "ls -la");
    }

    #[test]
    fn execute_fence_split_across_chunks() {
        let actions = parse_chunks(
            &["Here:\n```ex", "ecute_typescript\nlet x = 1;\n", "```\n"],
            true,
        );
        let executes: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, EmulatorAction::ExecuteCode(_)))
            .collect();
        assert_eq!(executes.len(), 1);
        assert_execute(executes[0], "let x = 1;");
    }

    #[test]
    fn multiple_commands_on_separate_lines() {
        // In practice, generation stops after the first tool call. But the
        // parser should detect commands separated by \n$ when fed as chunks.
        let actions = parse_chunks(&["Here:\n$ cd /tmp\n", "Done.\n$ ls\n"], false);
        let shells: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, EmulatorAction::ShellCommand(_)))
            .collect();
        assert_eq!(shells.len(), 2);
        assert_shell(shells[0], "cd /tmp");
        assert_shell(shells[1], "ls");
    }

    #[test]
    fn regular_code_fence_not_treated_as_execute() {
        let input = "```python\nprint('hi')\n```\n";
        let actions = parse_all(input, true);
        for action in &actions {
            assert!(
                matches!(action, EmulatorAction::Text(_)),
                "regular code fence should be text"
            );
        }
    }

    #[test]
    fn empty_command_ignored() {
        let actions = parse_all("$\n", false);
        // Empty command after $ should not produce a ShellCommand
        let shells: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, EmulatorAction::ShellCommand(_)))
            .collect();
        assert_eq!(shells.len(), 0);
    }

    #[test]
    fn token_by_token_streaming() {
        // Simulate LLM generating one token at a time
        let input = "$ echo hello\n";
        let chars: Vec<String> = input.chars().map(|c| c.to_string()).collect();
        let chunks: Vec<&str> = chars.iter().map(|s| s.as_str()).collect();
        let actions = parse_chunks(&chunks, false);
        let shells: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, EmulatorAction::ShellCommand(_)))
            .collect();
        assert_eq!(shells.len(), 1);
        assert_shell(shells[0], "echo hello");
    }

    #[test]
    fn execute_block_with_multiline_code() {
        let input = "```execute_typescript\nasync function run() {\n  const r = await Developer.shell({ command: \"ls\" });\n  return r;\n}\n```\n";
        let actions = parse_all(input, true);
        let executes: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, EmulatorAction::ExecuteCode(_)))
            .collect();
        assert_eq!(executes.len(), 1);
        match executes[0] {
            EmulatorAction::ExecuteCode(code) => {
                assert!(code.contains("async function run()"));
                assert!(code.contains("Developer.shell"));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn unclosed_execute_block_flushed() {
        // Model stops generating mid-block
        let input = "```execute_typescript\nlet x = 1;";
        let actions = parse_all(input, true);
        let executes: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, EmulatorAction::ExecuteCode(_)))
            .collect();
        assert_eq!(executes.len(), 1);
        assert_execute(executes[0], "let x = 1;");
    }
}
