use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;
use crate::session::extension_data;
use crate::session::extension_data::ExtensionState;
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool, ToolAnnotations,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "todo";

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TodoWriteParams {
    content: String,
}

pub struct TodoClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
}

impl TodoClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("Todo"),
            )
            .with_instructions(
                indoc! {r#"
                Your todo content is automatically available in your context.

                Workflow:
                - Start: write initial checklist
                - During: update progress
                - End: verify all complete

                Template:
                - [x] Requirement 1
                - [ ] Task
                  - [ ] Sub-task
                - [ ] Requirement 2
                - [ ] Another task
            "#}
                .to_string(),
            );

        Ok(Self { info, context })
    }

    async fn handle_write_todo(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<ContentBlock>, String> {
        let content = arguments
            .as_ref()
            .ok_or("Missing arguments")?
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: content")?
            .to_string();

        let char_count = content.chars().count();
        let max_chars = std::env::var("GOOSE_TODO_MAX_CHARS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50_000);

        if max_chars > 0 && char_count > max_chars {
            return Err(format!(
                "Todo list too large: {} chars (max: {})",
                char_count, max_chars
            ));
        }

        let manager = &self.context.session_manager;
        match manager.get_session(session_id, false).await {
            Ok(mut session) => {
                let todo_state = extension_data::TodoState::new(content);
                if todo_state
                    .to_extension_data(&mut session.extension_data)
                    .is_ok()
                {
                    match manager
                        .update(session_id)
                        .extension_data(session.extension_data)
                        .apply()
                        .await
                    {
                        Ok(_) => Ok(vec![ContentBlock::text(format!(
                            "Updated ({} chars)",
                            char_count
                        ))]),
                        Err(_) => Err("Failed to update session metadata".to_string()),
                    }
                } else {
                    Err("Failed to serialize TODO state".to_string())
                }
            }
            Err(_) => Err("Failed to read session metadata".to_string()),
        }
    }

    fn get_tools() -> Vec<Tool> {
        let schema = schema_for!(TodoWriteParams);
        let schema_value =
            serde_json::to_value(schema).expect("Failed to serialize TodoWriteParams schema");

        vec![Tool::new(
            "todo_write".to_string(),
            indoc! {r#"
                    Overwrite the entire TODO content.

                    The content persists across conversation turns and compaction. Use this for:
                    - Task tracking and progress updates
                    - Important notes and reminders

                    WARNING: This operation completely replaces the existing content. Always include
                    all content you want to keep, not just the changes.
                "#}
            .to_string(),
            schema_value.as_object().unwrap().clone(),
        )
        .annotate(ToolAnnotations::from_raw(
            Some("Write TODO".to_string()),
            Some(false),
            Some(true),
            Some(false),
            Some(false),
        ))]
    }
}

#[async_trait]
impl McpClientTrait for TodoClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let session_id = &ctx.session_id;
        let content = match name {
            "todo_write" => self.handle_write_todo(session_id, arguments).await,
            _ => Err(format!("Unknown tool: {}", name)),
        };

        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error: {}",
                error
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn get_moim(&self, session_id: &str) -> Option<String> {
        let metadata = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
            .ok()?;

        match extension_data::TodoState::from_extension_data(&metadata.extension_data) {
            Some(state) if !state.content.trim().is_empty() => {
                Some(format!("Current tasks and notes:\n{}\n", state.content))
            }
            _ => Some(
                "Current tasks and notes:\nOnce given a task, immediately update your todo with all explicit and implicit requirements\n"
                    .to_string(),
            ),
        }
    }
}
