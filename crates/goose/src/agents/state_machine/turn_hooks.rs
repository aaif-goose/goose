use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use rmcp::model::{CallToolResult, ContentBlock};

use crate::agents::extension_manager::ExtensionLease;
use crate::agents::session_hooks::is_session_start_message;
use crate::agents::state_machine::ops_toolcalling::{
    leased_working_dir, pending_advertised_tool_requests, tool_span, ToolDisposition,
};
use crate::agents::state_machine::stop_hook::run_stop_hooks;
use crate::agents::state_machine::tool_hooks::{check_pre_tool_hooks, emit_post_tool_hooks};
use crate::agents::state_machine::{
    messages_since_kickoff, ConversationEffect, EffectHandler, Emitter, GooseEffect, RunStatus,
    StateMachine, StepResult,
};
use crate::config::GooseMode;
use crate::conversation::message::{Message, MessageContent, ToolRequest};
use crate::hooks::{HookContext, HookEvent, HookManager};
use crate::session::Session;

const PRE_TOOL_HOOKS: &str = "pre_tool_hooks";

pub(crate) struct TurnHooks {
    hook_manager: HookManager,
    extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    stop_hook_block_cap: u32,
    allowed_tool_calls: HashMap<String, (ToolRequest, PathBuf)>,
}

impl TurnHooks {
    pub(crate) fn new(
        hook_manager: HookManager,
        extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
        stop_hook_block_cap: u32,
    ) -> Self {
        Self {
            hook_manager,
            extension_lease,
            stop_hook_block_cap,
            allowed_tool_calls: HashMap::new(),
        }
    }

    pub(crate) async fn start_turn(&self, session: &Session) {
        let Some(conversation) = &session.conversation else {
            return;
        };
        if messages_since_kickoff(conversation).map_or(true, |turn| turn.len() != 1) {
            return;
        }
        let Some((prompt, earlier)) = conversation.messages().split_last() else {
            return;
        };
        if !earlier.iter().any(|message| {
            is_session_start_message(message)
                || (is_user_prompt(message) && message.is_user_visible())
        }) {
            self.hook_manager
                .emit(
                    HookEvent::SessionStart,
                    HookContext::new(HookEvent::SessionStart, &session.id)
                        .with_working_dir(session.working_dir.to_string_lossy()),
                )
                .await;
        }
        self.emit_user_prompt_submit(session, prompt).await;
    }

    pub(crate) async fn step<R: EffectHandler<Session, GooseEffect>>(
        &mut self,
        machine: &StateMachine<'_, Session, GooseEffect>,
        runtime: &R,
        session: &Session,
        emit: &Emitter,
    ) -> Result<StepResult<GooseEffect>> {
        let mut result = match self.run_pre_tool_hooks(session).await? {
            Some(denials) => StepResult {
                effects: vec![emit.message(denials).into()],
                applied_step: Some(PRE_TOOL_HOOKS),
                status: RunStatus::Continuing,
            },
            None => {
                let result = machine.step(session, emit).await?;
                if result.applied_step.is_none() && result.status == RunStatus::Yielded {
                    run_stop_hooks(&self.hook_manager, self.stop_hook_block_cap, session, emit)
                        .await?
                } else {
                    result
                }
            }
        };
        machine
            .apply(runtime, session, &mut result.effects, emit)
            .await?;
        for message in result.effects.iter().filter_map(|effect| match effect {
            GooseEffect::Conversation(ConversationEffect::AppendMessage(message)) => Some(message),
            _ => None,
        }) {
            if message.metadata.steer {
                self.emit_user_prompt_submit(session, message).await;
            }
            self.run_post_tool_hooks(&session.id, message).await;
        }
        Ok(result)
    }

    async fn run_pre_tool_hooks(&mut self, session: &Session) -> Result<Option<Message>> {
        if session.goose_mode == GooseMode::Chat {
            return Ok(None);
        }
        let Some(conversation) = &session.conversation else {
            return Ok(None);
        };
        let requests: Vec<ToolRequest> =
            pending_advertised_tool_requests(messages_since_kickoff(conversation)?)
                .into_iter()
                .filter(|(request, disposition)| {
                    *disposition == ToolDisposition::Execute
                        && !self.allowed_tool_calls.contains_key(&request.id)
                })
                .map(|(request, _)| request)
                .collect();
        if requests.is_empty() {
            return Ok(None);
        }
        let working_dir = leased_working_dir(&self.extension_lease, session);
        let denials = futures::future::join_all(requests.iter().map(|request| {
            check_pre_tool_hooks(&self.hook_manager, &session.id, &working_dir, request)
        }))
        .await;

        let mut response = Message::user();
        for (request, denial) in requests.into_iter().zip(denials) {
            let Some(denial) = denial else {
                self.allowed_tool_calls
                    .insert(request.id.clone(), (request, working_dir.clone()));
                continue;
            };
            let tool_name = request
                .tool_call
                .as_ref()
                .map(|tool_call| tool_call.name.to_string())
                .unwrap_or_default();
            tool_span(&tool_name, &request.id, &session.id).record("error.type", denial.error_type);
            response.add_tool_response_with_metadata(
                request.id,
                Ok(CallToolResult::error(vec![ContentBlock::text(
                    denial.message,
                )])),
                request.metadata.as_ref(),
            );
        }
        Ok((!response.content.is_empty()).then_some(response))
    }

    async fn run_post_tool_hooks(&mut self, session_id: &str, message: &Message) {
        for content in &message.content {
            let MessageContent::ToolResponse(response) = content else {
                continue;
            };
            if let Some((request, working_dir)) = self.allowed_tool_calls.remove(&response.id) {
                emit_post_tool_hooks(
                    &self.hook_manager,
                    session_id,
                    &working_dir,
                    &request,
                    response,
                )
                .await;
            }
        }
    }

    async fn emit_user_prompt_submit(&self, session: &Session, prompt: &Message) {
        let prompt = prompt.agent_visible_content().as_concat_text();
        if prompt.is_empty() {
            return;
        }
        self.hook_manager
            .emit(
                HookEvent::UserPromptSubmit,
                HookContext::new(HookEvent::UserPromptSubmit, &session.id)
                    .with_message(prompt)
                    .with_working_dir(session.working_dir.to_string_lossy()),
            )
            .await;
    }
}

fn is_user_prompt(message: &Message) -> bool {
    message.role == rmcp::model::Role::User && !message.is_tool_response()
}
