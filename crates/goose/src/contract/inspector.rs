use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use super::AgentContract;
use crate::config::GooseMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::session::SessionManager;
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};

const PATH_ARG_KEYS: &[&str] = &[
    "path",
    "file",
    "filepath",
    "file_path",
    "filename",
    "dir",
    "directory",
    "folder",
    "source",
    "destination",
    "target",
    "output",
    "output_path",
    "root",
    "cwd",
];

const COMMAND_ARG_KEYS: &[&str] = &["command", "cmd", "script"];

/// Enforces the repository's `agent.yaml` contract: tool calls that touch a
/// `forbidden_paths` pattern or contain a `forbidden_commands` string are
/// denied outright, no matter what permission mode the session runs in.
pub struct ContractInspector {
    session_manager: Arc<SessionManager>,
}

impl ContractInspector {
    pub fn new(session_manager: Arc<SessionManager>) -> Self {
        Self { session_manager }
    }

    async fn working_dir(&self, session_id: &str) -> PathBuf {
        self.session_manager
            .get_session(session_id, false)
            .await
            .map(|session| session.working_dir)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

/// Returns `(base_dir, token)` pairs: path-ish args resolve against the
/// session working dir, while tokens inside shell commands resolve against a
/// cwd tracked across `cd`/`pushd`/`chdir` within that command string.
fn extract_path_candidates(
    arguments: &serde_json::Map<String, serde_json::Value>,
    working_dir: &Path,
) -> Vec<(PathBuf, String)> {
    let mut candidates = Vec::new();
    for key in PATH_ARG_KEYS {
        if let Some(value) = arguments.get(*key).and_then(|v| v.as_str()) {
            candidates.push((working_dir.to_path_buf(), value.to_string()));
        }
    }
    for key in COMMAND_ARG_KEYS {
        if let Some(command) = arguments.get(*key).and_then(|v| v.as_str()) {
            let tokens = shell_words::split(command).unwrap_or_default();
            let mut shell_cwd = working_dir.to_path_buf();
            for (i, token) in tokens.iter().enumerate() {
                if matches!(token.as_str(), "cd" | "pushd" | "chdir") {
                    if let Some(target) = tokens.get(i + 1) {
                        if !target.starts_with('-') {
                            candidates.push((shell_cwd.clone(), target.clone()));
                            shell_cwd = if Path::new(target).is_absolute() {
                                PathBuf::from(target)
                            } else {
                                shell_cwd.join(target)
                            };
                        }
                    }
                    continue;
                }
                if !token.starts_with('-')
                    && (token.contains('/') || token.contains('\\') || token.contains('.'))
                {
                    candidates.push((shell_cwd.clone(), token.clone()));
                }
            }
        }
    }
    candidates
}

fn extract_command_texts(arguments: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    COMMAND_ARG_KEYS
        .iter()
        .filter_map(|key| {
            arguments
                .get(*key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}

#[async_trait]
impl ToolInspector for ContractInspector {
    fn name(&self) -> &'static str {
        "contract"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        session_id: &str,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        _goose_mode: GooseMode,
    ) -> Result<Vec<InspectionResult>> {
        let working_dir = self.working_dir(session_id).await;
        let Some(loaded) = AgentContract::discover(&working_dir) else {
            return Ok(Vec::new());
        };

        let mut results = Vec::new();
        for tool_request in tool_requests {
            let Ok(tool_call) = &tool_request.tool_call else {
                continue;
            };
            let Some(arguments) = tool_call.arguments.as_ref() else {
                continue;
            };

            for command in extract_command_texts(arguments) {
                if let Some(forbidden) = loaded
                    .contract
                    .forbidden_commands
                    .iter()
                    .find(|f| command.contains(f.as_str()))
                {
                    results.push(deny_result(
                        tool_request,
                        format!(
                            "command contains forbidden string {:?} declared in agent.yaml",
                            forbidden
                        ),
                    ));
                    break;
                }
            }
            if results.iter().any(|r| r.tool_request_id == tool_request.id) {
                continue;
            }

            for (base, candidate) in extract_path_candidates(arguments, &working_dir) {
                if let Some(pattern) = loaded.forbidden_match(&base, &candidate) {
                    results.push(deny_result(
                        tool_request,
                        format!(
                            "{:?} matches forbidden path pattern {:?} declared in agent.yaml",
                            candidate, pattern
                        ),
                    ));
                    break;
                }
            }
        }

        Ok(results)
    }
}

fn deny_result(tool_request: &ToolRequest, reason: String) -> InspectionResult {
    tracing::info!(
        security.event_type = "agent_contract",
        security.action = "BLOCK",
        tool.request_id = %tool_request.id,
        contract.reason = %reason,
        "agent.yaml contract violation"
    );
    InspectionResult {
        tool_request_id: tool_request.id.clone(),
        action: InspectionAction::Deny,
        reason,
        confidence: 1.0,
        inspector_name: "contract".to_string(),
        finding_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::object;

    #[test]
    fn test_extract_path_candidates() {
        let wd = Path::new("/repo");
        let args = object!({
            "path": "src/.env",
            "command": "cat secrets/api.key && cargo build"
        });
        let candidates = extract_path_candidates(&args, wd);
        assert!(candidates.contains(&(wd.to_path_buf(), "src/.env".to_string())));
        assert!(candidates.contains(&(wd.to_path_buf(), "secrets/api.key".to_string())));
        assert!(!candidates.iter().any(|(_, c)| c == "cargo"));
    }

    #[test]
    fn test_extract_path_candidates_cd_target() {
        let wd = Path::new("/repo");
        let args = object!({
            "command": "cd nested && cat .env"
        });
        let candidates = extract_path_candidates(&args, wd);
        assert!(candidates.contains(&(wd.to_path_buf(), "nested".to_string())));
        assert!(candidates.contains(&(wd.join("nested"), ".env".to_string())));
    }

    #[test]
    fn test_extract_command_texts() {
        let args = object!({
            "command": "rm -rf target",
            "path": "src/main.rs"
        });
        let commands = extract_command_texts(&args);
        assert_eq!(commands, vec!["rm -rf target".to_string()]);
    }
}
