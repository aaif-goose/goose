use crate::tools::AcpAwareToolMeta;
use agent_client_protocol_schema::TerminalId;
use async_trait::async_trait;
use fs_err as fs;
use goose::agents::mcp_client::{Error as McpError, McpClientTrait};
use goose::agents::platform_extensions::developer::edit::{
    resolve_path, string_replace, FileEditParams, FileReadParams, FileWriteParams,
};
use goose::agents::platform_extensions::developer::shell::{ShellParams, OUTPUT_LIMIT_BYTES};
use goose::agents::platform_extensions::developer::DeveloperClient;
use rmcp::model::{CallToolResult, Content as RmcpContent, Tool, ToolAnnotations};
use sacp::schema::{
    CreateTerminalRequest, Diff, KillTerminalRequest, ReadTextFileRequest, ReleaseTerminalRequest,
    SessionId, SessionNotification, SessionUpdate, Terminal, TerminalOutputRequest,
    ToolCallContent, ToolCallId, ToolCallLocation, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    WaitForTerminalExitRequest, WriteTextFileRequest,
};
use sacp::{Client, ConnectionTo};
use schemars::schema_for;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

async fn acp_read_text_file(
    cx: &ConnectionTo<Client>,
    session_id: &SessionId,
    path: &Path,
    line: Option<u32>,
    limit: Option<u32>,
) -> Result<String, String> {
    let mut request = ReadTextFileRequest::new(session_id.clone(), path.to_path_buf());
    if let Some(l) = line {
        request = request.line(l);
    }
    if let Some(l) = limit {
        request = request.limit(l);
    }
    let response = cx
        .send_request(request)
        .block_task()
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(response.content)
}

async fn acp_write_text_file(
    cx: &ConnectionTo<Client>,
    session_id: &SessionId,
    path: &Path,
    content: &str,
) -> Result<(), String> {
    let request =
        WriteTextFileRequest::new(session_id.clone(), path.to_path_buf(), content.to_string());
    cx.send_request(request)
        .block_task()
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

pub(crate) struct AcpTools {
    pub(crate) inner: Arc<dyn McpClientTrait>,
    pub(crate) cx: ConnectionTo<Client>,
    pub(crate) session_id: SessionId,
    pub(crate) fs_read: bool,
    pub(crate) fs_write: bool,
    pub(crate) terminal: bool,
}

fn error_result(msg: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![RmcpContent::text(msg.to_string()).with_priority(0.0)])
}

fn fail(action: &str, path: &str, err: impl std::fmt::Display) -> CallToolResult {
    error_result(format!("Failed to {action} {path}: {err}"))
}

fn read_tool() -> Tool {
    let schema = serde_json::to_value(schema_for!(FileReadParams))
        .expect("schema serialization should succeed")
        .as_object()
        .expect("schema should serialize to an object")
        .clone();
    Tool::new("read", "Read a text file from disk.", schema).annotate(
        ToolAnnotations::with_title("Read")
            .read_only(true)
            .destructive(false)
            .idempotent(false)
            .open_world(false),
    )
}

impl AcpTools {
    fn update_tool_call(&self, ctx: &goose::agents::ToolCallContext, fields: ToolCallUpdateFields) {
        if let Some(ref req_id) = ctx.tool_call_request_id {
            let _ = self
                .cx
                .send_notification(SessionNotification::new(
                    self.session_id.clone(),
                    SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                        ToolCallId::new(req_id.clone()),
                        fields,
                    )),
                ))
                .inspect_err(|e| tracing::error!("error updating tool call with client: {}", e));
        }
    }

    fn parse_args<T: serde::de::DeserializeOwned>(
        arguments: Option<rmcp::model::JsonObject>,
    ) -> Result<T, String> {
        DeveloperClient::parse_args(arguments).map_err(|e| format!("Error: {e}"))
    }

    async fn read_content(&self, path: &Path) -> Result<String, String> {
        if self.fs_read {
            acp_read_text_file(&self.cx, &self.session_id, path, None, None).await
        } else {
            fs::read_to_string(path).map_err(|e| e.to_string())
        }
    }

    async fn acp_read(
        &self,
        arguments: Option<rmcp::model::JsonObject>,
        ctx: &goose::agents::ToolCallContext,
    ) -> Result<CallToolResult, McpError> {
        let params: FileReadParams = match Self::parse_args(arguments) {
            Ok(p) => p,
            Err(e) => return Ok(error_result(e)),
        };
        let path = resolve_path(&params.path, ctx.working_dir.as_deref());
        self.update_tool_call(
            ctx,
            ToolCallUpdateFields::new()
                .kind(ToolKind::Read)
                .locations(vec![ToolCallLocation::new(&path)]),
        );
        match acp_read_text_file(&self.cx, &self.session_id, &path, params.line, params.limit).await
        {
            Ok(content) => Ok(CallToolResult::success(vec![
                RmcpContent::text(content).with_priority(0.0)
            ])),
            Err(e) => Ok(fail("read", &params.path, e)),
        }
    }

    async fn acp_write(
        &self,
        arguments: Option<rmcp::model::JsonObject>,
        ctx: &goose::agents::ToolCallContext,
    ) -> Result<CallToolResult, McpError> {
        let params: FileWriteParams = match Self::parse_args(arguments) {
            Ok(p) => p,
            Err(e) => return Ok(error_result(e)),
        };
        let path = resolve_path(&params.path, ctx.working_dir.as_deref());
        self.update_tool_call(
            ctx,
            ToolCallUpdateFields::new()
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new(&path)]),
        );
        match acp_write_text_file(&self.cx, &self.session_id, &path, &params.content).await {
            Ok(()) => {
                self.update_tool_call(
                    ctx,
                    ToolCallUpdateFields::new().content(vec![ToolCallContent::Diff(Diff::new(
                        &path,
                        &params.content,
                    ))]),
                );
                let line_count = params.content.lines().count();
                let action = if path.exists() { "Wrote" } else { "Created" };
                Ok(CallToolResult::success(vec![RmcpContent::text(format!(
                    "{action} {} ({line_count} lines)",
                    params.path
                ))
                .with_priority(0.0)]))
            }
            Err(e) => Ok(fail("write", &params.path, e)),
        }
    }

    async fn acp_edit(
        &self,
        arguments: Option<rmcp::model::JsonObject>,
        ctx: &goose::agents::ToolCallContext,
    ) -> Result<CallToolResult, McpError> {
        let params: FileEditParams = match Self::parse_args(arguments) {
            Ok(p) => p,
            Err(e) => return Ok(error_result(e)),
        };
        let path = resolve_path(&params.path, ctx.working_dir.as_deref());
        self.update_tool_call(
            ctx,
            ToolCallUpdateFields::new()
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new(&path)]),
        );

        let content = match self.read_content(&path).await {
            Ok(c) => c,
            Err(e) => return Ok(fail("read", &params.path, e)),
        };

        let new_content = match string_replace(&content, &params.before, &params.after) {
            Ok(c) => c,
            Err(msg) => return Ok(error_result(msg)),
        };

        let write_result = if self.fs_write {
            acp_write_text_file(&self.cx, &self.session_id, &path, &new_content).await
        } else {
            fs::write(&path, &new_content).map_err(|e| e.to_string())
        };

        match write_result {
            Ok(()) => {
                self.update_tool_call(
                    ctx,
                    ToolCallUpdateFields::new().content(vec![ToolCallContent::Diff(
                        Diff::new(&path, &new_content).old_text(&content),
                    )]),
                );
                let old_lines = params.before.lines().count();
                let new_lines = params.after.lines().count();
                Ok(CallToolResult::success(vec![RmcpContent::text(format!(
                    "Edited {} ({old_lines} lines -> {new_lines} lines)",
                    params.path
                ))
                .with_priority(0.0)]))
            }
            Err(e) => Ok(fail("write", &params.path, e)),
        }
    }

    async fn acp_shell(
        &self,
        arguments: Option<rmcp::model::JsonObject>,
        ctx: &goose::agents::ToolCallContext,
    ) -> Result<CallToolResult, McpError> {
        let params: ShellParams = match Self::parse_args(arguments) {
            Ok(p) => p,
            Err(e) => return Ok(error_result(e)),
        };
        self.update_tool_call(ctx, ToolCallUpdateFields::new().kind(ToolKind::Execute));

        let create_res = self
            .cx
            .send_request(
                CreateTerminalRequest::new(self.session_id.clone(), &params.command)
                    .cwd(ctx.working_dir.clone())
                    .output_byte_limit(OUTPUT_LIMIT_BYTES as u64),
            )
            .block_task()
            .await
            .map_err(|e| {
                McpError::McpError(rmcp::model::ErrorData::new(
                    rmcp::model::ErrorCode::INTERNAL_ERROR,
                    format!("failed to create terminal: {e:?}"),
                    None,
                ))
            })?;
        let terminal_id = create_res.terminal_id;

        self.update_tool_call(
            ctx,
            ToolCallUpdateFields::new().content(vec![ToolCallContent::Terminal(Terminal::new(
                terminal_id.clone(),
            ))]),
        );

        let result = self
            .run_terminal_to_completion(&terminal_id, params.timeout_secs)
            .await;

        // Always release the terminal, even if we hit errors above.
        let _ = self
            .cx
            .send_request(ReleaseTerminalRequest::new(
                self.session_id.clone(),
                terminal_id.clone(),
            ))
            .block_task()
            .await
            .inspect_err(|e| tracing::error!("failed to release terminal: {e:?}"));

        let output_res = result?;

        let exit_code = output_res
            .exit_status
            .and_then(|s| s.exit_code)
            .unwrap_or_default();

        let content = vec![
            RmcpContent::text(format!("exit code: {exit_code}")).with_priority(0.0),
            RmcpContent::text(output_res.output).with_priority(0.0),
        ];

        if exit_code != 0 {
            Ok(CallToolResult::error(content))
        } else {
            Ok(CallToolResult::success(content))
        }
    }

    async fn run_terminal_to_completion(
        &self,
        terminal_id: &TerminalId,
        timeout_secs: Option<u64>,
    ) -> Result<sacp::schema::TerminalOutputResponse, McpError> {
        let wait_fut = self
            .cx
            .send_request(WaitForTerminalExitRequest::new(
                self.session_id.clone(),
                terminal_id.clone(),
            ))
            .block_task();

        let timed_out = match timeout_secs {
            Some(secs) if secs > 0 => match timeout(Duration::from_secs(secs), wait_fut).await {
                Ok(res) => {
                    res.map_err(|e| {
                        McpError::McpError(rmcp::model::ErrorData::new(
                            rmcp::model::ErrorCode::INTERNAL_ERROR,
                            format!("failed to wait for terminal exit: {e:?}"),
                            None,
                        ))
                    })?;
                    false
                }
                Err(_) => {
                    let _ = self
                        .cx
                        .send_request(KillTerminalRequest::new(
                            self.session_id.clone(),
                            terminal_id.clone(),
                        ))
                        .block_task()
                        .await
                        .inspect_err(|e| tracing::error!("failed to kill terminal: {e:?}"));
                    true
                }
            },
            _ => {
                wait_fut.await.map_err(|e| {
                    McpError::McpError(rmcp::model::ErrorData::new(
                        rmcp::model::ErrorCode::INTERNAL_ERROR,
                        format!("failed to wait for terminal exit: {e:?}"),
                        None,
                    ))
                })?;
                false
            }
        };

        let mut output_res = self
            .cx
            .send_request(TerminalOutputRequest::new(
                self.session_id.clone(),
                terminal_id.clone(),
            ))
            .block_task()
            .await
            .map_err(|e| {
                McpError::McpError(rmcp::model::ErrorData::new(
                    rmcp::model::ErrorCode::INTERNAL_ERROR,
                    format!("failed to get terminal output: {e:?}"),
                    None,
                ))
            })?;

        if timed_out {
            output_res.output.push_str(&format!(
                "\n\nCommand timed out after {} seconds",
                timeout_secs.unwrap_or(0)
            ));
        }

        Ok(output_res)
    }
}

#[async_trait]
impl McpClientTrait for AcpTools {
    async fn list_tools(
        &self,
        session_id: &str,
        next_cursor: Option<String>,
        cancellation_token: CancellationToken,
    ) -> Result<rmcp::model::ListToolsResult, McpError> {
        let mut result = self
            .inner
            .list_tools(session_id, next_cursor, cancellation_token)
            .await?;
        if self.fs_read {
            result.tools.insert(0, read_tool());
        }
        Ok(result)
    }

    async fn call_tool(
        &self,
        ctx: &goose::agents::ToolCallContext,
        name: &str,
        arguments: Option<rmcp::model::JsonObject>,
        cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, McpError> {
        match name {
            "read" if self.fs_read => self
                .acp_read(arguments, ctx)
                .await
                .map(|r| r.with_acp_aware_meta()),
            "write" if self.fs_write => self
                .acp_write(arguments, ctx)
                .await
                .map(|r| r.with_acp_aware_meta()),
            "edit" if self.fs_read && self.fs_write => self
                .acp_edit(arguments, ctx)
                .await
                .map(|r| r.with_acp_aware_meta()),
            "shell" if self.terminal => self
                .acp_shell(arguments, ctx)
                .await
                .map(|r| r.with_acp_aware_meta()),
            _ => {
                self.inner
                    .call_tool(ctx, name, arguments, cancellation_token)
                    .await
            }
        }
    }

    fn get_info(&self) -> Option<&rmcp::model::InitializeResult> {
        self.inner.get_info()
    }
}
