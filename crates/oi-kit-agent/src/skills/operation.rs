//! Offers the host's skills to the model through `load_skill` and as slash commands.

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_acp_types::custom_requests::{SourceEntry, SourceType};
use oi_kit_provider_types::conversation::message::{Message, ToolRequest};
use oi_kit_provider_types::conversation::Conversation;
use oi_kit_provider_types::maybe_send::{MaybeSend, MaybeSync};
use rmcp::handler::server::common::schema_for_type;
use rmcp::model::{CallToolResult, ContentBlock, JsonObject, Tool};
use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use super::{loaded_skill_context, skill_content_with_args};
use crate::command::Command;
use crate::operation::{
    applied, messages_since_kickoff, not_applicable, yielded_with, ConversationEffect, Emitter,
    Operation, OperationResult,
};
use crate::tool_call_state::{tool_calls, ToolCallState};

pub const LOAD_SKILL_TOOL_NAME: &str = "load_skill";

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait SkillSource<S>: MaybeSend + MaybeSync {
    async fn skills(&self, session: &S) -> Result<Vec<SourceEntry>>;

    /// `relative_path` is one of `skill.supporting_files`, relative to `skill.path`.
    async fn read_supporting_file(
        &self,
        skill: &SourceEntry,
        relative_path: &Path,
    ) -> Result<String>;
}

pub struct SkillOperation<'a, S> {
    source: Arc<dyn SkillSource<S> + 'a>,
    no_skills_help: String,
}

impl<'a, S> SkillOperation<'a, S> {
    pub fn new(source: Arc<dyn SkillSource<S> + 'a>) -> Self {
        Self {
            source,
            no_skills_help: "No skills installed.\n".to_string(),
        }
    }

    pub fn with_no_skills_help(mut self, help: impl Into<String>) -> Self {
        self.no_skills_help = help.into();
        self
    }

    async fn sorted_skills(&self, session: &S) -> Result<Vec<SourceEntry>> {
        let mut skills = self.source.skills(session).await?;
        skills.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
        Ok(skills)
    }

    fn installed_skills(&self, skills: &[SourceEntry]) -> String {
        if skills.is_empty() {
            return self.no_skills_help.clone();
        }
        let mut output = format!("**Installed skills ({}):**\n\n", skills.len());
        for skill in skills {
            let builtin = if skill.source_type == SourceType::BuiltinSkill {
                " *(builtin)*"
            } else {
                ""
            };
            output.push_str(&format!(
                "- **{}**{}: {}\n",
                skill.name, builtin, skill.description
            ));
        }
        output
    }

    async fn load_skill(&self, session: &S, arguments: Option<JsonObject>) -> Result<String> {
        let params: LoadSkillParams =
            serde_json::from_value(serde_json::Value::Object(arguments.unwrap_or_default()))
                .map_err(|error| anyhow!("Invalid arguments: {error}"))?;
        let name = params.name.as_str();
        let skills = self.source.skills(session).await?;

        if let Some(skill) = skills.iter().find(|skill| skill.name == name) {
            let content = skill_content_with_args(skill, params.args.as_deref())
                .map_err(|error| anyhow!("Failed to parse skill arguments: {error}"))?;
            return Ok(loaded_skill_context(skill, &content));
        }

        if let Some((skill_name, relative_path)) = name.split_once('/') {
            if let Some(skill) = skills.iter().find(|skill| skill.name == skill_name) {
                return self
                    .load_supporting_file(skill, name, &relative_path.replace('\\', "/"))
                    .await;
            }
        }

        let lowercase = name.to_lowercase();
        let suggestions: Vec<&str> = skills
            .iter()
            .filter(|skill| {
                let skill_name = skill.name.to_lowercase();
                skill_name.contains(&lowercase) || lowercase.contains(&skill_name)
            })
            .take(3)
            .map(|skill| skill.name.as_str())
            .collect();
        if suggestions.is_empty() {
            Err(anyhow!("Skill '{name}' not found."))
        } else {
            Err(anyhow!(
                "Skill '{name}' not found. Did you mean: {}?",
                suggestions.join(", ")
            ))
        }
    }

    async fn load_supporting_file(
        &self,
        skill: &SourceEntry,
        name: &str,
        relative_path: &str,
    ) -> Result<String> {
        let skill_dir = Path::new(&skill.path);
        let available: Vec<_> = skill
            .supporting_files
            .iter()
            .filter_map(|file| Path::new(file).strip_prefix(skill_dir).ok())
            .collect();
        if let Some(relative) = available
            .iter()
            .find(|relative| relative.to_string_lossy().replace('\\', "/") == relative_path)
        {
            return self
                .source
                .read_supporting_file(skill, relative)
                .await
                .map_err(|error| anyhow!("Failed to read '{name}': {error}"));
        }
        if available.is_empty() {
            return Err(anyhow!("Skill '{}' has no supporting files.", skill.name));
        }
        let available: Vec<_> = available
            .iter()
            .take(10)
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .collect();
        Err(anyhow!(
            "File '{name}' not found. Available: {}",
            available.join(", ")
        ))
    }
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct LoadSkillParams {
    /// Name of the skill to load. Use "skill-name/path" to load a supporting file.
    name: String,
    /// Optional arguments to provide when loading the skill.
    #[serde(default)]
    args: Option<String>,
}

fn load_skill_tool() -> Tool {
    Tool::new(
        LOAD_SKILL_TOOL_NAME,
        "Load a skill's full content into your context so you can follow its instructions.\n\n\
         Skills are listed in your system instructions. When you need to use one, \
         load it first to get the detailed instructions.",
        schema_for_type::<LoadSkillParams>(),
    )
}

fn pending_load_skill_calls(messages: &[Message]) -> Vec<&ToolRequest> {
    tool_calls(messages)
        .into_iter()
        .filter(|(request, state)| {
            *state == ToolCallState::Executable
                && !request.was_executed_externally()
                && request
                    .tool_call
                    .as_ref()
                    .is_ok_and(|call| call.name == LOAD_SKILL_TOOL_NAME)
        })
        .map(|(request, _)| request)
        .collect()
}

fn kickoff(conversation: &Conversation) -> Result<&Message> {
    messages_since_kickoff(conversation)?
        .first()
        .filter(|message| message.id.is_some())
        .ok_or_else(|| anyhow!("slash command message has no id"))
}

fn kickoff_message_id(conversation: &Conversation) -> Result<String> {
    Ok(kickoff(conversation)?
        .id
        .clone()
        .expect("kickoff has an id"))
}

fn hide_kickoff_from_agent(message_id: String) -> ConversationEffect {
    ConversationEffect::SetMessageVisibility {
        message_id,
        user_visible: true,
        agent_visible: false,
    }
}

fn command_response<E>(
    conversation: &Conversation,
    text: String,
    emit: &Emitter,
) -> Result<OperationResult<E>>
where
    E: From<ConversationEffect>,
{
    let message_id = kickoff_message_id(conversation)?;
    emit.message(kickoff(conversation)?.clone().with_visibility(true, false));
    let response = emit.message(
        Message::assistant()
            .with_text(text)
            .with_visibility(true, false),
    );
    yielded_with([
        E::from(hide_kickoff_from_agent(message_id)),
        E::from(ConversationEffect::from(response)),
    ])
}

fn run_skill_command<E>(
    skill: &SourceEntry,
    arguments: &str,
    conversation: &Conversation,
    emit: &Emitter,
) -> Result<OperationResult<E>>
where
    E: From<ConversationEffect>,
{
    let arguments = (!arguments.is_empty()).then_some(arguments);
    let content = match skill_content_with_args(skill, arguments) {
        Ok(content) => content,
        Err(error) => {
            return command_response(
                conversation,
                format!("Skill /{}: {error}", skill.name),
                emit,
            )
        }
    };
    let prompt = Message::user()
        .with_text(loaded_skill_context(skill, &content))
        .with_visibility(false, true);
    applied([
        E::from(hide_kickoff_from_agent(kickoff_message_id(conversation)?)),
        E::from(ConversationEffect::from(prompt)),
    ])
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for SkillOperation<'_, S>
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<ConversationEffect> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "skills"
    }

    async fn commands(&self, session: &S) -> Result<Vec<Command<'_, S, E>>> {
        let skills = self.sorted_skills(session).await?;
        let mut commands = vec![Command::new(
            "skills",
            "List installed skills",
            move |call| {
                Box::pin(async move {
                    let skills = self.sorted_skills(call.session).await?;
                    command_response(call.conversation, self.installed_skills(&skills), call.emit)
                })
            },
        )];
        for skill in skills {
            let name = skill.name.to_lowercase();
            let description = skill.description.clone();
            let input_hint = skill
                .properties
                .get("argument-hint")
                .and_then(|value| value.as_str())
                .filter(|hint| !hint.is_empty())
                .map(str::to_string);
            let mut command = Command::new(name, description, move |call| {
                let result =
                    run_skill_command(&skill, call.arguments, call.conversation, call.emit);
                Box::pin(async move { result })
            });
            command.input_hint = input_hint;
            commands.push(command);
        }
        Ok(commands)
    }

    async fn inference_tools(&self, _session: &S) -> Result<Vec<Tool>> {
        Ok(vec![load_skill_tool()])
    }

    async fn prompt_parts(
        &self,
        session: &S,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let skills = self.sorted_skills(session).await?;
        if skills.is_empty() {
            return Ok(Vec::new());
        }
        let mut instructions = String::from(
            "# Skills\n\nYou have these skills at your disposal. Load one when it can help with the task or when the user asks for it:",
        );
        for skill in skills {
            instructions.push_str(&format!("\n- {}: {}", skill.name, skill.description));
        }
        Ok(vec![("skills".to_string(), instructions)])
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let pending = pending_load_skill_calls(messages_since_kickoff(conversation)?);
        if pending.is_empty() {
            return not_applicable();
        }

        let mut response = Message::user();
        for request in pending {
            let arguments = request
                .tool_call
                .as_ref()
                .ok()
                .and_then(|call| call.arguments.clone());
            let result = match self.load_skill(session, arguments).await {
                Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
                Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
            };
            response.add_tool_response_with_metadata(
                request.id.clone(),
                Ok(result),
                request.metadata.as_ref(),
            );
        }
        applied([E::from(ConversationEffect::from(emit.message(response)))])
    }
}
