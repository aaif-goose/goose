use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::Tool;

use super::ops_toolcalling::{
    extension_prompt_parts, pending_tool_requests, requests_can_resolve_tools, ToolDisposition,
    ToolExecution,
};
use super::{
    messages_since_kickoff, not_applicable, Emitter, GooseEffect, Operation, OperationResult,
};
use crate::agents::tool_snapshot::ToolSnapshot;
use crate::agents::StateMachineServices;
use crate::conversation::Conversation;
use crate::session::Session;

pub(crate) struct PlatformExtensionOperation {
    services: Arc<StateMachineServices>,
    tools: Arc<Mutex<Option<Arc<ToolSnapshot>>>>,
    execution: ToolExecution,
}

impl PlatformExtensionOperation {
    pub(crate) fn new(
        services: Arc<StateMachineServices>,
        tools: Arc<Mutex<Option<Arc<ToolSnapshot>>>>,
    ) -> Self {
        Self {
            services,
            tools,
            execution: ToolExecution::new(),
        }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for PlatformExtensionOperation {
    fn name(&self) -> &'static str {
        "platform_extensions"
    }

    async fn inference_tools(&self, session: &Session) -> Result<Vec<Tool>> {
        let tools = ToolSnapshot::for_session(&self.tools, &self.services, &session.id).await?;
        Ok(tools.platform_tools().await)
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let tools = ToolSnapshot::for_session(&self.tools, &self.services, &session.id).await?;
        #[cfg(feature = "code-mode")]
        if tools.is_enabled(crate::agents::platform_extensions::code_execution::EXTENSION_NAME) {
            return Ok(Vec::new());
        }
        let mut extensions = tools.platform.instructions().await;
        extensions.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(if extensions.is_empty() {
            Vec::new()
        } else {
            extension_prompt_parts(&extensions, false)
        })
    }

    async fn moim_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<String>> {
        Ok(
            ToolSnapshot::for_session(&self.tools, &self.services, &session.id)
                .await?
                .moim()
                .await,
        )
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let pending = pending_tool_requests(messages);
        if pending.is_empty() {
            return not_applicable();
        }
        let tools = self
            .tools
            .lock()
            .expect("tool snapshot unavailable")
            .clone();
        let tools = match tools {
            Some(tools) => tools,
            None if requests_can_resolve_tools(
                messages,
                pending
                    .iter()
                    .filter(|(_, disposition)| *disposition == ToolDisposition::Execute)
                    .map(|(request, _)| request),
            ) =>
            {
                ToolSnapshot::for_session(&self.tools, &self.services, &session.id).await?
            }
            None => return not_applicable(),
        };
        let calls = tools
            .platform
            .prepare(
                &tools,
                pending.into_iter().map(|(request, _)| request).collect(),
                session,
                emit.cancel_token().clone(),
                &self.services,
            )
            .await;
        if calls.is_empty() {
            return not_applicable();
        }
        let mut session = session.clone();
        if let Some(directory) = tools.working_dir() {
            session.working_dir = directory.to_path_buf();
        }
        self.execution.run(calls, &session, emit).await
    }

    async fn finalize_cancellation(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<GooseEffect> {
        self.execution.finalize_cancellation(emit)
    }
}
