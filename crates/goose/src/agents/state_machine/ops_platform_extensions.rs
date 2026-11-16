use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::Tool;

use super::ops_toolcalling::ToolExecutionOperation;
use super::{Emitter, GooseEffect, Operation, OperationResult};
use crate::agents::extension_manager::{ExtensionLease, ExtensionManager};
use crate::conversation::Conversation;
use crate::hooks::HookManager;
use crate::session::Session;

pub(crate) struct PlatformExtensionOperation {
    execution: ToolExecutionOperation,
}

impl PlatformExtensionOperation {
    pub(crate) fn new(
        manager: Arc<ExtensionManager>,
        hooks: HookManager,
        lease: Arc<Mutex<Option<Arc<ExtensionLease>>>>,
    ) -> Self {
        Self {
            execution: ToolExecutionOperation::new(manager, hooks, lease, false, Vec::new())
                .for_platform(),
        }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for PlatformExtensionOperation {
    fn name(&self) -> &'static str {
        "platform_extensions"
    }

    async fn inference_tools(&self, session: &Session) -> Result<Vec<Tool>> {
        self.execution.inference_tools(session).await
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        self.execution.prompt_parts(session, conversation).await
    }

    async fn moim_parts(
        &self,
        session: &Session,
        conversation: &Conversation,
    ) -> Result<Vec<String>> {
        self.execution.moim_parts(session, conversation).await
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        self.execution.run(session, conversation, emit).await
    }

    async fn finalize_cancellation(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<GooseEffect> {
        self.execution
            .finalize_cancellation(session, conversation, emit)
            .await
    }
}
