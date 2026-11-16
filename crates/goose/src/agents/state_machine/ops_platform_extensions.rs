use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::Tool;

use super::ops_toolcalling::{
    extension_prompt_parts, pending_advertised_tool_requests, requests_can_resolve_tools,
    ToolDisposition, ToolExecution,
};
use super::{
    messages_since_kickoff, not_applicable, Emitter, GooseEffect, Operation, OperationResult,
};
use crate::agents::extension_manager::ExtensionManager;
use crate::agents::tool_dispatcher::ToolDispatcher;
use crate::conversation::Conversation;
use crate::session::Session;

pub(crate) struct PlatformExtensionOperation {
    manager: Arc<ExtensionManager>,
    tools: Arc<Mutex<Option<Arc<ToolDispatcher>>>>,
    execution: ToolExecution,
}

impl PlatformExtensionOperation {
    pub(crate) fn new(
        manager: Arc<ExtensionManager>,
        tools: Arc<Mutex<Option<Arc<ToolDispatcher>>>>,
    ) -> Self {
        Self {
            manager,
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
        Ok(
            ToolDispatcher::for_session(&self.tools, &self.manager, &session.id)
                .await?
                .platform_tools()
                .await
                .into_iter()
                .filter(|tool| {
                    !crate::agents::extension_manager::is_tool_owned_by_extension(
                        tool,
                        crate::skills::EXTENSION_NAME,
                    )
                })
                .collect(),
        )
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let tools = ToolDispatcher::for_session(&self.tools, &self.manager, &session.id).await?;
        #[cfg(feature = "code-mode")]
        if tools.is_enabled(crate::agents::platform_extensions::code_execution::EXTENSION_NAME) {
            return Ok(Vec::new());
        }
        let mut extensions = tools.platform_instructions().await;
        extensions.retain(|extension| extension.name != crate::skills::EXTENSION_NAME);
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
            ToolDispatcher::for_session(&self.tools, &self.manager, &session.id)
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
        let pending = pending_advertised_tool_requests(messages);
        if pending.is_empty() {
            return not_applicable();
        }
        let tools = self
            .tools
            .lock()
            .expect("tool dispatcher unavailable")
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
                ToolDispatcher::for_session(&self.tools, &self.manager, &session.id).await?
            }
            None => return not_applicable(),
        };
        let names: HashSet<_> = tools
            .platform_tools()
            .await
            .into_iter()
            .filter(|tool| {
                !crate::agents::extension_manager::is_tool_owned_by_extension(
                    tool,
                    crate::skills::EXTENSION_NAME,
                )
            })
            .map(|tool| tool.name.to_string())
            .collect();
        let pending: Vec<_> = pending
            .into_iter()
            .filter(|(request, _)| {
                request
                    .tool_call
                    .as_ref()
                    .is_ok_and(|call| names.contains(call.name.as_ref()))
            })
            .collect();
        if pending.is_empty() {
            return not_applicable();
        }
        let mut session = session.clone();
        if let Some(directory) = tools.working_dir() {
            session.working_dir = directory.to_path_buf();
        }
        self.execution
            .run(
                &self.manager,
                &tools,
                pending,
                HashMap::new(),
                &session,
                emit,
            )
            .await
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
