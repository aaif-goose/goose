use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use futures::future::BoxFuture;
use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::base::{
    ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata, ProviderUsage, Usage,
};
use super::errors::ProviderError;
use super::utils::{filter_extensions_from_system_prompt, RequestLog};
use crate::config::base::{CodexCommand, CodexReasoningEffort, CodexSkipGitCheck};
use crate::config::paths::Paths;
use crate::config::search_path::SearchPaths;
use crate::config::{Config, ExtensionConfig, GooseMode};
use crate::conversation::message::{Message, MessageContent};
use crate::model::ModelConfig;
use crate::subprocess::configure_subprocess;
use rmcp::model::Role;
use rmcp::model::Tool;

const CODEX_PROVIDER_NAME: &str = "codex";
pub const CODEX_DEFAULT_MODEL: &str = "gpt-5.2-codex";
pub const CODEX_KNOWN_MODELS: &[&str] = &[
    "gpt-5.2-codex",
    "gpt-5.2",
    "gpt-5.1-codex-max",
    "gpt-5.1-codex-mini",
];
pub const CODEX_DOC_URL: &str = "https://developers.openai.com/codex/cli";

/// Valid reasoning effort levels for Codex
pub const CODEX_REASONING_LEVELS: &[&str] = &["none", "low", "medium", "high", "xhigh"];

/// Spawns the Codex CLI (`codex exec`) as a one-shot child process per turn.
/// Text prompt is piped via stdin (`-`), images are passed as temporary files
/// via the `-i` flag. Output is JSONL on stdout (`--json`), with events like
/// `item.completed`, `turn.completed`, and `error`.
#[derive(Debug, serde::Serialize)]
pub struct CodexProvider {
    command: PathBuf,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
    /// Reasoning effort level (none, low, medium, high, xhigh)
    reasoning_effort: String,
    /// Whether to skip git repo check
    skip_git_check: bool,
    /// CLI config overrides for MCP servers
    mcp_config_overrides: Vec<String>,
    #[serde(skip)]
    mode_by_session: tokio::sync::RwLock<HashMap<String, GooseMode>>,
}

impl CodexProvider {
    fn supports_reasoning_effort(model_name: &str, reasoning_effort: &str) -> bool {
        if !CODEX_REASONING_LEVELS.contains(&reasoning_effort) {
            return false;
        }

        if reasoning_effort == "none" && model_name.contains("codex") {
            return false;
        }

        true
    }

    /// Apply permission flags based on GooseMode
    fn apply_permission_flags(
        cmd: &mut Command,
        goose_mode: GooseMode,
    ) -> Result<(), ProviderError> {
        match goose_mode {
            GooseMode::Auto => {
                // --yolo is shorthand for --dangerously-bypass-approvals-and-sandbox
                cmd.arg("--yolo");
            }
            GooseMode::SmartApprove => {
                // --full-auto applies workspace-write sandbox and approvals only on failure
                cmd.arg("--full-auto");
            }
            GooseMode::Approve => {
                // Default codex behavior - interactive approvals
                // No special flags needed
            }
            GooseMode::Chat => {
                // Read-only sandbox mode
                cmd.arg("--sandbox").arg("read-only");
            }
        }
        Ok(())
    }

    /// Execute codex CLI command
    async fn execute_command(
        &self,
        system: &str,
        messages: &[Message],
        _tools: &[Tool],
        goose_mode: GooseMode,
    ) -> Result<Vec<String>, ProviderError> {
        // Single pass: text → prompt (stdin), images → temp files (-i flags)
        let image_dir = Paths::state_dir().join("codex/images");
        std::fs::create_dir_all(&image_dir).ok();
        let (prompt, temp_files) = prepare_input(system, messages, &image_dir)?;

        if std::env::var("GOOSE_CODEX_DEBUG").is_ok() {
            println!("=== CODEX PROVIDER DEBUG ===");
            println!("Command: {:?}", self.command);
            println!("Model: {}", self.model.model_name);
            println!("Reasoning effort: {}", self.reasoning_effort);
            println!("Skip git check: {}", self.skip_git_check);
            println!("Prompt length: {} chars", prompt.len());
            println!("Prompt: {}", prompt);
            println!("Image files: {}", temp_files.len());
            println!("============================");
        }

        let mut cmd = Command::new(&self.command);
        configure_subprocess(&mut cmd);

        // Propagate extended PATH so the codex subprocess can find Node.js
        // and other dependencies (especially when launched from the desktop app
        // where the inherited PATH is limited).
        if let Ok(path) = SearchPaths::builder().with_npm().path() {
            cmd.env("PATH", path);
        }

        // Use 'exec' subcommand for non-interactive mode
        cmd.arg("exec");

        // Only pass model parameter if it's in the known models list
        // This allows users to set GOOSE_PROVIDER=codex without needing to specify a model
        if CODEX_KNOWN_MODELS.contains(&self.model.model_name.as_str()) {
            cmd.arg("-m").arg(&self.model.model_name);
        }

        // Reasoning effort configuration
        cmd.arg("-c").arg(format!(
            "model_reasoning_effort=\"{}\"",
            self.reasoning_effort
        ));

        for override_config in &self.mcp_config_overrides {
            cmd.arg("-c").arg(override_config);
        }

        // JSON output format for structured parsing
        cmd.arg("--json");

        Self::apply_permission_flags(&mut cmd, goose_mode)?;

        // Skip git repo check if configured
        if self.skip_git_check {
            cmd.arg("--skip-git-repo-check");
        }

        // Codex treats -i as supplementary context, not positionally interleaved with text
        for tmp in &temp_files {
            cmd.arg("-i").arg(tmp.path());
        }

        // Pass the prompt via stdin using '-' argument
        cmd.arg("-");

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            ProviderError::RequestFailed(format!(
                "Failed to spawn Codex CLI command '{:?}': {}. \
                Make sure the Codex CLI is installed (npm i -g @openai/codex) \
                and available in the configured search paths.",
                self.command, e
            ))
        })?;

        // Write prompt to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(prompt.as_bytes()).await.map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to write to stdin: {}", e))
            })?;
            // Close stdin to signal end of input
            drop(stdin);
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProviderError::RequestFailed("Failed to capture stdout".to_string()))?;

        // Drain stderr concurrently to prevent pipe buffer deadlock
        let stderr_handle = {
            let stderr = child.stderr.take();
            tokio::spawn(async move {
                let mut output = String::new();
                if let Some(mut stderr) = stderr {
                    use tokio::io::AsyncReadExt;
                    let _ = stderr.read_to_string(&mut output).await;
                }
                output
            })
        };

        let mut reader = BufReader::new(stdout);
        let mut lines = Vec::new();
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        lines.push(trimmed.to_string());
                    }
                }
                Err(e) => {
                    return Err(ProviderError::RequestFailed(format!(
                        "Failed to read output: {}",
                        e
                    )));
                }
            }
        }

        let exit_status = child.wait().await.map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to wait for command: {}", e))
        })?;

        // Allow the stderr task to finish
        let _ = stderr_handle.await;

        if !exit_status.success() && lines.is_empty() {
            return Err(ProviderError::RequestFailed(format!(
                "Codex command failed with exit code: {:?}",
                exit_status.code()
            )));
        }

        tracing::debug!("Codex CLI executed successfully, got {} lines", lines.len());

        Ok(lines)
    }

    /// Extract text content from an item.completed event (agent_message only, skip reasoning)
    fn extract_text_from_item(item: &serde_json::Value) -> Option<String> {
        let item_type = item.get("type").and_then(|t| t.as_str());
        if item_type == Some("agent_message") {
            item.get("text")
                .and_then(|t| t.as_str())
                .filter(|text| !text.trim().is_empty())
                .map(|s| s.to_string())
        } else {
            None
        }
    }

    /// Extract usage information from a JSON object
    fn extract_usage(usage_info: &serde_json::Value, usage: &mut Usage) {
        if usage.input_tokens.is_none() {
            usage.input_tokens = usage_info
                .get("input_tokens")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
        }
        if usage.output_tokens.is_none() {
            usage.output_tokens = usage_info
                .get("output_tokens")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
        }
    }

    /// Extract error message from an error event
    fn extract_error(parsed: &serde_json::Value) -> Option<String> {
        parsed
            .get("message")
            .and_then(|m| m.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                parsed
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
    }

    /// Extract text from legacy message formats
    fn extract_legacy_text(parsed: &serde_json::Value) -> Vec<String> {
        let mut texts = Vec::new();
        if let Some(content) = parsed.get("content").and_then(|c| c.as_array()) {
            for item in content {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    texts.push(text.to_string());
                }
            }
        }
        if let Some(text) = parsed.get("text").and_then(|t| t.as_str()) {
            texts.push(text.to_string());
        }
        if let Some(text) = parsed.get("result").and_then(|r| r.as_str()) {
            texts.push(text.to_string());
        }
        texts
    }

    /// Build fallback text from non-JSON lines
    fn build_fallback_text(lines: &[String]) -> Option<String> {
        let response_text: String = lines
            .iter()
            .filter(|line| {
                !line.starts_with('{')
                    || serde_json::from_str::<serde_json::Value>(line)
                        .map(|v| v.get("type").is_none())
                        .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if response_text.trim().is_empty() {
            None
        } else {
            Some(response_text)
        }
    }

    /// Parse newline-delimited JSON response from Codex CLI
    fn parse_response(&self, lines: &[String]) -> Result<(Message, Usage), ProviderError> {
        let mut all_text_content = Vec::new();
        let mut usage = Usage::default();
        let mut error_message: Option<String> = None;

        for line in lines {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(event_type) = parsed.get("type").and_then(|t| t.as_str()) {
                    match event_type {
                        "item.completed" => {
                            if let Some(item) = parsed.get("item") {
                                if let Some(text) = Self::extract_text_from_item(item) {
                                    all_text_content.push(text);
                                }
                            }
                        }
                        "turn.completed" | "result" | "done" => {
                            if let Some(usage_info) = parsed.get("usage") {
                                Self::extract_usage(usage_info, &mut usage);
                            }
                            all_text_content.extend(Self::extract_legacy_text(&parsed));
                        }
                        "error" | "turn.failed" => {
                            error_message = Self::extract_error(&parsed);
                        }
                        "message" | "assistant" => {
                            all_text_content.extend(Self::extract_legacy_text(&parsed));
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(err) = error_message {
            if all_text_content.is_empty() {
                if err.contains("context window") || err.contains("context_length_exceeded") {
                    return Err(ProviderError::ContextLengthExceeded(err));
                }
                if err.to_lowercase().contains("rate limit") {
                    return Err(ProviderError::RateLimitExceeded {
                        details: err,
                        retry_delay: None,
                    });
                }
                return Err(ProviderError::RequestFailed(format!(
                    "Codex CLI error: {}",
                    err
                )));
            }
        }

        if all_text_content.is_empty() {
            if let Some(fallback) = Self::build_fallback_text(lines) {
                all_text_content.push(fallback);
            }
        }

        if let (Some(input), Some(output)) = (usage.input_tokens, usage.output_tokens) {
            usage.total_tokens = Some(input + output);
        }

        let combined_text = all_text_content.join("\n\n");
        if combined_text.is_empty() {
            return Err(ProviderError::RequestFailed(
                "Empty response from Codex CLI".to_string(),
            ));
        }

        let message = Message::new(
            Role::Assistant,
            chrono::Utc::now().timestamp(),
            vec![MessageContent::text(combined_text)],
        );

        Ok((message, usage))
    }
}

/// Builds the text prompt and extracts images to temp files in a single pass.
/// Text goes to the prompt string (piped via stdin); images become temp files
/// (passed via `-i` flags). Returns (prompt, temp_files).
fn prepare_input(
    system: &str,
    messages: &[Message],
    image_dir: &Path,
) -> Result<(String, Vec<NamedTempFile>), ProviderError> {
    let mut prompt = String::new();
    let mut temp_files = Vec::new();

    let filtered_system = filter_extensions_from_system_prompt(system);
    if !filtered_system.is_empty() {
        prompt.push_str(&filtered_system);
        prompt.push_str("\n\n");
    }

    for message in messages.iter().filter(|m| m.is_agent_visible()) {
        let role_prefix = match message.role {
            Role::User => "Human: ",
            Role::Assistant => "Assistant: ",
        };
        prompt.push_str(role_prefix);

        for content in &message.content {
            match content {
                MessageContent::Text(t) => {
                    prompt.push_str(&t.text);
                    prompt.push('\n');
                }
                MessageContent::Image(img) => {
                    let decoded = BASE64.decode(&img.data).map_err(|e| {
                        ProviderError::RequestFailed(format!("Failed to decode image: {}", e))
                    })?;
                    // Codex only supports png and jpeg:
                    // https://github.com/openai/codex/blob/aea7610c/codex-rs/utils/image/src/lib.rs#L162-L167
                    let ext = match img.mime_type.as_str() {
                        "image/png" => "png",
                        "image/jpeg" => "jpg",
                        _ => {
                            return Err(ProviderError::RequestFailed(format!(
                                "Unsupported image MIME type for Codex: {}",
                                img.mime_type
                            )));
                        }
                    };
                    let mut tmp = tempfile::Builder::new()
                        .suffix(&format!(".{}", ext))
                        .tempfile_in(image_dir)
                        .map_err(|e| {
                            ProviderError::RequestFailed(format!(
                                "Failed to create temp file: {}",
                                e
                            ))
                        })?;
                    tmp.write_all(&decoded).map_err(|e| {
                        ProviderError::RequestFailed(format!("Failed to write image: {}", e))
                    })?;
                    temp_files.push(tmp);
                }
                MessageContent::ToolRequest(req) => {
                    if let Ok(call) = &req.tool_call {
                        prompt.push_str(&format!("[tool_use: {} id={}]\n", call.name, req.id));
                    }
                }
                MessageContent::ToolResponse(resp) => {
                    if let Ok(result) = &resp.tool_result {
                        let text: String = result
                            .content
                            .iter()
                            .filter_map(|c| match &c.raw {
                                rmcp::model::RawContent::Text(t) => Some(t.text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<&str>>()
                            .join("\n");
                        prompt.push_str(&format!("[tool_result id={}] {}\n", resp.id, text));
                    }
                }
                _ => {}
            }
        }
        prompt.push('\n');
    }

    prompt.push_str("Assistant: ");
    Ok((prompt, temp_files))
}

fn toml_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if c.is_control() => {
                // TOML \uXXXX for other control characters
                for unit in c.encode_utf16(&mut [0; 2]) {
                    out.push_str(&format!("\\u{:04X}", unit));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// Codex CLI only supports inline `-c key=value` TOML overrides — no file-based
// config merging. Resolved secrets (from env_keys/keystore) in envs/headers end
// up in process argv, visible via `ps`. Claude Code avoids this by writing to a
// temp file with 0o600 permissions.
// Tracking: https://github.com/openai/codex/issues/2628
fn codex_mcp_config_overrides(extensions: &[ExtensionConfig]) -> Vec<String> {
    let mut overrides = Vec::new();
    for extension in extensions {
        match extension {
            ExtensionConfig::StreamableHttp { uri, headers, .. } => {
                let key = extension.key();
                overrides.push(format!("mcp_servers.{}.url={}", key, toml_quote(uri)));
                if !headers.is_empty() {
                    let mut hkeys: Vec<_> = headers.keys().collect();
                    hkeys.sort();
                    let entries: Vec<_> = hkeys
                        .iter()
                        .map(|k| format!("{} = {}", toml_quote(k), toml_quote(&headers[*k])))
                        .collect();
                    overrides.push(format!(
                        "mcp_servers.{}.http_headers={{{}}}",
                        key,
                        entries.join(", ")
                    ));
                }
            }
            ExtensionConfig::Stdio {
                cmd, args, envs, ..
            } => {
                let key = extension.key();
                overrides.push(format!("mcp_servers.{}.command={}", key, toml_quote(cmd)));
                if !args.is_empty() {
                    let items: Vec<_> = args.iter().map(|a| toml_quote(a)).collect();
                    overrides.push(format!("mcp_servers.{}.args=[{}]", key, items.join(", ")));
                }
                let env_map = envs.get_env();
                if !env_map.is_empty() {
                    let mut ekeys: Vec<_> = env_map.keys().collect();
                    ekeys.sort();
                    let entries: Vec<_> = ekeys
                        .iter()
                        .map(|k| {
                            format!("{} = {}", toml_quote(k), toml_quote(&env_map[k.as_str()]))
                        })
                        .collect();
                    overrides.push(format!(
                        "mcp_servers.{}.env={{{}}}",
                        key,
                        entries.join(", ")
                    ));
                }
            }
            ExtensionConfig::Sse { name, .. } => {
                tracing::debug!(name, "skipping SSE extension, migrate to streamable_http");
            }
            _ => {}
        }
    }
    overrides
}

impl ProviderDef for CodexProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            CODEX_PROVIDER_NAME,
            "OpenAI Codex CLI",
            "[Deprecated: use chatgpt_codex or codex-acp instead] Execute OpenAI models via Codex CLI tool. Requires codex CLI installed.",
            CODEX_DEFAULT_MODEL,
            CODEX_KNOWN_MODELS.to_vec(),
            CODEX_DOC_URL,
            vec![
                ConfigKey::from_value_type::<CodexCommand>(true, false, true),
                ConfigKey::from_value_type::<CodexReasoningEffort>(false, false, true),
                ConfigKey::from_value_type::<CodexSkipGitCheck>(false, false, true),
            ],
        )
    }

    fn from_env(
        model: ModelConfig,
        extensions: Vec<ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(async move {
            let config = Config::global();
            let command: String = config.get_codex_command().unwrap_or_default().into();
            let resolved_command = SearchPaths::builder().with_npm().resolve(command)?;

            // Get reasoning effort from config, default to "high"
            let reasoning_effort = config
                .get_codex_reasoning_effort()
                .map(String::from)
                .unwrap_or_else(|_| "high".to_string());

            // Validate reasoning effort
            let reasoning_effort =
                if Self::supports_reasoning_effort(&model.model_name, &reasoning_effort) {
                    reasoning_effort
                } else {
                    tracing::warn!(
                        "Invalid CODEX_REASONING_EFFORT '{}' for model '{}', using 'high'",
                        reasoning_effort,
                        model.model_name
                    );
                    "high".to_string()
                };

            // Get skip_git_check from config, default to false
            let skip_git_check = config
                .get_codex_skip_git_check()
                .map(|s| s.to_lowercase() == "true")
                .unwrap_or(false);

            let mut resolved = Vec::with_capacity(extensions.len());
            for ext in extensions {
                resolved.push(ext.resolve(config).await?);
            }

            Ok(Self {
                command: resolved_command,
                model,
                name: CODEX_PROVIDER_NAME.to_string(),
                reasoning_effort,
                skip_git_check,
                mcp_config_overrides: codex_mcp_config_overrides(&resolved),
                mode_by_session: tokio::sync::RwLock::new(HashMap::new()),
            })
        })
    }
}

#[async_trait]
impl Provider for CodexProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        if super::cli_common::is_session_description_request(system) {
            let (message, provider_usage) = super::cli_common::generate_simple_session_description(
                &model_config.model_name,
                messages,
            )?;
            return Ok(super::base::stream_from_single_message(
                message,
                provider_usage,
            ));
        }

        let goose_mode = {
            let map = self.mode_by_session.read().await;
            map.get(session_id).copied().unwrap_or_default()
        };
        let lines = self
            .execute_command(system, messages, tools, goose_mode)
            .await?;

        let (message, usage) = self.parse_response(&lines)?;

        // Create a payload for debug tracing
        let payload = json!({
            "command": self.command,
            "model": model_config.model_name,
            "reasoning_effort": self.reasoning_effort,
            "system_length": system.len(),
            "messages_count": messages.len()
        });

        let mut log = RequestLog::start(model_config, &payload).map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to start request log: {}", e))
        })?;

        let response = json!({
            "lines": lines.len(),
            "usage": usage
        });

        log.write(&response, Some(&usage)).map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to write request log: {}", e))
        })?;

        let provider_usage = ProviderUsage::new(model_config.model_name.clone(), usage);
        Ok(super::base::stream_from_single_message(
            message,
            provider_usage,
        ))
    }

    async fn update_mode(&self, session_id: &str, mode: GooseMode) -> Result<(), ProviderError> {
        self.mode_by_session
            .write()
            .await
            .insert(session_id.to_string(), mode);
        Ok(())
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(CODEX_KNOWN_MODELS.iter().map(|s| s.to_string()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::extension::Envs;
    use goose_test_support::TEST_IMAGE_B64;
    use std::collections::HashMap;
    use test_case::test_case;

    #[test]
    fn test_codex_metadata() {
        let metadata = CodexProvider::metadata();
        assert_eq!(metadata.name, "codex");
        assert_eq!(metadata.default_model, CODEX_DEFAULT_MODEL);
        assert!(!metadata.known_models.is_empty());
        // Check that the default model is in the known models
        assert!(metadata
            .known_models
            .iter()
            .any(|m| m.name == CODEX_DEFAULT_MODEL));
    }

    #[test_case(
        ExtensionConfig::Stdio {
            name: "lookup".into(),
            cmd: "node".into(),
            args: vec!["server.js".into()],
            envs: Envs::new([("API_KEY".into(), "secret".into())].into()),
            env_keys: vec![],
            description: "Lookup".into(),
            timeout: Some(30),
            bundled: None,
            available_tools: vec![],
        },
        &[
            r#"mcp_servers.lookup.command="node""#,
            r#"mcp_servers.lookup.args=["server.js"]"#,
            r#"mcp_servers.lookup.env={"API_KEY" = "secret"}"#,
        ]
        ; "stdio_converts_to_mcp_overrides"
    )]
    #[test_case(
        ExtensionConfig::StreamableHttp {
            name: "lookup".into(),
            description: String::new(),
            uri: "http://localhost/mcp".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::from([("Authorization".into(), "Bearer token".into())]),
            timeout: None,
            socket: None,
            bundled: Some(false),
            available_tools: vec![],
        },
        &[
            r#"mcp_servers.lookup.url="http://localhost/mcp""#,
            r#"mcp_servers.lookup.http_headers={"Authorization" = "Bearer token"}"#,
        ]
        ; "streamable_http_converts_to_mcp_overrides"
    )]
    #[test_case(
        ExtensionConfig::StreamableHttp {
            name: "mcp_kiwi_com".into(),
            description: String::new(),
            uri: "https://mcp.kiwi.com".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::new(),
            timeout: None,
            socket: None,
            bundled: None,
            available_tools: vec![],
        },
        &[
            r#"mcp_servers.mcp_kiwi_com.url="https://mcp.kiwi.com""#,
        ]
        ; "resolved_name_used_as_key_http"
    )]
    #[test_case(
        ExtensionConfig::Stdio {
            name: "my-server".into(),
            cmd: "/usr/bin/my-server".into(),
            args: vec![],
            envs: Envs::default(),
            env_keys: vec![],
            description: String::new(),
            timeout: None,
            bundled: None,
            available_tools: vec![],
        },
        &[
            r#"mcp_servers.my-server.command="/usr/bin/my-server""#,
        ]
        ; "resolved_name_used_as_key_stdio"
    )]
    fn test_codex_mcp_overrides(config: ExtensionConfig, expected: &[&str]) {
        let overrides = codex_mcp_config_overrides(&[config]);
        let expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(overrides, expected);
    }

    #[test_case("simple", r#""simple""# ; "no_special_chars")]
    #[test_case(r#"back\slash"#, r#""back\\slash""# ; "backslash")]
    #[test_case(r#"has"quote"#, r#""has\"quote""# ; "double_quote")]
    #[test_case("line\nbreak", r#""line\nbreak""# ; "newline")]
    #[test_case("tab\there", r#""tab\there""# ; "tab")]
    #[test_case("cr\rhere", r#""cr\rhere""# ; "carriage_return")]
    #[test_case("bell\u{0008}here", r#""bell\bhere""# ; "backspace")]
    #[test_case("ff\u{000C}here", r#""ff\fhere""# ; "form_feed")]
    #[test_case("null\u{0000}here", r#""null\u0000here""# ; "null_control_char")]
    fn test_toml_quote(input: &str, expected: &str) {
        assert_eq!(toml_quote(input), expected);
    }

    #[test_case("image/png", ".png" ; "png image")]
    #[test_case("image/jpeg", ".jpg" ; "jpeg image")]
    fn test_prepare_input_image(mime: &str, expected_ext: &str) {
        let dir = tempfile::tempdir().unwrap();
        let messages = vec![Message::user()
            .with_text("Describe")
            .with_image(TEST_IMAGE_B64, mime)];
        let (_prompt, temp_files) = prepare_input("", &messages, dir.path()).unwrap();
        assert_eq!(temp_files.len(), 1);
        let path = temp_files[0].path();
        assert!(
            path.to_str().unwrap().ends_with(expected_ext),
            "expected extension {expected_ext}, got {:?}",
            path
        );
    }

    #[test_case("image/gif" ; "gif")]
    #[test_case("image/webp" ; "webp")]
    #[test_case("image/svg+xml" ; "svg")]
    fn test_prepare_input_image_unsupported(mime: &str) {
        let dir = tempfile::tempdir().unwrap();
        let messages = vec![Message::user()
            .with_text("Describe")
            .with_image(TEST_IMAGE_B64, mime)];
        let err = prepare_input("", &messages, dir.path()).unwrap_err();
        assert!(
            err.to_string().contains("Unsupported image MIME type"),
            "expected unsupported MIME error, got: {err}"
        );
    }

    #[test]
    fn test_prepare_input_tool_request() {
        use rmcp::model::CallToolRequestParams;
        let dir = tempfile::tempdir().unwrap();
        let tool_call = Ok(CallToolRequestParams::new("developer__shell")
            .with_arguments(serde_json::from_value(json!({"cmd": "ls"})).unwrap()));
        let messages = vec![Message::new(
            Role::Assistant,
            0,
            vec![MessageContent::tool_request("call_123", tool_call)],
        )];
        let (prompt, temp_files) = prepare_input("", &messages, dir.path()).unwrap();
        assert!(prompt.contains("[tool_use: developer__shell id=call_123]"));
        assert!(temp_files.is_empty());
    }

    #[test]
    fn test_prepare_input_tool_response() {
        use rmcp::model::{CallToolResult, Content};
        let dir = tempfile::tempdir().unwrap();
        let result = CallToolResult::success(vec![Content::text("file1.txt\nfile2.txt")]);
        let messages = vec![Message::new(
            Role::User,
            0,
            vec![MessageContent::tool_response("call_123", Ok(result))],
        )];
        let (prompt, temp_files) = prepare_input("", &messages, dir.path()).unwrap();
        assert!(prompt.contains("[tool_result id=call_123] file1.txt\nfile2.txt"));
        assert!(temp_files.is_empty());
    }

    #[test]
    fn test_parse_response_plain_text() {
        let provider = CodexProvider {
            command: PathBuf::from("codex"),
            model: ModelConfig::new("gpt-5.2-codex").unwrap(),
            name: "codex".to_string(),
            reasoning_effort: "high".to_string(),
            skip_git_check: false,
            mcp_config_overrides: Vec::new(),
            mode_by_session: tokio::sync::RwLock::new(HashMap::new()),
        };

        let lines = vec!["Hello, world!".to_string()];
        let result = provider.parse_response(&lines);
        assert!(result.is_ok());

        let (message, _usage) = result.unwrap();
        assert_eq!(message.role, Role::Assistant);
        assert!(message.content.len() == 1);
    }

    #[test]
    fn test_parse_response_json_events() {
        let provider = CodexProvider {
            command: PathBuf::from("codex"),
            model: ModelConfig::new("gpt-5.2-codex").unwrap(),
            name: "codex".to_string(),
            reasoning_effort: "high".to_string(),
            skip_git_check: false,
            mcp_config_overrides: Vec::new(),
            mode_by_session: tokio::sync::RwLock::new(HashMap::new()),
        };

        // Test with actual Codex CLI output format
        let lines = vec![
            r#"{"type":"thread.started","thread_id":"test-123"}"#.to_string(),
            r#"{"type":"turn.started"}"#.to_string(),
            r#"{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"Thinking..."}}"#.to_string(),
            r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Hello there!"}}"#.to_string(),
            r#"{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50,"cached_input_tokens":30}}"#.to_string(),
        ];
        let result = provider.parse_response(&lines);
        assert!(result.is_ok());

        let (message, usage) = result.unwrap();
        // Should only contain agent_message text, not reasoning
        if let MessageContent::Text(text) = &message.content[0] {
            assert!(text.text.contains("Hello there!"));
            assert!(!text.text.contains("Thinking"));
        }
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.total_tokens, Some(150));
    }

    #[test]
    fn test_parse_response_empty() {
        let provider = CodexProvider {
            command: PathBuf::from("codex"),
            model: ModelConfig::new("gpt-5.2-codex").unwrap(),
            name: "codex".to_string(),
            reasoning_effort: "high".to_string(),
            skip_git_check: false,
            mcp_config_overrides: Vec::new(),
            mode_by_session: tokio::sync::RwLock::new(HashMap::new()),
        };

        let lines: Vec<String> = vec![];
        let result = provider.parse_response(&lines);
        assert!(result.is_err());
    }

    #[test]
    fn test_reasoning_level_validation() {
        assert!(CODEX_REASONING_LEVELS.contains(&"none"));
        assert!(CODEX_REASONING_LEVELS.contains(&"low"));
        assert!(CODEX_REASONING_LEVELS.contains(&"medium"));
        assert!(CODEX_REASONING_LEVELS.contains(&"high"));
        assert!(CODEX_REASONING_LEVELS.contains(&"xhigh"));
        assert!(!CODEX_REASONING_LEVELS.contains(&"minimal"));
        assert!(!CODEX_REASONING_LEVELS.contains(&"invalid"));
    }

    #[test]
    fn test_reasoning_effort_support_by_model() {
        assert!(CodexProvider::supports_reasoning_effort("gpt-5.2", "none"));
        assert!(!CodexProvider::supports_reasoning_effort(
            "gpt-5.2-codex",
            "none"
        ));
        assert!(CodexProvider::supports_reasoning_effort(
            "gpt-5.2-codex",
            "xhigh"
        ));
    }

    #[test]
    fn test_known_models() {
        assert!(CODEX_KNOWN_MODELS.contains(&"gpt-5.2-codex"));
        assert!(CODEX_KNOWN_MODELS.contains(&"gpt-5.2"));
        assert!(CODEX_KNOWN_MODELS.contains(&"gpt-5.1-codex-max"));
        assert!(CODEX_KNOWN_MODELS.contains(&"gpt-5.1-codex-mini"));
    }

    #[test]
    fn test_parse_response_item_completed() {
        let provider = CodexProvider {
            command: PathBuf::from("codex"),
            model: ModelConfig::new("gpt-5.2-codex").unwrap(),
            name: "codex".to_string(),
            reasoning_effort: "high".to_string(),
            skip_git_check: false,
            mcp_config_overrides: Vec::new(),
            mode_by_session: tokio::sync::RwLock::new(HashMap::new()),
        };

        let lines = vec![
            r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"Hello from codex"}}"#.to_string(),
        ];
        let result = provider.parse_response(&lines);
        assert!(result.is_ok());

        let (message, _usage) = result.unwrap();
        if let MessageContent::Text(text) = &message.content[0] {
            assert!(text.text.contains("Hello from codex"));
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_parse_response_turn_completed_usage() {
        let provider = CodexProvider {
            command: PathBuf::from("codex"),
            model: ModelConfig::new("gpt-5.2-codex").unwrap(),
            name: "codex".to_string(),
            reasoning_effort: "high".to_string(),
            skip_git_check: false,
            mcp_config_overrides: Vec::new(),
            mode_by_session: tokio::sync::RwLock::new(HashMap::new()),
        };

        let lines = vec![
            r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"Response"}}"#.to_string(),
            r#"{"type":"turn.completed","usage":{"input_tokens":5000,"output_tokens":100,"cached_input_tokens":3000}}"#.to_string(),
        ];
        let result = provider.parse_response(&lines);
        assert!(result.is_ok());

        let (_message, usage) = result.unwrap();
        assert_eq!(usage.input_tokens, Some(5000));
        assert_eq!(usage.output_tokens, Some(100));
        assert_eq!(usage.total_tokens, Some(5100));
    }

    #[test_case(
        &[
            r#"{"type":"thread.started","thread_id":"test"}"#,
            r#"{"type":"error","message":"Codex ran out of room in the model's context window and could not finish the task."}"#,
        ],
        ProviderError::ContextLengthExceeded(
            "Codex ran out of room in the model's context window and could not finish the task.".to_string()
        )
        ; "context_window_exceeded"
    )]
    #[test_case(
        &[
            r#"{"type":"thread.started","thread_id":"test"}"#,
            r#"{"type":"error","message":"Rate limit reached for gpt-5.1 in organization on tokens per min (TPM): Limit 30000."}"#,
        ],
        ProviderError::RateLimitExceeded {
            details: "Rate limit reached for gpt-5.1 in organization on tokens per min (TPM): Limit 30000.".to_string(),
            retry_delay: None,
        }
        ; "rate_limit"
    )]
    #[test_case(
        &[
            r#"{"type":"thread.started","thread_id":"test"}"#,
            r#"{"type":"error","message":"You exceeded your current quota, please check your plan and billing details."}"#,
        ],
        ProviderError::RequestFailed(
            "Codex CLI error: You exceeded your current quota, please check your plan and billing details.".to_string()
        )
        ; "quota_exceeded"
    )]
    #[test_case(
        &[
            r#"{"type":"thread.started","thread_id":"test"}"#,
            r#"{"type":"error","message":"Model not supported"}"#,
        ],
        ProviderError::RequestFailed("Codex CLI error: Model not supported".to_string())
        ; "generic_error"
    )]
    #[test_case(
        &[
            r#"{"type":"thread.started","thread_id":"test"}"#,
            r#"{"type":"turn.failed","message":"response.failed event received (connection reset)"}"#,
        ],
        ProviderError::RequestFailed(
            "Codex CLI error: response.failed event received (connection reset)".to_string()
        )
        ; "stream_error"
    )]
    fn test_parse_response_error_event(lines: &[&str], expected: ProviderError) {
        let provider = CodexProvider {
            command: PathBuf::from("codex"),
            model: ModelConfig::new("gpt-5.2-codex").unwrap(),
            name: "codex".to_string(),
            reasoning_effort: "high".to_string(),
            skip_git_check: false,
            mcp_config_overrides: Vec::new(),
            mode_by_session: tokio::sync::RwLock::new(HashMap::new()),
        };

        let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        let result = provider.parse_response(&lines);
        assert_eq!(result.unwrap_err(), expected);
    }

    #[test]
    fn test_parse_response_skips_reasoning() {
        let provider = CodexProvider {
            command: PathBuf::from("codex"),
            model: ModelConfig::new("gpt-5.2-codex").unwrap(),
            name: "codex".to_string(),
            reasoning_effort: "high".to_string(),
            skip_git_check: false,
            mcp_config_overrides: Vec::new(),
            mode_by_session: tokio::sync::RwLock::new(HashMap::new()),
        };

        let lines = vec![
            r#"{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"Let me think about this..."}}"#.to_string(),
            r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"The answer is 42"}}"#.to_string(),
        ];
        let result = provider.parse_response(&lines);
        assert!(result.is_ok());

        let (message, _usage) = result.unwrap();
        if let MessageContent::Text(text) = &message.content[0] {
            assert!(text.text.contains("The answer is 42"));
            assert!(!text.text.contains("Let me think"));
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_session_description_generation() {
        let messages = vec![Message::new(
            Role::User,
            chrono::Utc::now().timestamp(),
            vec![MessageContent::text(
                "This is a very long message that should be truncated to four words",
            )],
        )];

        let result = crate::providers::cli_common::generate_simple_session_description(
            "gpt-5.2-codex",
            &messages,
        );
        assert!(result.is_ok());

        let (message, usage) = result.unwrap();
        assert_eq!(usage.model, "gpt-5.2-codex");
        if let MessageContent::Text(text) = &message.content[0] {
            let word_count = text.text.split_whitespace().count();
            assert!(word_count <= 4);
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_session_description_empty_messages() {
        let messages: Vec<Message> = vec![];

        let result = crate::providers::cli_common::generate_simple_session_description(
            "gpt-5.2-codex",
            &messages,
        );
        assert!(result.is_ok());

        let (message, _usage) = result.unwrap();
        if let MessageContent::Text(text) = &message.content[0] {
            assert_eq!(text.text, "Simple task");
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_config_keys() {
        let metadata = CodexProvider::metadata();
        assert_eq!(metadata.config_keys.len(), 3);

        // First key should be CODEX_COMMAND (required)
        assert_eq!(metadata.config_keys[0].name, "CODEX_COMMAND");
        assert!(metadata.config_keys[0].required);
        assert!(!metadata.config_keys[0].secret);

        // Second key should be CODEX_REASONING_EFFORT (optional)
        assert_eq!(metadata.config_keys[1].name, "CODEX_REASONING_EFFORT");
        assert!(!metadata.config_keys[1].required);

        // Third key should be CODEX_SKIP_GIT_CHECK (optional)
        assert_eq!(metadata.config_keys[2].name, "CODEX_SKIP_GIT_CHECK");
        assert!(!metadata.config_keys[2].required);
    }

    #[test]
    fn test_parse_response_multiple_agent_messages() {
        let provider = CodexProvider {
            command: PathBuf::from("codex"),
            model: ModelConfig::new("gpt-5.2-codex").unwrap(),
            name: "codex".to_string(),
            reasoning_effort: "high".to_string(),
            skip_git_check: false,
            mcp_config_overrides: Vec::new(),
            mode_by_session: tokio::sync::RwLock::new(HashMap::new()),
        };

        let lines = vec![
            r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"First part"}}"#.to_string(),
            r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Second part"}}"#.to_string(),
        ];
        let result = provider.parse_response(&lines);
        assert!(result.is_ok());

        let (message, _usage) = result.unwrap();
        if let MessageContent::Text(text) = &message.content[0] {
            assert!(text.text.contains("First part"));
            assert!(text.text.contains("Second part"));
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_doc_url() {
        assert_eq!(CODEX_DOC_URL, "https://developers.openai.com/codex/cli");
    }

    #[test]
    fn test_default_model() {
        assert_eq!(CODEX_DEFAULT_MODEL, "gpt-5.2-codex");
    }

    #[test_case(GooseMode::Auto, &["--yolo"] ; "auto_yolo")]
    #[test_case(GooseMode::SmartApprove, &["--full-auto"] ; "smart_approve_full_auto")]
    #[test_case(GooseMode::Approve, &[] as &[&str] ; "approve_no_flags")]
    #[test_case(GooseMode::Chat, &["--sandbox", "read-only"] ; "chat_read_only")]
    fn test_apply_permission_flags(mode: GooseMode, expected: &[&str]) {
        let mut cmd = tokio::process::Command::new("codex");
        CodexProvider::apply_permission_flags(&mut cmd, mode).unwrap();
        let args: Vec<&str> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap())
            .collect();
        assert_eq!(args, expected);
    }
}
