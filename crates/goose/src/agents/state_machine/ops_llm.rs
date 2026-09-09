//! Builds a provider request and streams the next assistant response.

use std::sync::Arc;

use crate::agents::state_machine::ops_unknown_tool::UNCLAIMED_TOOL_ERROR;
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, trailing_error, yielded_with,
    ConversationEffect, Emitter, GooseEffect, Inference, InferenceInput, Operation,
    OperationResult, SlashCommand,
};
use crate::agents::{ExtensionManager, PromptManager};
use crate::config::GooseMode;
use crate::conversation::message::{InferenceMetadata, Message, MessageContent};
use crate::conversation::{effective_role, Conversation, EffectiveRole};
use crate::providers::base::{Provider, ProviderUsage};
use crate::session::Session;
use crate::tool_inspection::ToolInspectionManager;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::StreamExt;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use tokio::sync::Mutex;
use tracing_futures::Instrument;

const EMPTY_RESPONSE_MESSAGE: &str =
    "The model returned an empty response. Please resend your message to continue.";
const CANCELLED_TOOL_RESPONSE: &str = "Tool call was cancelled before execution";

fn is_thinking(content: &MessageContent) -> bool {
    matches!(
        content,
        MessageContent::Thinking(_) | MessageContent::RedactedThinking(_)
    )
}

fn normalize_tool_call_thinking(accumulator: &mut Conversation, chunk: &mut Message) {
    if !chunk
        .content
        .iter()
        .any(|content| matches!(content, MessageContent::ToolRequest(_)))
    {
        return;
    }

    let has_direct_thinking = chunk.content.iter().any(is_thinking);
    let mut prior_thinking = Vec::new();
    for message in accumulator.messages_mut() {
        if message.role != chunk.role
            || message
                .content
                .iter()
                .any(|content| matches!(content, MessageContent::ToolRequest(_)))
        {
            continue;
        }
        prior_thinking.extend(
            message
                .content
                .iter()
                .filter(|content| is_thinking(content))
                .cloned(),
        );
        message.content.retain(|content| !is_thinking(content));
    }
    accumulator
        .messages_mut()
        .retain(|message| !message.content.is_empty());

    if !has_direct_thinking && !prior_thinking.is_empty() {
        if let Some(tool_request) = chunk
            .content
            .iter()
            .position(|content| matches!(content, MessageContent::ToolRequest(_)))
        {
            chunk
                .content
                .splice(tool_request..tool_request, prior_thinking);
        }
    }
}

pub(super) fn chat_span(
    provider: &dyn Provider,
    model_config: &ModelConfig,
    session_id: &str,
    purpose: &'static str,
) -> tracing::Span {
    tracing::info_span!(
        target: "goose::state_machine",
        "chat",
        "gen_ai.operation.name" = "chat",
        "gen_ai.provider.name" = %provider.get_name(),
        "gen_ai.request.model" = %model_config.model_name,
        "gen_ai.response.model" = tracing::field::Empty,
        "gen_ai.usage.input_tokens" = tracing::field::Empty,
        "gen_ai.usage.output_tokens" = tracing::field::Empty,
        "goose.chat.purpose" = purpose,
        "error.type" = tracing::field::Empty,
        session.id = %session_id,
    )
}

pub(super) fn record_chat_usage(span: &tracing::Span, usage: &ProviderUsage) {
    span.record("gen_ai.response.model", usage.model.as_str());
    if let Some(tokens) = usage.usage.input_tokens {
        span.record("gen_ai.usage.input_tokens", tokens);
    }
    if let Some(tokens) = usage.usage.output_tokens {
        span.record("gen_ai.usage.output_tokens", tokens);
    }
}

pub struct InferenceRunner<'a> {
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    #[cfg(feature = "code-mode")]
    extension_manager: Arc<ExtensionManager>,
    prompt_manager: &'a Mutex<PromptManager>,
    tool_inspection_manager: &'a ToolInspectionManager,
    frontend_instructions: &'a Mutex<Option<String>>,
}

/// The agent-visible conversation as the provider sees it: tool requests left
/// unanswered by an earlier turn are dropped, since nothing will answer them now.
fn messages_for_provider(conversation: &Conversation, turn: &[Message]) -> Vec<Message> {
    let answered: std::collections::HashSet<&str> = conversation
        .messages()
        .iter()
        .flat_map(|message| message.get_tool_response_ids())
        .collect();
    let start = conversation.len() - turn.len();
    conversation
        .messages()
        .iter()
        .enumerate()
        .filter(|(_, message)| message.is_agent_visible())
        .map(|(index, message)| {
            let mut message = message.agent_visible_content();
            if index < start {
                message.content.retain(|content| match content {
                    MessageContent::ToolRequest(request) => answered.contains(request.id.as_str()),
                    _ => true,
                });
            }
            message
        })
        .filter(|message| !message.content.is_empty())
        .collect()
}

fn ends_with_provider_turn(messages: &[Message]) -> bool {
    messages.last().is_some_and(|message| {
        matches!(
            effective_role(message),
            EffectiveRole::User | EffectiveRole::Tool
        )
    })
}

impl<'a> InferenceRunner<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Arc<dyn Provider>,
        model_config: ModelConfig,
        #[cfg(feature = "code-mode")] extension_manager: Arc<ExtensionManager>,
        #[cfg(not(feature = "code-mode"))] _extension_manager: Arc<ExtensionManager>,
        prompt_manager: &'a Mutex<PromptManager>,
        tool_inspection_manager: &'a ToolInspectionManager,
        frontend_instructions: &'a Mutex<Option<String>>,
    ) -> Self {
        Self {
            provider,
            model_config,
            #[cfg(feature = "code-mode")]
            extension_manager,
            prompt_manager,
            tool_inspection_manager,
            frontend_instructions,
        }
    }

    async fn error_outcome(&self, err: &ProviderError, emit: &Emitter) -> Vec<GooseEffect> {
        #[cfg(feature = "telemetry")]
        crate::posthog::emit_error(err.telemetry_type(), &err.to_string());
        tracing::Span::current().record("error.type", err.telemetry_type());
        tracing::error!("LLM provider error: {err}");
        let message = Message::from_provider_error(err);
        let message = emit.message(message).await;
        vec![message.into()]
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for InferenceRunner<'_> {
    fn name(&self) -> &'static str {
        "llm"
    }

    async fn cancel(
        &self,
        _session: &Session,
        conversation: &Conversation,
        result: OperationResult<GooseEffect>,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let mut answered = conversation
            .messages()
            .iter()
            .flat_map(Message::get_tool_response_ids)
            .map(str::to_string)
            .collect::<std::collections::HashSet<_>>();
        let mut requests = Vec::new();
        let mut request_ids = std::collections::HashSet::new();

        let mut collect = |message: &Message| {
            for content in &message.content {
                match content {
                    MessageContent::ToolRequest(request) => {
                        if request_ids.insert(request.id.clone()) {
                            requests.push(request.clone());
                        }
                    }
                    MessageContent::ToolResponse(response) => {
                        answered.insert(response.id.clone());
                    }
                    _ => {}
                }
            }
        };
        for message in messages_since_kickoff(conversation)? {
            collect(message);
        }
        if let OperationResult::Applied(step) = &result {
            for effect in &step.effects {
                if let GooseEffect::Conversation(ConversationEffect::AppendMessage(message)) =
                    effect
                {
                    collect(message);
                }
            }
        }

        let mut response = Message::user();
        for request in requests {
            if !answered.contains(&request.id) {
                response.add_tool_response_with_metadata(
                    request.id,
                    Ok(rmcp::model::CallToolResult::error(vec![
                        rmcp::model::ContentBlock::text(CANCELLED_TOOL_RESPONSE),
                    ])),
                    request.metadata.as_ref(),
                );
            }
        }
        if response.get_tool_response_ids().is_empty() {
            return Ok(result);
        }

        let response = emit.message(response).await;
        match result {
            OperationResult::NotApplicable => applied([response.into()]),
            OperationResult::Applied(mut step) => {
                step.effects.push(response.into());
                Ok(OperationResult::Applied(step))
            }
        }
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        if command.command != "status" {
            return not_applicable();
        }

        let context_limit = self
            .provider
            .get_context_limit(&self.model_config)
            .await
            .unwrap_or_else(|_| self.model_config.context_limit());
        let context_tokens = session.usage.total_tokens.unwrap_or(0).max(0) as usize;
        let lifetime_tokens = session.accumulated_usage.total_tokens.unwrap_or(0).max(0) as usize;
        let context_pct = if context_limit > 0 {
            let pct = ((context_tokens as f64 / context_limit as f64) * 100.0).round() as usize;
            format!("{}%", pct.min(100))
        } else {
            "N/A".to_string()
        };
        let response = Message::assistant()
            .with_text(format!(
                "**Session status**\n\n\
                 - Model: {}\n\
                 - Provider: {}\n\
                 - Mode: {}\n\
                 - Tokens (lifetime): {}\n\
                 - Context: {} / {} tokens ({})",
                self.model_config.model_name,
                self.provider.get_name(),
                session.goose_mode,
                lifetime_tokens,
                context_tokens,
                context_limit,
                context_pct,
            ))
            .with_visibility(true, false);
        let command_message = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("status command conversation has no kickoff message"))?;
        let message_id = command_message
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        emit.message(command_message.with_visibility(true, false))
            .await;
        let response = emit.message(response).await;
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
}

#[async_trait]
impl Inference<Session, GooseEffect> for InferenceRunner<'_> {
    fn applies(&self, conversation: &Conversation) -> bool {
        let Ok(turn) = messages_since_kickoff(conversation) else {
            return false;
        };
        trailing_error(conversation).is_none()
            && ends_with_provider_turn(&messages_for_provider(conversation, turn))
    }

    async fn infer(
        &self,
        session: &Session,
        conversation: &Conversation,
        mut input: InferenceInput,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        if trailing_error(conversation).is_some() {
            return not_applicable();
        }

        let mut messages_for_provider = messages_for_provider(conversation, messages);
        if !ends_with_provider_turn(&messages_for_provider) {
            return not_applicable();
        }

        let span = chat_span(
            self.provider.as_ref(),
            &self.model_config,
            &session.id,
            "inference",
        );

        async {
            #[cfg(feature = "code-mode")]
            let code_execution_mode = self
                .extension_manager
                .is_extension_enabled(
                    crate::agents::platform_extensions::code_execution::EXTENSION_NAME,
                )
                .await;
            #[cfg(not(feature = "code-mode"))]
            let code_execution_mode = false;

            let goose_mode = session.goose_mode;
            if goose_mode == GooseMode::SmartApprove {
                self.tool_inspection_manager
                    .apply_tool_annotations(&input.tools);
            }
            let tools = crate::agents::reply_parts::prepare_inference_tools(
                input.tools,
                code_execution_mode,
            );
            if let Some(frontend_instructions) = self.frontend_instructions.lock().await.clone() {
                input
                    .prompt_parts
                    .push(("frontend".to_string(), frontend_instructions));
            }
            let system_prompt = self.prompt_manager.lock().await.build_system_prompt(
                &session.working_dir,
                input.prompt_parts,
                goose_mode,
            );
            let (tools, toolshim_tools, system_prompt) =
                crate::agents::reply_parts::prepare_tools_for_provider(
                    tools,
                    system_prompt,
                    &self.model_config,
                );
            let mut available_tools = tools
                .iter()
                .chain(toolshim_tools.iter())
                .map(|tool| tool.name.as_ref())
                .collect::<Vec<_>>();
            available_tools.sort_unstable();
            available_tools.dedup();
            let available_tools = available_tools.join(", ");
            for message in &mut messages_for_provider {
                for content in &mut message.content {
                    let MessageContent::ToolResponse(response) = content else {
                        continue;
                    };
                    let Some(metadata) = &mut response.metadata else {
                        continue;
                    };
                    if metadata.remove(UNCLAIMED_TOOL_ERROR).is_none() {
                        continue;
                    }
                    let Ok(result) = &mut response.tool_result else {
                        continue;
                    };
                    result.content.push(rmcp::model::ContentBlock::text(format!(
                        "Available tools: [{}].",
                        available_tools
                    )));
                }
            }

            let context_limit = self
                .provider
                .get_context_limit(&self.model_config)
                .await
                .unwrap_or_else(|_| self.model_config.context_limit());
            let provider_name = self.provider.get_name();
            if let Some(session_id) = super::super::latest_provider_session_id(
                conversation.messages(),
                provider_name,
            ) {
                if let Err(error) = self.provider.resume(session_id).await {
                    tracing::warn!(
                        provider = provider_name,
                        %error,
                        "Could not resume provider session; continuing with a handoff"
                    );
                }
            }
            let turn = messages_since_kickoff(conversation)?;
            let turn_start = turn
                .first()
                .and_then(|message| chrono::DateTime::from_timestamp(message.created, 0))
                .map(|timestamp| timestamp.with_timezone(&chrono::Local))
                .unwrap_or_else(chrono::Local::now);
            // Persist a fresh turn-context event only when its bytes changed
            // (e.g. the turn budget ticked); earlier events stay in place.
            let last_turn_context = turn
                .iter()
                .rev()
                .find(|message| message.is_turn_context())
                .map(Message::as_concat_text);
            let turn_context = crate::agents::moim::turn_context_event(
                &session.working_dir,
                Some(context_limit),
                input.moim_parts,
                turn_start,
            )
            .filter(|event| Some(event.as_concat_text()) != last_turn_context);
            if let Some(event) = &turn_context {
                messages_for_provider.push(event.clone());
            }
            let conversation_for_provider = Conversation::new_unvalidated(messages_for_provider);
            let mut usage_effects: Vec<GooseEffect> =
                turn_context.into_iter().map(GooseEffect::from).collect();

            let stream = crate::agents::reply_parts::stream_response_from_provider(
                self.provider.clone(),
                self.model_config.clone(),
                &session.id,
                &system_prompt,
                conversation_for_provider.messages(),
                &tools,
                &toolshim_tools,
            )
            .await;

            let mut stream = match stream {
                Ok(stream) => stream,
                Err(err) => {
                    usage_effects.extend(self.error_outcome(&err, emit).await);
                    return applied(usage_effects);
                }
            };

            let requested_model = self.model_config.model_name.clone();
            let resolved_model = self
                .provider
                .fetch_model_info(&requested_model)
                .await
                .ok()
                .and_then(|model_info| model_info.resolved_model);
            let provider_session_id = self.provider.provider_session_id();
            let inference = Some(InferenceMetadata {
                provider: self.provider.get_name().to_string(),
                requested_model,
                resolved_model,
                provider_session_id,
            });

            let mut accumulator = Conversation::empty();
            let mut tool_request_ids = std::collections::HashSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = emit.cancelled() => break,
                    next = stream.next() => {
                        let Some(result) = next else { break };
                        let (msg_opt, usage_opt) = match result {
                            Ok(chunk) => chunk,
                            Err(err) => {
                                usage_effects.extend(accumulator.into_iter().map(GooseEffect::from));
                                usage_effects.extend(self.error_outcome(&err, emit).await);
                                return applied(usage_effects);
                            }
                        };
                        if let Some(usage) = usage_opt {
                            let span = tracing::Span::current();
                            record_chat_usage(&span, &usage);
                            usage_effects.push(GooseEffect::RecordUsage(usage));
                        }
                        if let Some(mut chunk) = msg_opt {
                            if let Some(inference) = &inference {
                                chunk = chunk.with_inference_if_assistant(inference.clone());
                            }
                            chunk.content.retain(|content| match content {
                                MessageContent::ToolRequest(request) => {
                                    tool_request_ids.insert(request.id.clone())
                                }
                                _ => true,
                            });
                            normalize_tool_call_thinking(&mut accumulator, &mut chunk);
                            if chunk.content.is_empty() {
                                if chunk.metadata.output_token_limit_reached {
                                    chunk = emit.message(chunk).await;
                                }
                                accumulator.push(chunk);
                                continue;
                            }
                            let chunk = emit.message(chunk).await;
                            accumulator.push(chunk);
                        }
                    }
                }
            }

            let empty_response = !accumulator
                .iter()
                .any(|message| message.metadata.output_token_limit_reached)
                && accumulator.iter().all(|message| {
                    message.content.iter().all(|content| match content {
                        MessageContent::Text(text) => text.text.trim().is_empty(),
                        MessageContent::Thinking(thinking) => thinking.thinking.trim().is_empty(),
                        _ => false,
                    })
                });
            if empty_response {
                let message = Message::assistant().with_text(EMPTY_RESPONSE_MESSAGE);
                let message = emit.message(message).await;
                usage_effects.push(message.into());
                return yielded_with(usage_effects);
            }

            let has_recorded_usage = usage_effects
                .iter()
                .any(|effect| matches!(effect, GooseEffect::RecordUsage(_)));
            if !has_recorded_usage {
                let mut usage = ProviderUsage::new(
                    self.model_config.model_name.clone(),
                    goose_providers::conversation::token_usage::Usage::default(),
                );
                if let Some(response) = accumulator.last() {
                    crate::providers::usage_estimator::ensure_usage_tokens(
                        &mut usage,
                        &system_prompt,
                        conversation_for_provider.messages(),
                        response,
                        &tools,
                    )
                    .await?;
                    record_chat_usage(&tracing::Span::current(), &usage);
                    usage_effects.push(GooseEffect::RecordUsage(usage));
                }
            }

            usage_effects.extend(accumulator.into_iter().map(Into::into));
            applied(usage_effects)
        }
        .instrument(span)
        .await
    }
}
