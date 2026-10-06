//! Makes filesystem skills available to inference and slash commands.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_sdk_types::custom_requests::{SourceEntry, SourceType};
use rmcp::model::{CallToolResult, ContentBlock, ErrorData, JsonObject, Tool};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::Value;
use tracing_futures::Instrument;

use crate::agents::extension_manager::ExtensionLease;
use crate::agents::state_machine::ops_toolcalling::{
    emit_post_tool_use, pending_advertised_tool_requests, run_pre_tool_hooks, tool_span,
    ToolDisposition, EXPIRED_APPROVAL_RESPONSE,
};
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, yielded_with, ConversationEffect, Emitter,
    GooseEffect, Operation, OperationResult, SlashCommand,
};
use crate::agents::tool_execution::{CHAT_MODE_TOOL_SKIPPED_RESPONSE, DECLINED_RESPONSE};
use crate::config::GooseMode;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::hooks::HookManager;
use crate::session::Session;

const LOAD_SKILL_TOOL_NAME: &str = "load_skill";

pub struct SkillOperation {
    hook_manager: HookManager,
    extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
}

#[derive(Deserialize, JsonSchema)]
struct LoadSkillParams {
    /// Name of the skill to load. Use "skill-name/path" to load a supporting file.
    name: String,
    /// Optional arguments to provide when loading the skill.
    #[serde(default)]
    args: Option<String>,
}

fn skill_tool() -> Result<Tool> {
    let schema = serde_json::to_value(schema_for!(LoadSkillParams))?;
    let schema = schema
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("load_skill schema is not an object"))?;
    Ok(Tool::new(
        LOAD_SKILL_TOOL_NAME,
        "Load a skill's full content into your context so you can follow its instructions.\n\n\
         Skills are listed in your system instructions. When you need to use one, \
         load it first to get the detailed instructions.",
        schema,
    ))
}

fn skill_entries(
    working_dir: &Path,
    mcp: Vec<crate::skills::mcp::SkillRecord>,
) -> Vec<SourceEntry> {
    let filesystem = crate::skills::discover_skills(Some(working_dir));
    crate::skills::mcp::merge_skill_entries(filesystem, mcp)
        .into_iter()
        .filter(|source| {
            matches!(
                source.source_type,
                SourceType::Skill | SourceType::BuiltinSkill
            )
        })
        .collect()
}

fn render_skill_instructions(skills: &[&SourceEntry]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }
    let mut ordered: Vec<&SourceEntry> = skills.to_vec();
    ordered.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
    let mut instructions = String::from(
        "# Skills\n\nYou have these skills at your disposal. Load one when it can help with the task or when the user asks for it:",
    );
    for skill in ordered {
        instructions.push_str(&format!("\n- {}: {}", skill.name, skill.description));
    }
    Some(instructions)
}

fn current_lease(
    extension_lease: &Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    session: &Session,
) -> Option<Arc<ExtensionLease>> {
    let lease = extension_lease
        .lock()
        .expect("extension lease unavailable")
        .clone()?;
    (lease.scope_id() == session.id.as_str()).then_some(lease)
}

async fn execute_skill(
    working_dir: &Path,
    arguments: Option<JsonObject>,
    lease: Option<&ExtensionLease>,
) -> CallToolResult {
    let params = arguments
        .map(Value::Object)
        .ok_or_else(|| "Missing arguments".to_string())
        .and_then(|arguments| {
            serde_json::from_value::<LoadSkillParams>(arguments)
                .map_err(|error| format!("Invalid arguments: {error}"))
        });
    let params = match params {
        Ok(params) => params,
        Err(error) => return CallToolResult::error(vec![ContentBlock::text(error)]),
    };
    let requested = params.name.as_str();
    let (skill_name, relative_file) = match requested.split_once('/') {
        Some((name, path)) => (name, Some(path.replace('\\', "/"))),
        None => (requested, None),
    };
    let args = params.args.as_deref();
    let mcp = match lease {
        Some(lease) => lease.list_mcp_skills().await,
        None => Vec::new(),
    };
    let skills = skill_entries(working_dir, mcp);

    if let Some(skill) = skills.iter().find(|skill| skill.name == skill_name) {
        if let Some(relative_path) = relative_file.as_deref() {
            if crate::skills::mcp::is_mcp_skill(skill) {
                return load_mcp_supporting_file(skill, relative_path, lease).await;
            }
            return load_supporting_file(skill, requested, relative_path);
        }
        if crate::skills::mcp::is_mcp_skill(skill) {
            return load_mcp_skill(skill, args, lease).await;
        }
        return match crate::skills::loaded_skill_context_with_args(skill, args) {
            Ok(rendered) => CallToolResult::success(vec![ContentBlock::text(rendered)]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to parse skill arguments: {error}"
            ))]),
        };
    }

    let suggestions: Vec<&str> = skills
        .iter()
        .filter(|skill| {
            skill
                .name
                .to_lowercase()
                .contains(&skill_name.to_lowercase())
                || skill_name
                    .to_lowercase()
                    .contains(&skill.name.to_lowercase())
        })
        .take(3)
        .map(|skill| skill.name.as_str())
        .collect();
    if suggestions.is_empty() {
        CallToolResult::error(vec![ContentBlock::text(format!(
            "Skill '{skill_name}' not found."
        ))])
    } else {
        CallToolResult::error(vec![ContentBlock::text(format!(
            "Skill '{skill_name}' not found. Did you mean: {}?",
            suggestions.join(", ")
        ))])
    }
}

fn skill_error(message: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.to_string())])
}

async fn load_mcp_skill(
    skill: &SourceEntry,
    args: Option<&str>,
    lease: Option<&ExtensionLease>,
) -> CallToolResult {
    let Some(lease) = lease else {
        return skill_error("Skill server is not connected.");
    };
    let Some(extension_name) = skill
        .properties
        .get("mcpExtension")
        .and_then(|value| value.as_str())
    else {
        return skill_error(format!("Skill '{}' has no serving extension.", skill.name));
    };
    let got = match lease.send_skill_get(extension_name, &skill.path).await {
        Ok(record) => record,
        Err(error) => return skill_error(error),
    };
    if got.name != skill.name {
        return skill_error(crate::skills::mcp::SkillLoadError::InvalidName {
            uri: skill.path.clone(),
        });
    }
    let bytes = match lease.read_skill_text(extension_name, &skill.path).await {
        Ok(text) => text,
        Err(error) => return skill_error(error),
    };
    if let Err(error) = crate::skills::mcp::verify_file(&got, &skill.path, bytes.as_bytes()) {
        return skill_error(error);
    }
    let mut loaded = skill.clone();
    loaded.description = got.description;
    loaded.content = bytes;
    loaded.supporting_files.clear();
    match crate::skills::loaded_skill_context_with_args(&loaded, args) {
        Ok(rendered) => CallToolResult::success(vec![ContentBlock::text(rendered)]),
        Err(error) => skill_error(format!("Failed to parse skill arguments: {error}")),
    }
}

async fn load_mcp_supporting_file(
    skill: &SourceEntry,
    relative_path: &str,
    lease: Option<&ExtensionLease>,
) -> CallToolResult {
    let Some(lease) = lease else {
        return skill_error("Skill server is not connected.");
    };
    let Some(extension_name) = skill
        .properties
        .get("mcpExtension")
        .and_then(|value| value.as_str())
    else {
        return skill_error(format!("Skill '{}' has no serving extension.", skill.name));
    };
    if relative_path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return skill_error(format!("File '{relative_path}' is not a skill file."));
    }
    let got = match lease.send_skill_get(extension_name, &skill.path).await {
        Ok(record) => record,
        Err(error) => return skill_error(error),
    };
    let Some((skill_path, _)) = crate::skills::mcp::parse_skill_uri(&skill.path) else {
        return skill_error(format!("Skill '{}' has no skill URI.", skill.name));
    };
    let uri = format!("skill://{skill_path}/{relative_path}");
    if let Err(error) = crate::skills::mcp::listed_file(&got, &uri) {
        return skill_error(error);
    }
    let bytes = match lease.read_skill_text(extension_name, &uri).await {
        Ok(text) => text,
        Err(error) => return skill_error(error),
    };
    if let Err(error) = crate::skills::mcp::verify_file(&got, &uri, bytes.as_bytes()) {
        return skill_error(error);
    }
    CallToolResult::success(vec![ContentBlock::text(bytes)])
}

fn load_supporting_file(
    skill: &SourceEntry,
    skill_name: &str,
    relative_path: &str,
) -> CallToolResult {
    let skill_dir = PathBuf::from(&skill.path);
    for file_path in &skill.supporting_files {
        let file_path = Path::new(file_path);
        let Ok(relative) = file_path.strip_prefix(&skill_dir) else {
            continue;
        };
        if relative.to_string_lossy().replace('\\', "/") != relative_path {
            continue;
        }
        return match crate::skills::load_supporting_file(&skill_dir, relative, skill_name) {
            Ok(content) => CallToolResult::success(vec![ContentBlock::text(content)]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to read '{skill_name}': {error}"
            ))]),
        };
    }

    let available: Vec<String> = skill
        .supporting_files
        .iter()
        .filter_map(|file| {
            Path::new(file)
                .strip_prefix(&skill_dir)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        })
        .take(10)
        .collect();
    if available.is_empty() {
        CallToolResult::error(vec![ContentBlock::text(format!(
            "Skill '{}' has no supporting files.",
            skill.name
        ))])
    } else {
        CallToolResult::error(vec![ContentBlock::text(format!(
            "File '{skill_name}' not found. Available: {}",
            available.join(", ")
        ))])
    }
}

impl SkillOperation {
    pub fn new(
        hook_manager: HookManager,
        extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    ) -> Self {
        Self {
            hook_manager,
            extension_lease,
        }
    }

    fn leased_session(&self, session: &Session) -> Option<Session> {
        let lease = self
            .extension_lease
            .lock()
            .expect("extension lease unavailable")
            .clone()?;
        if lease.scope_id() != session.id.as_str() {
            return None;
        }
        let mut session = session.clone();
        session.working_dir = lease.working_dir()?.to_path_buf();
        Some(session)
    }

    async fn command_response(
        conversation: &Conversation,
        message: String,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let command = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("skill command conversation has no kickoff message"))?;
        let message_id = command
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        let command = command.with_visibility(true, false);
        let response = Message::assistant()
            .with_text(message)
            .with_visibility(true, false);
        emit.message(command).await;
        let response = emit.message(response).await;
        yielded_with([
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            response.into(),
        ])
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for SkillOperation {
    fn name(&self) -> &'static str {
        "skills"
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        if command.command == "skills" {
            return Self::command_response(
                conversation,
                crate::slash_commands::skill_slash_command::format_installed_skills(Some(
                    &session.working_dir,
                )),
                emit,
            )
            .await;
        }

        let prompt = match crate::slash_commands::skill_slash_command::resolve_command(
            command.command,
            command.params_str,
            Some(&session.working_dir),
        ) {
            Ok(Some(prompt)) => prompt,
            Ok(None) => return not_applicable(),
            Err(error) => return Self::command_response(conversation, error, emit).await,
        };
        let command_message = messages_since_kickoff(conversation)?
            .first()
            .ok_or_else(|| anyhow!("skill command conversation has no kickoff message"))?;
        let message_id = command_message
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        applied([
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            Message::user()
                .with_text(prompt)
                .with_visibility(false, true)
                .into(),
        ])
    }

    async fn inference_tools(&self, _session: &Session) -> Result<Vec<Tool>> {
        Ok(vec![skill_tool()?])
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let mcp = match current_lease(&self.extension_lease, session) {
            Some(lease) => lease.list_mcp_skills().await,
            None => Vec::new(),
        };
        let skills = skill_entries(&session.working_dir, mcp);
        let skills: Vec<&SourceEntry> = skills.iter().collect();
        Ok(render_skill_instructions(&skills)
            .map(|instructions| ("skills".to_string(), instructions))
            .into_iter()
            .collect())
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let pending: Vec<_> = pending_advertised_tool_requests(messages)
            .into_iter()
            .filter(|(request, _)| {
                request
                    .tool_call
                    .as_ref()
                    .is_ok_and(|tool_call| tool_call.name.as_ref() == LOAD_SKILL_TOOL_NAME)
            })
            .collect();
        if pending.is_empty() {
            return not_applicable();
        }

        let leased_session = self.leased_session(session);
        let mut response = Message::user();
        for (request, disposition) in pending {
            let result: std::result::Result<CallToolResult, ErrorData> = match disposition {
                ToolDisposition::Execute if session.goose_mode == GooseMode::Chat => {
                    // Nothing executes in chat mode, so no tool lifecycle runs.
                    Ok(CallToolResult::success(vec![ContentBlock::text(
                        CHAT_MODE_TOOL_SKIPPED_RESPONSE,
                    )]))
                }
                ToolDisposition::Execute => {
                    let Some(session) = leased_session.as_ref() else {
                        response.add_tool_response_with_metadata(
                            request.id,
                            Ok(CallToolResult::error(vec![ContentBlock::text(
                                EXPIRED_APPROVAL_RESPONSE,
                            )])),
                            request.metadata.as_ref(),
                        );
                        continue;
                    };
                    let tool_call = request.tool_call.as_ref().map_err(|error| {
                        anyhow!("load_skill tool call could not be parsed: {error}")
                    })?;
                    let span = tool_span(&tool_call.name, &request.id, &session.id);
                    // `load_skill` is executed here rather than by
                    // ToolExecutionOperation, which is registered after this one.
                    // Run the same hook lifecycle it would have run, so the state
                    // machine and the legacy loop agree on what a skill load emits.
                    let tool_input = tool_call
                        .arguments
                        .as_ref()
                        .map(|arguments| Value::Object(arguments.clone()));
                    match run_pre_tool_hooks(
                        &self.hook_manager,
                        session,
                        &request.id,
                        &tool_call.name,
                        tool_input.as_ref(),
                    )
                    .instrument(span.clone())
                    .await
                    {
                        // A denial returns before execution and emits no post
                        // event, the same shape ToolExecutionOperation has: its
                        // dispatch returns the denial before the post-hook wrapper
                        // is ever applied.
                        Err(denial) => Err(denial),
                        Ok(()) => {
                            let lease = current_lease(&self.extension_lease, session);
                            let result = execute_skill(
                                &session.working_dir,
                                tool_call.arguments.clone(),
                                lease.as_deref(),
                            )
                            .instrument(span.clone())
                            .await;
                            if result.is_error == Some(true) {
                                span.record("error.type", "tool_error");
                            }
                            let output = Ok(result);
                            // Post event carries the same tool_call_id as the pre
                            // events. The large-response rewrite
                            // ToolExecutionOperation applies is deliberately not
                            // reused: a skill body is content the model is meant to
                            // read, not a payload to offload to a temp file.
                            emit_post_tool_use(
                                &self.hook_manager,
                                &session.id,
                                &session.working_dir.to_string_lossy(),
                                &tool_call.name,
                                &request.id,
                                tool_input.as_ref(),
                                &output,
                            )
                            .instrument(span.clone())
                            .await;
                            output
                        }
                    }
                }
                ToolDisposition::Decline => Ok(CallToolResult::error(vec![ContentBlock::text(
                    DECLINED_RESPONSE,
                )])),
                ToolDisposition::ParseError(error) => {
                    Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                        "The tool call could not be parsed: {error}. Correct the arguments and try again."
                    ))]))
                }
            };
            response.add_tool_response_with_metadata(request.id, result, request.metadata.as_ref());
        }
        let response = emit.message(response).await;
        applied([response.into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn supporting_file_loader_reads_nested_regular_file() {
        let root = tempfile::tempdir().unwrap();
        let skill_dir = std::fs::canonicalize(root.path()).unwrap();
        let nested = skill_dir.join("nested");
        std::fs::create_dir(&nested).unwrap();
        let file = nested.join("guide.md");
        std::fs::write(&file, "Nested guidance.").unwrap();
        let skill = SourceEntry {
            source_type: SourceType::Skill,
            name: "test-skill".to_string(),
            description: String::new(),
            content: String::new(),
            path: skill_dir.to_string_lossy().into_owned(),
            global: false,
            writable: true,
            supporting_files: vec![file.to_string_lossy().into_owned()],
            properties: HashMap::new(),
        };

        let result = load_supporting_file(&skill, "test-skill/nested/guide.md", "nested/guide.md");

        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().expect("expected text");
        assert!(text.text.contains("Nested guidance."));
    }
}
