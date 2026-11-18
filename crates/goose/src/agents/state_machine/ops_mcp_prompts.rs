//! Slash commands for the prompts MCP extensions offer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rmcp::model::Role;

use crate::agents::state_machine::{
    applied, messages_since_kickoff, yielded_with, Command, ConversationEffect, Emitter,
    GooseEffect, Operation, OperationResult,
};
use crate::agents::tool_snapshot::ToolSnapshot;
use crate::agents::StateMachineServices;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::session::Session;

pub struct McpPromptOperation {
    services: Arc<StateMachineServices>,
    tools: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
}

impl McpPromptOperation {
    pub fn new(
        services: Arc<StateMachineServices>,
        tools: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
    ) -> Self {
        Self { services, tools }
    }

    fn command_response(
        conversation: &Conversation,
        message: String,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let command = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("prompt command conversation has no kickoff message"))?;
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

    async fn list_prompts(
        &self,
        arguments: &str,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let prompts = ToolSnapshot::for_session(&self.tools, &self.services, &session.id)
            .await?
            .mcp
            .list_prompts(emit.cancel_token().clone())
            .await;
        let extension_filter = arguments.split_whitespace().next();
        if let Some(filter) = extension_filter {
            if !prompts.contains_key(filter) {
                return Self::command_response(
                    conversation,
                    format!("Extension '{filter}' not found"),
                    emit,
                );
            }
        }

        let filtered: HashMap<_, _> = prompts
            .into_iter()
            .filter(|(extension, _)| extension_filter.is_none_or(|filter| extension == filter))
            .collect();
        let mut output = String::new();
        if filtered.is_empty() {
            output.push_str("No prompts available.\n");
        } else {
            output.push_str("Available prompts:\n\n");
            for (extension, prompts) in filtered {
                output.push_str(&format!("**{extension}**:\n"));
                for prompt in prompts {
                    output.push_str(&format!("  - {}\n", prompt.name));
                }
                output.push('\n');
            }
        }
        Self::command_response(conversation, output, emit)
    }

    async fn run_prompt(
        &self,
        arguments: &str,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let params: Vec<_> = arguments.split_whitespace().collect();
        let Some(prompt_name) = params.first() else {
            return Self::command_response(
                conversation,
                "Prompt name argument is required".to_string(),
                emit,
            );
        };
        let prompts = ToolSnapshot::for_session(&self.tools, &self.services, &session.id)
            .await?
            .mcp
            .list_prompts(emit.cancel_token().clone())
            .await;
        let found = prompts.iter().find_map(|(extension, prompts)| {
            prompts
                .iter()
                .find(|prompt| prompt.name == *prompt_name)
                .map(|prompt| (extension.clone(), prompt.clone()))
        });
        let Some((extension, prompt)) = found else {
            return Self::command_response(
                conversation,
                format!("Prompt '{prompt_name}' not found"),
                emit,
            );
        };

        if params.get(1) == Some(&"--info") {
            let mut output = format!("**Prompt: {}**\n\n", prompt.name);
            if let Some(description) = &prompt.description {
                output.push_str(&format!("Description: {description}\n\n"));
            }
            output.push_str(&format!("Extension: {extension}\n\n"));
            if let Some(arguments) = &prompt.arguments {
                output.push_str("Arguments:\n");
                for argument in arguments {
                    output.push_str(&format!("  - {}", argument.name));
                    if let Some(description) = &argument.description {
                        output.push_str(&format!(": {description}"));
                    }
                    output.push('\n');
                }
            }
            return Self::command_response(conversation, output, emit);
        }

        let arguments: HashMap<_, _> = params
            .iter()
            .skip(1)
            .filter_map(|param| param.split_once('='))
            .map(|(key, value)| (key.to_string(), value.trim_matches('"').to_string()))
            .collect();
        let result = match ToolSnapshot::for_session(&self.tools, &self.services, &session.id)
            .await?
            .mcp
            .get_prompt(
                &extension,
                prompt_name,
                serde_json::to_value(arguments)?,
                emit.cancel_token().clone(),
            )
            .await
        {
            Ok(result) => result,
            Err(error) => {
                return Self::command_response(conversation, error.to_string(), emit);
            }
        };

        let command_message = messages_since_kickoff(conversation)?
            .first()
            .ok_or_else(|| anyhow!("prompt command conversation has no kickoff message"))?;
        let message_id = command_message
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        let mut effects = vec![ConversationEffect::SetMessageVisibility {
            message_id,
            user_visible: true,
            agent_visible: false,
        }
        .into()];
        for (index, prompt_message) in result.messages.into_iter().enumerate() {
            let message = Message::from(prompt_message);
            let expected_role = if index % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            if message.role != expected_role {
                return Self::command_response(
                    conversation,
                    format!(
                        "Expected {expected_role:?} message at position {index}, but found {:?}",
                        message.role
                    ),
                    emit,
                );
            }
            effects.push(message.with_visibility(false, true).into());
        }
        if effects.len() == 1 {
            return Self::command_response(
                conversation,
                format!("Prompt '{prompt_name}' returned no messages"),
                emit,
            );
        }
        applied(effects)
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for McpPromptOperation {
    fn name(&self) -> &'static str {
        "mcp_prompts"
    }

    async fn commands(&self, _session: &Session) -> Result<Vec<Command<'_, Session, GooseEffect>>> {
        Ok(vec![
            Command::new(
                "prompts",
                "List available prompts, optionally filtered by extension",
                move |call| {
                    Box::pin(self.list_prompts(
                        call.arguments,
                        call.session,
                        call.conversation,
                        call.emit,
                    ))
                },
            ),
            Command::new(
                "prompt",
                "Execute a prompt or show its info with --info",
                move |call| {
                    Box::pin(self.run_prompt(
                        call.arguments,
                        call.session,
                        call.conversation,
                        call.emit,
                    ))
                },
            ),
        ])
    }
}
