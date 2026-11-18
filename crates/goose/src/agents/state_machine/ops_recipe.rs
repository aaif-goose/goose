//! Applies recipe commands and recipe instructions.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;

use crate::agents::state_machine::{
    applied, messages_since_kickoff, yielded_with, Command, ConversationEffect, Emitter,
    GooseEffect, Operation, OperationResult,
};
use crate::agents::StateMachineServices;
use crate::config::extensions::is_extension_available;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::session::Session;
use crate::slash_commands::recipe_slash_command::{build_recipe, recipe_entry, runnable_recipes};

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
    services: Arc<StateMachineServices>,
}

impl RecipeOperation {
    pub fn new(services: Arc<StateMachineServices>) -> Self {
        Self { services }
    }

    async fn run_recipe(
        &self,
        name: String,
        recipe_path: PathBuf,
        arguments: &str,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let (recipe, prompt) = match build_recipe(&name, &recipe_path, arguments) {
            Ok(recipe) => recipe,
            Err(error) => return command_error(conversation, error, emit),
        };

        if let Some(response) = &recipe.response {
            if let Err(error) = response.validate() {
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
                .services
                .enable_extension(&session.id, extension.clone())
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
}

#[async_trait]
impl Operation<Session, GooseEffect> for RecipeOperation {
    fn name(&self) -> &'static str {
        "recipe"
    }

    async fn commands(&self, _session: &Session) -> Result<Vec<Command<'_, Session, GooseEffect>>> {
        Ok(runnable_recipes()
            .into_iter()
            .map(|(name, recipe_path)| {
                let (description, input_hint) = match recipe_entry(&recipe_path) {
                    Some(metadata) => (metadata.description, metadata.input_hint),
                    None => (String::new(), None),
                };
                let mut command = Command::new(name.clone(), description, move |call| {
                    Box::pin(self.run_recipe(
                        name.clone(),
                        recipe_path.clone(),
                        call.arguments,
                        call.session,
                        call.conversation,
                        call.emit,
                    ))
                });
                command.input_hint = input_hint;
                command
            })
            .collect())
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
