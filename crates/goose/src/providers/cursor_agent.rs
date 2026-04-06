use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::Role;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::base::{
    stream_from_single_message, ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata,
    ProviderUsage, Usage,
};
use super::errors::ProviderError;
use super::utils::{filter_extensions_from_system_prompt, RequestLog};
use crate::config::base::CursorAgentCommand;
use crate::config::search_path::SearchPaths;
use crate::conversation::message::{Message, MessageContent};
use crate::model::ModelConfig;
use crate::subprocess::configure_subprocess;
use futures::future::BoxFuture;
use rmcp::model::Tool;

const CURSOR_AGENT_PROVIDER_NAME: &str = "cursor-agent";
pub const CURSOR_AGENT_DEFAULT_MODEL: &str = "auto";
pub const CURSOR_AGENT_KNOWN_MODELS: &[&str] = &["auto", "composer-2", "composer-2-fast"];

pub const CURSOR_AGENT_DOC_URL: &str = "https://docs.cursor.com/en/cli/overview";

#[derive(Debug, serde::Serialize)]
pub struct CursorAgentProvider {
    command: PathBuf,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
}

impl CursorAgentProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();
        let command: String = config.get_cursor_agent_command().unwrap_or_default().into();
        let resolved_command = SearchPaths::builder().with_npm().resolve(&command)?;

        Ok(Self {
            command: resolved_command,
            model,
            name: CURSOR_AGENT_PROVIDER_NAME.to_string(),
        })
    }

    /// Get authentication status from cursor-agent
    async fn get_authentication_status(&self) -> bool {
        Command::new(&self.command)
            .arg("status")
            .output()
            .await
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("✓ Logged in as"))
            .unwrap_or(false)
    }

    /// Convert goose messages to a simple prompt format for cursor-agent CLI
    fn messages_to_cursor_agent_format(&self, system: &str, messages: &[Message]) -> String {
        let mut full_prompt = String::new();

        let filtered_system = filter_extensions_from_system_prompt(system);
        full_prompt.push_str(&filtered_system);
        full_prompt.push_str("\n\n");

        // Add conversation history
        for message in messages {
            let role_prefix = match message.role {
                Role::User => "Human: ",
                Role::Assistant => "Assistant: ",
            };
            full_prompt.push_str(role_prefix);

            for content in &message.content {
                match content {
                    MessageContent::Text(text_content) => {
                        full_prompt.push_str(&text_content.text);
                        full_prompt.push('\n');
                    }
                    MessageContent::ToolRequest(tool_request) => {
                        if let Ok(tool_call) = &tool_request.tool_call {
                            full_prompt.push_str(&format!(
                                "Tool Use: {} with args: {:?}\n",
                                tool_call.name, tool_call.arguments
                            ));
                        }
                    }
                    MessageContent::ToolResponse(tool_response) => {
                        if let Ok(result) = &tool_response.tool_result {
                            let content_text = result
                                .content
                                .iter()
                                .filter_map(|content| match &content.raw {
                                    rmcp::model::RawContent::Text(text_content) => {
                                        Some(text_content.text.as_str())
                                    }
                                    _ => None,
                                })
                                .collect::<Vec<&str>>()
                                .join("\n");

                            full_prompt.push_str(&format!("Tool Result: {}\n", content_text));
                        }
                    }
                    _ => {
                        // Skip other content types for now
                    }
                }
            }
            full_prompt.push('\n');
        }

        full_prompt.push_str("Assistant: ");
        full_prompt
    }

    /// Parse the JSON response from cursor-agent CLI
    fn parse_cursor_agent_response(
        &self,
        lines: &[String],
    ) -> Result<(Message, Usage), ProviderError> {
        // Try parsing each line as a JSON object and find the one with type="result"
        for line in lines {
            if let Ok(json_value) = serde_json::from_str::<Value>(line) {
                if let Some(type_val) = json_value.get("type") {
                    if type_val == "result" {
                        let text_content = if let Some(result) = json_value.get("result") {
                            let result_str = result.as_str().unwrap_or("").to_string();

                            if result_str.is_empty() {
                                if json_value
                                    .get("is_error")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false)
                                {
                                    "Error: cursor-agent returned an error response".to_string()
                                } else {
                                    "cursor-agent completed successfully but returned no content"
                                        .to_string()
                                }
                            } else {
                                result_str
                            }
                        } else {
                            format!("Raw cursor-agent response: {}", line)
                        };

                        let message_content = vec![MessageContent::text(text_content)];
                        let response_message = Message::new(
                            Role::Assistant,
                            chrono::Utc::now().timestamp(),
                            message_content,
                        );

                        let usage = Usage::default();

                        return Ok((response_message, usage));
                    }
                }
            }
        }

        // If no valid result line found, fall back to joining all lines
        let response_text = lines.join("\n");

        let message_content = vec![MessageContent::text(response_text)];
        let response_message = Message::new(
            Role::Assistant,
            chrono::Utc::now().timestamp(),
            message_content,
        );
        let usage = Usage::default();
        Ok((response_message, usage))
    }

    async fn execute_command(
        &self,
        system: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<Vec<String>, ProviderError> {
        let prompt = self.messages_to_cursor_agent_format(system, messages);

        if std::env::var("GOOSE_CURSOR_AGENT_DEBUG").is_ok() {
            println!("=== CURSOR AGENT PROVIDER DEBUG ===");
            println!("Command: {:?}", self.command);
            println!("Original system prompt length: {} chars", system.len());
            println!(
                "Filtered system prompt length: {} chars",
                filter_extensions_from_system_prompt(system).len()
            );
            println!("Full prompt: {}", prompt);
            println!("Model: {}", self.model.model_name);
            println!("================================");
        }

        let mut cmd = Command::new(&self.command);
        configure_subprocess(&mut cmd);

        if let Ok(path) = SearchPaths::builder().with_npm().path() {
            cmd.env("PATH", path);
        }

        cmd.arg("--model").arg(&self.model.model_name);

        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("json")
            .arg("--force");

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd
                .spawn()
                .map_err(|e| ProviderError::RequestFailed(format!(
                    "Failed to spawn cursor-agent CLI command '{:?}': {}. \
                    Make sure the cursor-agent CLI is installed and available in the configured search paths, or set CURSOR_AGENT_COMMAND in your config to the correct path.",
                    self.command, e
                )))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProviderError::RequestFailed("Failed to capture stdout".to_string()))?;

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

        if !exit_status.success() {
            if !self.get_authentication_status().await {
                return Err(ProviderError::Authentication(
                    "You are not logged in to cursor-agent. Please run 'cursor-agent login' to authenticate first."
                        .to_string()));
            }
            return Err(ProviderError::RequestFailed(format!(
                "Command failed with exit code: {:?}",
                exit_status.code()
            )));
        }

        tracing::debug!("Command executed successfully, got {} lines", lines.len());
        for (i, line) in lines.iter().enumerate() {
            tracing::debug!("Line {}: {}", i, line);
        }

        Ok(lines)
    }
}

impl ProviderDef for CursorAgentProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            CURSOR_AGENT_PROVIDER_NAME,
            "Cursor Agent",
            "Execute AI models via cursor-agent CLI tool",
            CURSOR_AGENT_DEFAULT_MODEL,
            CURSOR_AGENT_KNOWN_MODELS.to_vec(),
            CURSOR_AGENT_DOC_URL,
            vec![ConfigKey::from_value_type::<CursorAgentCommand>(
                true, false, true,
            )],
        )
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(model))
    }
}

#[async_trait]
impl Provider for CursorAgentProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        // Return the model config with appropriate context limit for Cursor models
        self.model.clone()
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(CURSOR_AGENT_KNOWN_MODELS
            .iter()
            .map(|s| s.to_string())
            .collect())
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        _session_id: &str, // CLI has no external session-id flag to propagate.
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        if super::cli_common::is_session_description_request(system) {
            let (message, provider_usage) = super::cli_common::generate_simple_session_description(
                &model_config.model_name,
                messages,
            )?;
            return Ok(stream_from_single_message(message, provider_usage));
        }

        let lines = self.execute_command(system, messages, tools).await?;

        let (message, usage) = self.parse_cursor_agent_response(&lines)?;

        // Create a dummy payload for debug tracing
        let payload = json!({
            "command": self.command,
            "model": model_config.model_name,
            "system": system,
            "messages": messages.len()
        });

        let response = json!({
            "lines": lines.len(),
            "usage": usage
        });

        let mut log = RequestLog::start(&self.model, &payload)?;
        log.write(&response, Some(&usage))?;

        let provider_usage = ProviderUsage::new(model_config.model_name.clone(), usage);
        Ok(stream_from_single_message(message, provider_usage))
    }
}
