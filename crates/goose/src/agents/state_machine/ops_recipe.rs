//! Applies recipe commands and recipe instructions.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;

use crate::agents::extension_manager::ExtensionManager;
use crate::agents::final_output_tool::FinalOutputTool;
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, yielded_with, ConversationEffect, Emitter,
    GooseEffect, Operation, OperationResult, SlashCommand,
};
use crate::config::extensions::is_extension_available;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::session::Session;

pub(super) fn command_error(
    conversation: &Conversation,
    message: String,
    emit: &Emitter,
) -> Result<OperationResult<GooseEffect>> {
    let command = messages_since_kickoff(conversation)?
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("recipe command conversation has no kickoff message"))?;
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

pub struct RecipeOperation {
    extension_manager: Arc<ExtensionManager>,
}

impl RecipeOperation {
    pub fn new(extension_manager: Arc<ExtensionManager>) -> Self {
        Self { extension_manager }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for RecipeOperation {
    fn name(&self) -> &'static str {
        "recipe"
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let (recipe, prompt) = match crate::slash_commands::recipe_slash_command::resolve_command(
            command.command,
            command.params_str,
        ) {
            Ok(Some(recipe)) => recipe,
            Ok(None) => return not_applicable(),
            Err(error) => return command_error(conversation, error, emit),
        };

        if let Some(response) = recipe.response.clone() {
            if let Err(error) = FinalOutputTool::try_new(response) {
                return command_error(conversation, format!("Recipe is not valid: {error}"), emit);
            }
        }

        for extension in recipe
            .extensions
            .iter()
            .flatten()
            .filter(|extension| is_extension_available(extension))
        {
            if let Err(error) = self
                .extension_manager
                .enable(&session.id, extension.clone())
                .await
            {
                return command_error(
                    conversation,
                    format!(
                        "Could not load recipe extension '{}': {error}",
                        extension.name()
                    ),
                    emit,
                );
            }
        }

        #[cfg(feature = "telemetry")]
        crate::posthog::emit_custom_slash_command_used();

        let command_message = messages_since_kickoff(conversation)?
            .first()
            .ok_or_else(|| anyhow!("recipe command conversation has no kickoff message"))?;
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
            GooseEffect::SetRecipe(Box::new(Some(recipe))),
            Message::user()
                .with_text(prompt)
                .with_visibility(false, true)
                .into(),
        ])
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(session
            .recipe
            .as_ref()
            .and_then(|recipe| recipe.instructions.clone())
            .map(|instructions| ("recipe".to_string(), instructions))
            .into_iter()
            .collect())
    }
}
