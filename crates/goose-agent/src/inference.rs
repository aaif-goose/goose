//! Provider inference operation for the unrolled agent loop.

use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use goose_provider_types::base::Provider;
use goose_provider_types::conversation::message::{InferenceMetadata, Message, MessageContent};
use goose_provider_types::conversation::token_usage::ProviderUsage;
use goose_provider_types::conversation::{
    effective_role, fix_conversation, merge_consecutive_messages_for_request, Conversation,
    EffectiveRole,
};
use goose_provider_types::errors::ProviderError;
use goose_provider_types::model::ModelConfig;
use tracing_futures::Instrument;

use crate::operation::{
    applied, messages_since_kickoff, not_applicable, trailing_error, yielded_with, Emitter,
    Inference, InferenceInput, Operation, OperationResult,
};
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

pub struct PreparedInferenceRequest {
    pub system_prompt: String,
    pub tools: Vec<rmcp::model::Tool>,
    pub additional_messages: Vec<Message>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait InferenceRequestPreparer<S>: MaybeSend + MaybeSync {
    async fn prepare_session(&self, _session: &S) -> Result<Option<S>> {
        Ok(None)
    }

    async fn prepare(
        &self,
        session: &S,
        conversation: &Conversation,
        input: InferenceInput,
    ) -> Result<PreparedInferenceRequest>;
}

pub struct IdentityInferenceRequestPreparer;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: MaybeSync> InferenceRequestPreparer<S> for IdentityInferenceRequestPreparer {
    async fn prepare(
        &self,
        _session: &S,
        _conversation: &Conversation,
        input: InferenceInput,
    ) -> Result<PreparedInferenceRequest> {
        Ok(PreparedInferenceRequest {
            system_prompt: input
                .prompt_parts
                .into_iter()
                .map(|(_, part)| part)
                .collect::<Vec<_>>()
                .join("\n\n"),
            tools: input.tools,
            additional_messages: Vec::new(),
        })
    }
}

pub trait InferenceEffect: From<Message> + MaybeSend + 'static {
    fn record_usage(usage: ProviderUsage) -> Self;
}

const EMPTY_RESPONSE_MESSAGE: &str =
    "The model returned an empty response. Please resend your message to continue.";

fn is_thinking(content: &MessageContent) -> bool {
    matches!(
        content,
        MessageContent::Thinking(_) | MessageContent::RedactedThinking(_)
    )
}

fn drop_repeated_tool_call_thinking(accumulator: &Conversation, chunk: &mut Message) {
    if !chunk
        .content
        .iter()
        .any(|content| matches!(content, MessageContent::ToolRequest(_)))
    {
        return;
    }
    let prior: Vec<&MessageContent> = accumulator
        .iter()
        .filter(|message| message.role == chunk.role)
        .flat_map(|message| message.content.iter())
        .filter(|content| is_thinking(content))
        .collect();
    chunk
        .content
        .retain(|content| !(is_thinking(content) && prior.contains(&content)));
}

pub fn chat_span(
    provider: &dyn Provider,
    model_config: &ModelConfig,
    session_id: &str,
    purpose: &'static str,
) -> tracing::Span {
    let span = tracing::info_span!(
        target: "goose::state_machine",
        "chat",
        "gen_ai.operation.name" = "chat",
        "gen_ai.provider.name" = %provider.get_name(),
        "gen_ai.request.model" = %model_config.model_name,
        "gen_ai.request.temperature" = tracing::field::Empty,
        "gen_ai.request.max_tokens" = tracing::field::Empty,
        "gen_ai.response.model" = tracing::field::Empty,
        "gen_ai.response.finish_reasons" = tracing::field::Empty,
        "gen_ai.response.id" = tracing::field::Empty,
        "gen_ai.usage.input_tokens" = tracing::field::Empty,
        "gen_ai.usage.output_tokens" = tracing::field::Empty,
        "goose.chat.purpose" = purpose,
        "error.type" = tracing::field::Empty,
        session.id = %session_id,
    );
    record_request_params(&span, model_config);
    span
}

fn is_empty_response(message: &Message) -> bool {
    message.content.iter().all(|content| match content {
        MessageContent::Text(text) => text.text.trim().is_empty(),
        MessageContent::Thinking(thinking) => {
            thinking.thinking.trim().is_empty() && thinking.signature.is_empty()
        }
        _ => false,
    })
}

pub fn ends_with_successful_tool_response(messages: &[Message]) -> bool {
    let Some(message) = messages.last() else {
        return false;
    };
    let mut responses = message
        .content
        .iter()
        .filter_map(MessageContent::as_tool_response)
        .peekable();
    responses.peek().is_some()
        && responses.all(|response| {
            response
                .tool_result
                .as_ref()
                .is_ok_and(|result| !result.is_error.unwrap_or(false))
        })
}

fn record_request_params(span: &tracing::Span, model_config: &ModelConfig) {
    if let Some(temperature) = model_config.temperature {
        span.record("gen_ai.request.temperature", temperature as f64);
    }
    if let Some(max_tokens) = model_config.max_tokens {
        span.record("gen_ai.request.max_tokens", max_tokens as i64);
    }
}

pub fn record_chat_usage(span: &tracing::Span, usage: &ProviderUsage) {
    span.record("gen_ai.response.model", usage.model.as_str());
    if let Some(tokens) = usage.usage.input_tokens {
        span.record("gen_ai.usage.input_tokens", tokens);
    }
    if let Some(tokens) = usage.usage.output_tokens {
        span.record("gen_ai.usage.output_tokens", tokens);
    }
    if let Some(tokens) = usage.usage.cache_read_input_tokens {
        span.record("gen_ai.usage.cache_read.input_tokens", tokens);
    }
    if let Some(tokens) = usage.usage.cache_write_input_tokens {
        span.record("gen_ai.usage.cache_creation.input_tokens", tokens);
    }
    if let Some(reasons) = &usage.finish_reasons {
        let reasons_json = serde_json::to_string(reasons).unwrap_or_default();
        span.record("gen_ai.response.finish_reasons", reasons_json.as_str());
    }
    if let Some(id) = &usage.response_id {
        span.record("gen_ai.response.id", id.as_str());
    }
}

struct InferenceOutput {
    accumulator: Conversation,
    additional_messages: Vec<Message>,
    usage: Option<ProviderUsage>,
}

impl Default for InferenceOutput {
    fn default() -> Self {
        Self {
            accumulator: Conversation::empty(),
            additional_messages: Vec::new(),
            usage: None,
        }
    }
}

pub struct InferenceRunner<'a, S, E> {
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    request_preparer: Arc<dyn InferenceRequestPreparer<S> + 'a>,
    output: Mutex<InferenceOutput>,
    effect: std::marker::PhantomData<fn() -> E>,
}

/// The agent-visible conversation as the provider sees it: tool requests left
/// unanswered by an earlier turn are dropped, since nothing will answer them now.
fn messages_for_provider(
    conversation: &Conversation,
    turn: &[Message],
    keep_empty_messages: bool,
) -> Vec<Message> {
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
        .filter(|message| keep_empty_messages || !message.content.is_empty())
        .collect()
}

fn latest_provider_session_id<'a>(
    conversation: &'a Conversation,
    provider: &str,
) -> Option<&'a str> {
    conversation
        .messages()
        .iter()
        .rev()
        .find_map(|message| message.metadata.inference.as_ref())
        .filter(|inference| inference.provider == provider)
        .and_then(|inference| inference.provider_session_id.as_deref())
}

fn ends_with_provider_turn(messages: &[Message]) -> bool {
    messages.last().is_some_and(|message| {
        matches!(
            effective_role(message),
            EffectiveRole::User | EffectiveRole::Tool
        )
    })
}

fn should_infer(conversation: &Conversation, turn: &[Message]) -> bool {
    let projected = messages_for_provider(conversation, turn, true);
    if projected
        .last()
        .is_some_and(|message| message.content.is_empty())
    {
        return false;
    }
    ends_with_provider_turn(&messages_for_provider(conversation, turn, false))
}

fn inference_span(provider: &dyn Provider, model_config: &ModelConfig) -> tracing::Span {
    let span = tracing::info_span!(
        target: "goose::state_machine",
        "chat",
        "gen_ai.operation.name" = "chat",
        "gen_ai.provider.name" = %provider.get_name(),
        "gen_ai.request.model" = %model_config.model_name,
        "gen_ai.request.temperature" = tracing::field::Empty,
        "gen_ai.request.max_tokens" = tracing::field::Empty,
        "gen_ai.response.model" = tracing::field::Empty,
        "gen_ai.response.finish_reasons" = tracing::field::Empty,
        "gen_ai.response.id" = tracing::field::Empty,
        "gen_ai.usage.input_tokens" = tracing::field::Empty,
        "gen_ai.usage.output_tokens" = tracing::field::Empty,
        "error.type" = tracing::field::Empty,
    );
    record_request_params(&span, model_config);
    span
}

impl<'a, S: MaybeSync, E: InferenceEffect> InferenceRunner<'a, S, E> {
    pub fn new(provider: Arc<dyn Provider>, model_config: ModelConfig) -> Self {
        Self {
            provider,
            model_config,
            request_preparer: Arc::new(IdentityInferenceRequestPreparer),
            output: Mutex::new(InferenceOutput::default()),
            effect: std::marker::PhantomData,
        }
    }

    pub fn with_request_preparer(
        mut self,
        request_preparer: Arc<dyn InferenceRequestPreparer<S> + 'a>,
    ) -> Self {
        self.request_preparer = request_preparer;
        self
    }

    fn output(&self) -> MutexGuard<'_, InferenceOutput> {
        self.output.lock().expect("inference output lock poisoned")
    }

    fn take_output(&self) -> Vec<E> {
        let output = std::mem::take(&mut *self.output());
        let mut effects: Vec<E> = output
            .additional_messages
            .into_iter()
            .map(E::from)
            .collect();
        if let Some(usage) = output.usage {
            effects.push(E::record_usage(usage));
        }
        effects.extend(output.accumulator.into_iter().map(E::from));
        effects
    }

    async fn emit_message(&self, message: Message, emit: &Emitter) {
        let message = message.with_generated_id_if_missing();
        self.output().accumulator.push(message.clone());
        emit.message(message).await;
    }

    async fn error_outcome(
        &self,
        err: &ProviderError,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        tracing::Span::current().record("error.type", err.telemetry_type());
        tracing::error!("LLM provider error: {err}");
        self.emit_message(Message::from_provider_error(err), emit)
            .await;
        applied(self.take_output())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: MaybeSync, E: InferenceEffect> Operation<S, E> for InferenceRunner<'_, S, E> {
    fn name(&self) -> &'static str {
        "llm"
    }

    async fn cancel(&self, _session: &S, _conversation: &Conversation, _emit: &Emitter) -> Vec<E> {
        self.take_output()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: MaybeSync, E: InferenceEffect> Inference<S, E> for InferenceRunner<'_, S, E> {
    fn applies(&self, conversation: &Conversation) -> bool {
        let Ok(turn) = messages_since_kickoff(conversation) else {
            return false;
        };
        trailing_error(conversation).is_none() && should_infer(conversation, turn)
    }

    async fn prepare_session(&self, session: &S) -> Result<Option<S>> {
        self.request_preparer.prepare_session(session).await
    }

    async fn infer(
        &self,
        session: &S,
        conversation: &Conversation,
        input: InferenceInput,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = messages_since_kickoff(conversation)?;
        if trailing_error(conversation).is_some() {
            return not_applicable();
        }

        if !should_infer(conversation, messages) {
            return not_applicable();
        }
        let mut messages_for_provider = messages_for_provider(conversation, messages, false);

        let span = inference_span(self.provider.as_ref(), &self.model_config);

        async {
            let PreparedInferenceRequest {
                system_prompt,
                tools,
                additional_messages,
            } = self
                .request_preparer
                .prepare(session, conversation, input)
                .await?;

            for message in &additional_messages {
                messages_for_provider.push(message.clone());
            }
            self.output().additional_messages = additional_messages;

            let provider_name = self.provider.get_name();
            if let Some(session_id) = latest_provider_session_id(conversation, provider_name) {
                if let Err(error) = self.provider.resume(session_id).await {
                    tracing::warn!(
                        provider = provider_name,
                        %error,
                        "Could not resume provider session; continuing with a handoff"
                    );
                }
            }

            let projected =
                Conversation::new_unvalidated(messages_for_provider).agent_visible_messages();
            let (fixed, _) = fix_conversation(Conversation::new_unvalidated(projected));
            let conversation_for_provider = Conversation::new_unvalidated(
                merge_consecutive_messages_for_request(fixed.messages().clone()),
            );
            let stream = self
                .provider
                .stream(
                    &self.model_config,
                    &system_prompt,
                    conversation_for_provider.messages(),
                    &tools,
                )
                .await;

            let mut stream = match stream {
                Ok(stream) => stream,
                Err(err) => return self.error_outcome(&err, emit).await,
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

            let mut tool_request_ids = std::collections::HashSet::new();
            while let Some(result) = stream.next().await {
                let (msg_opt, usage_opt) = match result {
                    Ok(chunk) => chunk,
                    Err(err) => return self.error_outcome(&err, emit).await,
                };
                if let Some(usage) = usage_opt {
                    record_chat_usage(&tracing::Span::current(), &usage);
                    self.output().usage = Some(usage);
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
                    drop_repeated_tool_call_thinking(&self.output().accumulator, &mut chunk);
                    if chunk.content.is_empty() && !chunk.metadata.output_token_limit_reached {
                        self.output().accumulator.push(chunk);
                    } else {
                        self.emit_message(chunk, emit).await;
                    }
                }
            }

            let empty_output = {
                let output = self.output();
                !output
                    .accumulator
                    .iter()
                    .any(|message| message.metadata.output_token_limit_reached)
                    && output.accumulator.iter().all(is_empty_response)
            };
            let successful_tool_response =
                ends_with_successful_tool_response(conversation.messages());
            if empty_output && !successful_tool_response && !emit.cancel_token().is_cancelled() {
                self.output().accumulator.clear();
                self.emit_message(Message::assistant().with_text(EMPTY_RESPONSE_MESSAGE), emit)
                    .await;
                return yielded_with(self.take_output());
            }

            if empty_output && successful_tool_response {
                let message = {
                    let mut output = self.output();
                    let mut message = output
                        .accumulator
                        .last()
                        .cloned()
                        .unwrap_or_else(Message::assistant);
                    message.content.clear();
                    message.metadata.user_visible = false;
                    message.metadata.agent_visible = true;
                    output.accumulator.clear();
                    message
                };
                self.emit_message(message, emit).await;
            }
            applied(self.take_output())
        }
        .instrument(span)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_session_id_comes_only_from_latest_inference() {
        let conversation = Conversation::new_unvalidated([
            Message::assistant().with_inference(InferenceMetadata {
                provider: "provider-a".to_string(),
                requested_model: "model".to_string(),
                resolved_model: None,
                provider_session_id: Some("session-a".to_string()),
            }),
            Message::assistant().with_inference(InferenceMetadata {
                provider: "provider-b".to_string(),
                requested_model: "model".to_string(),
                resolved_model: None,
                provider_session_id: Some("session-b".to_string()),
            }),
        ]);

        assert_eq!(
            latest_provider_session_id(&conversation, "provider-b"),
            Some("session-b")
        );
        assert_eq!(
            latest_provider_session_id(&conversation, "provider-a"),
            None
        );
    }

    #[test]
    fn signed_thinking_without_text_is_not_an_empty_response() {
        assert!(is_empty_response(
            &Message::assistant().with_content(MessageContent::thinking("", ""))
        ));
        assert!(!is_empty_response(
            &Message::assistant().with_content(MessageContent::thinking("", "sig-omitted"))
        ));
    }

    #[test]
    fn whitespace_only_text_is_an_empty_response() {
        assert!(is_empty_response(&Message::assistant().with_text(" \n\t ")));
    }
}
