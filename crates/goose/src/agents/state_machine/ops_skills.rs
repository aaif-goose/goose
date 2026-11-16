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

use crate::agents::state_machine::ops_toolcalling::{
    pending_advertised_tool_requests, tool_span, ToolDisposition, EXPIRED_APPROVAL_RESPONSE,
};
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, yielded_with, Command, ConversationEffect,
    Emitter, GooseEffect, Operation, OperationResult,
};
use crate::agents::tool_snapshot::ToolSnapshot;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::session::Session;
use crate::slash_commands::skill_slash_command::{format_installed_skills, runnable_skills};

pub(super) const LOAD_SKILL_TOOL_NAME: &str = "load_skill";

pub struct SkillOperation {
    tool_snapshot: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
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

fn skill_instructions(working_dir: &Path) -> Option<String> {
    let sources = crate::skills::discover_skills(Some(working_dir));
    let mut skills: Vec<&SourceEntry> = sources
        .iter()
        .filter(|source| {
            matches!(
                source.source_type,
                SourceType::Skill | SourceType::BuiltinSkill
            )
        })
        .collect();
    skills.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
    if skills.is_empty() {
        return None;
    }

    let mut instructions = String::from(
        "# Skills\n\nYou have these skills at your disposal. Load one when it can help with the task or when the user asks for it:",
    );
    for skill in skills {
        instructions.push_str(&format!("\n- {}: {}", skill.name, skill.description));
    }
    Some(instructions)
}

fn execute_skill(working_dir: &Path, arguments: Option<JsonObject>) -> CallToolResult {
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
    let skill_name = params.name.as_str();
    let args = params.args.as_deref();
    let skills = crate::skills::discover_skills(Some(working_dir));

    if let Some(skill) = skills.iter().find(|skill| skill.name == skill_name) {
        return match crate::skills::loaded_skill_context_with_args(skill, args) {
            Ok(rendered) => CallToolResult::success(vec![ContentBlock::text(rendered)]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to parse skill arguments: {error}"
            ))]),
        };
    }

    if let Some((parent_skill_name, raw_relative_path)) = skill_name.split_once('/') {
        let relative_path = raw_relative_path.replace('\\', "/");
        if let Some(skill) = skills.iter().find(|skill| {
            skill.name == parent_skill_name
                && matches!(
                    skill.source_type,
                    SourceType::Skill | SourceType::BuiltinSkill
                )
        }) {
            return load_supporting_file(skill, skill_name, &relative_path);
        }
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

fn load_supporting_file(
    skill: &SourceEntry,
    skill_name: &str,
    relative_path: &str,
) -> CallToolResult {
    let listed_skill_dir = PathBuf::from(&skill.path);
    let load_skill_dir = match listed_skill_dir.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            return CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to resolve '{}': {}",
                skill.name, error
            ))]);
        }
    };
    for file_path in &skill.supporting_files {
        let file_path = Path::new(file_path);
        let Ok(relative) = file_path.strip_prefix(&listed_skill_dir) else {
            continue;
        };
        if relative.to_string_lossy().replace('\\', "/") != relative_path {
            continue;
        }
        return match crate::skills::load_supporting_file(&load_skill_dir, relative, skill_name) {
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
                .strip_prefix(&listed_skill_dir)
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
    pub fn new(tool_snapshot: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>) -> Self {
        Self { tool_snapshot }
    }

    fn leased_session(&self, session: &Session) -> Option<Session> {
        let lease = self
            .tool_snapshot
            .lock()
            .expect("tool snapshot unavailable")
            .clone()?;
        if lease.scope_id() != session.id.as_str() {
            return None;
        }
        let mut session = session.clone();
        session.working_dir = lease.working_dir()?.to_path_buf();
        Some(session)
    }

    fn command_response(
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
        emit.message(command);
        let response = emit.message(response);
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

    fn run_skill(
        skill: &SourceEntry,
        arguments: &str,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let arguments = (!arguments.is_empty()).then_some(arguments);
        let prompt = match crate::skills::loaded_skill_context_with_args(skill, arguments) {
            Ok(prompt) => prompt,
            Err(error) => {
                return Self::command_response(
                    conversation,
                    format!("Skill /{}: {error}", skill.name),
                    emit,
                )
            }
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
}

#[async_trait]
impl Operation<Session, GooseEffect> for SkillOperation {
    fn name(&self) -> &'static str {
        "skills"
    }

    async fn commands(&self, session: &Session) -> Result<Vec<Command<'_, Session, GooseEffect>>> {
        let mut commands = vec![Command::<Session, GooseEffect>::new(
            "skills",
            "List installed skills and other available sources",
            |call| {
                Box::pin(async move {
                    Self::command_response(
                        call.conversation,
                        format_installed_skills(Some(&call.session.working_dir)),
                        call.emit,
                    )
                })
            },
        )];
        for (name, skill) in runnable_skills(Some(&session.working_dir)) {
            let description = skill.description.clone();
            let input_hint = crate::skills::skill_argument_hint(&skill);
            let mut command = Command::new(name, description, move |call| {
                let result = Self::run_skill(&skill, call.arguments, call.conversation, call.emit);
                Box::pin(async move { result })
            });
            command.input_hint = input_hint;
            commands.push(command);
        }
        Ok(commands)
    }

    async fn inference_tools(&self, _session: &Session) -> Result<Vec<Tool>> {
        Ok(vec![skill_tool()?])
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(skill_instructions(&session.working_dir)
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
                    let result = {
                        let _entered = span.enter();
                        execute_skill(&session.working_dir, tool_call.arguments.clone())
                    };
                    if result.is_error == Some(true) {
                        span.record("error.type", "tool_error");
                    }
                    Ok(result)
                }
                ToolDisposition::ParseError(error) => {
                    Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                        "The tool call could not be parsed: {error}. Correct the arguments and try again."
                    ))]))
                }
            };
            response.add_tool_response_with_metadata(request.id, result, request.metadata.as_ref());
        }
        let response = emit.message(response);
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

    #[cfg(unix)]
    #[test]
    fn supporting_file_loader_reads_file_under_symlinked_directory() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tempdir.path()).unwrap();

        let real_skills = root.join("real-skills");
        std::fs::create_dir(&real_skills).unwrap();
        let skill_dir = real_skills.join("test-skill");
        let references = skill_dir.join("references");
        std::fs::create_dir_all(&references).unwrap();
        let file = references.join("guide.md");
        std::fs::write(&file, "Symlinked guidance.").unwrap();

        let linked_skills = root.join("linked-skills");
        std::os::unix::fs::symlink(&real_skills, &linked_skills).unwrap();
        let linked_skill_dir = linked_skills.join("test-skill");

        let skill = SourceEntry {
            source_type: SourceType::Skill,
            name: "test-skill".to_string(),
            description: String::new(),
            content: String::new(),
            path: linked_skill_dir.to_string_lossy().into_owned(),
            global: false,
            writable: true,
            supporting_files: vec![linked_skill_dir
                .join("references")
                .join("guide.md")
                .to_string_lossy()
                .into_owned()],
            properties: HashMap::new(),
        };

        let result = load_supporting_file(
            &skill,
            "test-skill/references/guide.md",
            "references/guide.md",
        );

        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().expect("expected text");
        assert!(text.text.contains("Symlinked guidance."));
    }
}
