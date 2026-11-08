use anyhow::Result;
use goose_plugin::PluginRuntime;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, ErrorCode, ErrorData, Implementation, InitializeResult,
        ServerCapabilities, ServerConfig,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone)]
struct SessionAudit {
    tool_router: ToolRouter<Self>,
    log_file: PathBuf,
}

impl SessionAudit {
    fn new() -> Self {
        let log_file = std::env::var("GOOSE_PLUGIN_SESSION_AUDIT_LOG_FILE")
            .unwrap_or_else(|_| {
                dirs_next::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".goose")
                    .join("audit.log")
                    .to_string_lossy()
                    .to_string()
            });
        SessionAudit {
            tool_router: Self::tool_router(),
            log_file: PathBuf::from(log_file),
        }
    }
}

fn expand_home(path: &PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(&s[2..]);
        }
    }
    path.clone()
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ReadAuditLogInput {
    #[schemars(description = "Number of recent log lines to return (default 20, max 200)")]
    lines: Option<u32>,
    #[schemars(description = "Filter by session ID (optional)")]
    session_id: Option<String>,
    #[schemars(description = "Filter by event type, e.g. PostToolUse (optional)")]
    event_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ClearAuditLogInput {
    #[schemars(description = "Confirm clearing the log by passing true")]
    confirm: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct AuditStatsInput {
    #[schemars(description = "Session ID to filter stats for (optional, all sessions if omitted)")]
    session_id: Option<String>,
}

#[tool_router(router = tool_router)]
impl SessionAudit {
    #[tool(description = "Read recent entries from the audit log")]
    async fn read_audit_log(
        &self,
        params: Parameters<ReadAuditLogInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let limit = p.lines.unwrap_or(20).min(200) as usize;
        let log_path = expand_home(&self.log_file);

        let content = match tokio::fs::read_to_string(&log_path).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CallToolResult::success(vec![ContentBlock::text(
                    "Audit log is empty (no events recorded yet).".to_string(),
                )]));
            }
            Err(e) => {
                return Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    e.to_string(),
                    None,
                ))
            }
        };

        let lines: Vec<&str> = content.lines().collect();
        let filtered: Vec<&str> = lines
            .iter()
            .filter(|line| {
                if let Some(sid) = &p.session_id {
                    if !line.contains(sid.as_str()) {
                        return false;
                    }
                }
                if let Some(ev) = &p.event_type {
                    if !line.contains(ev.as_str()) {
                        return false;
                    }
                }
                true
            })
            .copied()
            .collect();

        let tail: Vec<&str> = filtered.iter().rev().take(limit).rev().copied().collect();
        if tail.is_empty() {
            return Ok(CallToolResult::success(vec![ContentBlock::text(
                "No matching log entries.".to_string(),
            )]));
        }

        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "Last {} entries (of {}):\n{}",
            tail.len(),
            filtered.len(),
            tail.join("\n")
        ))]))
    }

    #[tool(description = "Get statistics from the audit log: event counts per type and session")]
    async fn audit_stats(
        &self,
        params: Parameters<AuditStatsInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let log_path = expand_home(&self.log_file);

        let content = match tokio::fs::read_to_string(&log_path).await {
            Ok(c) => c,
            Err(_) => {
                return Ok(CallToolResult::success(vec![ContentBlock::text(
                    "No audit log found.".to_string(),
                )]))
            }
        };

        let mut counts: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        let mut tool_counts: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();

        for line in content.lines() {
            if let Some(ref sid) = p.session_id {
                if !line.contains(sid.as_str()) {
                    continue;
                }
            }
            for event in [
                "SessionStart",
                "SessionEnd",
                "PostToolUse",
                "PreToolUse",
                "Stop",
            ] {
                if line.contains(event) {
                    *counts.entry(event.to_string()).or_default() += 1;
                }
            }
            if let Some(tool_part) = line.split("tool=").nth(1) {
                let tool_name = tool_part.split_whitespace().next().unwrap_or("unknown");
                *tool_counts.entry(tool_name.to_string()).or_default() += 1;
            }
        }

        let mut lines = vec!["Event counts:".to_string()];
        let mut event_entries: Vec<_> = counts.into_iter().collect();
        event_entries.sort_by(|a, b| b.1.cmp(&a.1));
        for (ev, count) in &event_entries {
            lines.push(format!("  {ev}: {count}"));
        }
        if !tool_counts.is_empty() {
            lines.push("Top tools called:".to_string());
            let mut tool_entries: Vec<_> = tool_counts.into_iter().collect();
            tool_entries.sort_by(|a, b| b.1.cmp(&a.1));
            for (tool, count) in tool_entries.iter().take(10) {
                lines.push(format!("  {tool}: {count}"));
            }
        }

        Ok(CallToolResult::success(vec![ContentBlock::text(
            lines.join("\n"),
        )]))
    }

    #[tool(description = "Clear the audit log file")]
    async fn clear_audit_log(
        &self,
        params: Parameters<ClearAuditLogInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        if !params.0.confirm {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "Pass confirm: true to clear the audit log".to_string(),
                None,
            ));
        }
        let log_path = expand_home(&self.log_file);
        match tokio::fs::write(&log_path, "").await {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "Audit log cleared: {}",
                log_path.display()
            ))])),
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                e.to_string(),
                None,
            )),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SessionAudit {
    fn get_info(&self) -> ServerConfig {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("session-audit", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Session audit tools: read_audit_log, audit_stats, clear_audit_log. Hooks automatically log session events (SessionStart, SessionEnd, PostToolUse) to the configured log file.".to_string(),
            )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    PluginRuntime::run(SessionAudit::new()).await
}
