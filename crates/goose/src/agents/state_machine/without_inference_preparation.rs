use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use super::{Emitter, GooseEffect, Operation, OperationResult, SlashCommand};
use crate::conversation::Conversation;
use crate::session::Session;

/// Keeps application commands and lifecycle behavior without preparing completion inputs for ACP.
pub struct WithoutInferencePreparation<'a>(pub Arc<dyn Operation<Session, GooseEffect> + 'a>);

#[async_trait]
impl Operation<Session, GooseEffect> for WithoutInferencePreparation<'_> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        self.0.run(session, conversation, emit).await
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        self.0
            .run_command(command, session, conversation, emit)
            .await
    }

    async fn cancel(
        &self,
        session: &Session,
        conversation: &Conversation,
        result: OperationResult<GooseEffect>,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        self.0.cancel(session, conversation, result, emit).await
    }
}
