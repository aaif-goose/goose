//! Provider inference operation for the unrolled agent loop.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use goose_provider_types::base::{MessageStream, Provider};
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
    applied, messages_since_kickoff, not_applicable, trailing_error, Emitter, Inference,
    InferenceInput, Operation, OperationResult,
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

pub const EMPTY_RESPONSE_MESSAGE: &str =
    "The model returned an empty response. Please resend your message to continue.";
const MAX_EMPTY_RESPONSE_RETRIES: usize = 3;
const EMPTY_RESPONSE_NOTE_SCOPE: &str = "inference";
const EMPTY_RESPONSE_NOTE: &str = "empty_response";

/// The model's response stayed empty after retries. The fallback message is stored
/// hidden so that operations owning the end of a turn (recipe retries, final output)
/// can take over; when none does, it is revealed to the user.
pub fn is_empty_response_marker(message: &Message) -> bool {
    message
        .metadata
        .operation_note(EMPTY_RESPONSE_NOTE_SCOPE, EMPTY_RESPONSE_NOTE)
        .is_some()
}
const CANCELLED_TOOL_RESPONSE: &str = "Tool call was cancelled before execution";

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

pub struct InferenceRunner<'a, S, E> {
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    request_preparer: Arc<dyn InferenceRequestPreparer<S> + 'a>,
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

fn cancellation_response(persisted: &[Message], pending: &[Message]) -> Option<Message> {
    let mut answered = persisted
        .iter()
        .chain(pending)
        .flat_map(Message::get_tool_response_ids)
        .collect::<std::collections::HashSet<_>>();
    let mut request_ids = std::collections::HashSet::new();
    let mut response = Message::user();
    for request in persisted
        .iter()
        .chain(pending)
        .flat_map(|message| &message.content)
        .filter_map(MessageContent::as_tool_request)
    {
        if request_ids.insert(request.id.as_str()) && !answered.remove(request.id.as_str()) {
            response.add_tool_response_with_metadata(
                request.id.clone(),
                Ok(rmcp::model::CallToolResult::error(vec![
                    rmcp::model::ContentBlock::text(CANCELLED_TOOL_RESPONSE),
                ])),
                request.metadata.as_ref(),
            );
        }
    }
    (!response.get_tool_response_ids().is_empty()).then_some(response)
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

/// The emitted partial response and terminal state of an inference stream.
pub struct InferenceResponse {
    pub messages: Conversation,
    pub usage: Option<ProviderUsage>,
    pub cancelled: bool,
    pub error: Option<ProviderError>,
}

/// Consume a provider stream, retaining partial output even on cancellation or error.
pub async fn consume_inference_stream(
    mut stream: MessageStream,
    inference: InferenceMetadata,
    emit: &Emitter,
) -> InferenceResponse {
    let mut error = None;
    let mut accumulator = Conversation::empty();
    let mut tool_request_ids = std::collections::HashSet::new();
    let mut provider_usage = None;
    let mut cancelled = false;
    loop {
        tokio::select! {
            biased;
            _ = emit.cancelled() => {
                cancelled = true;
                break;
            },
            next = stream.next() => {
                let Some(result) = next else { break };
                let (msg_opt, usage_opt) = match result {
                    Ok(chunk) => chunk,
                    Err(err) => {
                        error = Some(err);
                        break;
                    }
                };
                if let Some(usage) = usage_opt {
                    let span = tracing::Span::current();
                    record_chat_usage(&span, &usage);
                    provider_usage = Some(usage);
                }
                if let Some(mut chunk) = msg_opt {
                    chunk = chunk.with_inference_if_assistant(inference.clone());
                    chunk.content.retain(|content| match content {
                        MessageContent::ToolRequest(request) => {
                            tool_request_ids.insert(request.id.clone())
                        }
                        _ => true,
                    });
                    drop_repeated_tool_call_thinking(&accumulator, &mut chunk);
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

    drop(stream);
    InferenceResponse {
        messages: accumulator,
        usage: provider_usage,
        cancelled,
        error,
    }
}

impl InferenceResponse {
    fn is_unexpected_empty_response(&self, conversation: &Conversation, emit: &Emitter) -> bool {
        self.error.is_none()
            && !self.cancelled
            && !emit.cancel_token().is_cancelled()
            && !ends_with_successful_tool_response(conversation.messages())
            && !self
                .messages
                .iter()
                .any(|message| message.metadata.output_token_limit_reached)
            && self.messages.iter().all(is_empty_response)
    }

    /// Produce persistence effects, leaving empty responses for end-of-turn operations.
    pub async fn finish<E: InferenceEffect>(
        self,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let empty_response = self.is_unexpected_empty_response(conversation, emit);
        let Self {
            mut messages,
            usage,
            cancelled,
            error,
        } = self;
        let mut usage_effects = Vec::new();

        if let Some(usage) = usage {
            usage_effects.push(E::record_usage(usage));
        }

        if let Some(err) = error {
            usage_effects.extend(messages.into_iter().map(E::from));
            usage_effects.extend(inference_error(&err, emit).await);
            return applied(usage_effects);
        }

        if cancelled || emit.cancel_token().is_cancelled() {
            if let Some(response) =
                cancellation_response(messages_since_kickoff(conversation)?, messages.messages())
            {
                let response = emit.message(response).await;
                messages.push(response);
            }
        }

        if empty_response {
            let mut marker = Message::assistant()
                .with_text(EMPTY_RESPONSE_MESSAGE)
                .with_visibility(false, false);
            marker.metadata.set_operation_note(
                EMPTY_RESPONSE_NOTE_SCOPE,
                EMPTY_RESPONSE_NOTE,
                serde_json::Value::Bool(true),
            );
            usage_effects.push(E::from(marker));
            return applied(usage_effects);
        }

        if ends_with_successful_tool_response(conversation.messages())
            && !messages
                .iter()
                .any(|message| message.metadata.output_token_limit_reached)
            && messages.iter().all(is_empty_response)
        {
            let mut message = messages
                .into_iter()
                .last()
                .unwrap_or_else(Message::assistant);
            message.content.clear();
            message.metadata.user_visible = false;
            message.metadata.agent_visible = true;
            let message = emit.message(message).await;
            usage_effects.push(E::from(message));
        } else {
            usage_effects.extend(messages.into_iter().map(|message| E::from(message)));
        }
        applied(usage_effects)
    }
}

/// Emit and persist a provider error using the standard inference outcome.
pub async fn inference_error<E: InferenceEffect>(err: &ProviderError, emit: &Emitter) -> Vec<E> {
    tracing::Span::current().record("error.type", err.telemetry_type());
    tracing::error!("LLM provider error: {err}");
    let message = Message::from_provider_error(err);
    let message = emit.message(message).await;
    vec![E::from(message)]
}

/// Answer unresolved tool calls when cancellation precedes inference.
pub async fn cancel_inference<E: InferenceEffect>(
    conversation: &Conversation,
    result: OperationResult<E>,
    emit: &Emitter,
) -> Result<OperationResult<E>> {
    let OperationResult::NotApplicable = result else {
        return Ok(result);
    };
    let Some(response) = cancellation_response(messages_since_kickoff(conversation)?, &[]) else {
        return Ok(OperationResult::NotApplicable);
    };
    let response = emit.message(response).await;
    applied([E::from(response)])
}

impl<'a, S: MaybeSync, E: InferenceEffect> InferenceRunner<'a, S, E> {
    pub fn new(provider: Arc<dyn Provider>, model_config: ModelConfig) -> Self {
        Self {
            provider,
            model_config,
            request_preparer: Arc::new(IdentityInferenceRequestPreparer),
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
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: MaybeSync, E: InferenceEffect> Operation<S, E> for InferenceRunner<'_, S, E> {
    fn name(&self) -> &'static str {
        "llm"
    }

    async fn cancel(
        &self,
        _session: &S,
        conversation: &Conversation,
        result: OperationResult<E>,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        cancel_inference(conversation, result, emit).await
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
            let mut usage_effects: Vec<E> = additional_messages.into_iter().map(E::from).collect();

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
            let mut empty_responses = 0;
            let response = loop {
                let stream = self
                    .provider
                    .stream(
                        &self.model_config,
                        &system_prompt,
                        conversation_for_provider.messages(),
                        &tools,
                    )
                    .await;

                let stream = match stream {
                    Ok(stream) => stream,
                    Err(err) => {
                        usage_effects.extend(inference_error(&err, emit).await);
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
                let inference = InferenceMetadata {
                    provider: self.provider.get_name().to_string(),
                    requested_model,
                    resolved_model,
                    provider_session_id,
                };

                let mut response = consume_inference_stream(stream, inference, emit).await;
                if !response.is_unexpected_empty_response(conversation, emit)
                    || empty_responses == MAX_EMPTY_RESPONSE_RETRIES
                {
                    break response;
                }
                if let Some(usage) = response.usage.take() {
                    usage_effects.push(E::record_usage(usage));
                }
                empty_responses += 1;
                tracing::warn!(
                    "Provider returned an empty response; retrying ({empty_responses}/{MAX_EMPTY_RESPONSE_RETRIES})"
                );
            };

            let mut result = response.finish::<E>(conversation, emit).await?;
            if let OperationResult::Applied(step) = &mut result {
                usage_effects.append(&mut step.effects);
                step.effects = usage_effects;
            }
            Ok(result)
        }
        .instrument(span)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    enum TestEffect {
        Message(Message),
        Usage(ProviderUsage),
    }

    impl From<Message> for TestEffect {
        fn from(message: Message) -> Self {
            Self::Message(message)
        }
    }

    impl InferenceEffect for TestEffect {
        fn record_usage(usage: ProviderUsage) -> Self {
            Self::Usage(usage)
        }
    }

    fn test_emitter() -> (
        Emitter,
        tokio::sync::mpsc::Receiver<crate::events::AgentEvent>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        (
            Emitter::new(tx, tokio_util::sync::CancellationToken::new()),
            rx,
        )
    }

    fn test_inference() -> InferenceMetadata {
        InferenceMetadata {
            provider: "test".into(),
            requested_model: "model".into(),
            resolved_model: None,
            provider_session_id: None,
        }
    }

    fn applied_result(
        result: OperationResult<TestEffect>,
    ) -> crate::operation::StepResult<TestEffect> {
        match result {
            OperationResult::Applied(step) => step,
            OperationResult::NotApplicable => panic!("expected applied inference"),
        }
    }

    #[tokio::test]
    async fn shared_stream_error_preserves_partial_output_and_usage() {
        let (emit, _events) = test_emitter();
        let usage = ProviderUsage::new("model".into(), Default::default());
        let stream: MessageStream = Box::pin(futures::stream::iter([
            Ok((Some(Message::assistant().with_text("partial")), Some(usage))),
            Err(ProviderError::RequestFailed("failed".into())),
        ]));
        let response = consume_inference_stream(stream, test_inference(), &emit).await;
        assert!(response.error.is_some());
        assert_eq!(response.messages.len(), 1);
        let step = applied_result(
            response
                .finish::<TestEffect>(
                    &Conversation::new_unvalidated([Message::user().with_text("hi")]),
                    &emit,
                )
                .await
                .unwrap(),
        );
        assert!(!step.yield_to_client);
        assert_eq!(step.effects.len(), 3);
        assert!(matches!(&step.effects[0], TestEffect::Usage(usage) if usage.model == "model"));
        assert!(
            matches!(&step.effects[1], TestEffect::Message(message) if message.error_kind().is_none())
        );
        assert!(
            matches!(&step.effects[2], TestEffect::Message(message) if message.error_kind().is_some())
        );
    }

    #[tokio::test]
    async fn shared_stream_cancellation_is_biased_and_answers_pending_tools() {
        let (emit, _events) = test_emitter();
        emit.cancel_token().cancel();
        let stream: MessageStream = Box::pin(futures::stream::iter([Ok((
            Some(Message::assistant().with_text("not consumed")),
            None,
        ))]));
        let response = consume_inference_stream(stream, test_inference(), &emit).await;
        assert!(response.cancelled);
        assert!(response.messages.is_empty());
        let conversation = Conversation::new_unvalidated([
            Message::user().with_text("run it"),
            Message::assistant().with_tool_request(
                "pending",
                Ok(rmcp::model::CallToolRequestParams::new("tool")),
            ),
        ]);
        let step = applied_result(
            response
                .finish::<TestEffect>(&conversation, &emit)
                .await
                .unwrap(),
        );
        assert!(!step.yield_to_client);
        assert!(
            matches!(&step.effects[..], [TestEffect::Message(message)] if message.get_tool_response_ids().contains("pending"))
        );
        let step = applied_result(
            cancel_inference::<TestEffect>(&conversation, OperationResult::NotApplicable, &emit)
                .await
                .unwrap(),
        );
        assert_eq!(step.effects.len(), 1);
    }

    #[tokio::test]
    async fn shared_empty_response_is_hidden_but_successful_tool_completion_is_silent() {
        let (emit, _events) = test_emitter();
        let conversation = Conversation::new_unvalidated([Message::user().with_text("hi")]);
        let response =
            consume_inference_stream(Box::pin(futures::stream::empty()), test_inference(), &emit)
                .await;
        let step = applied_result(
            response
                .finish::<TestEffect>(&conversation, &emit)
                .await
                .unwrap(),
        );
        assert!(!step.yield_to_client);
        assert!(
            matches!(&step.effects[..], [TestEffect::Message(message)] if is_empty_response_marker(message) && !message.is_user_visible() && !message.is_agent_visible())
        );

        let conversation = Conversation::new_unvalidated([
            Message::user().with_text("hi"),
            Message::user()
                .with_tool_response("done", Ok(rmcp::model::CallToolResult::success(vec![]))),
        ]);
        let response =
            consume_inference_stream(Box::pin(futures::stream::empty()), test_inference(), &emit)
                .await;
        let step = applied_result(
            response
                .finish::<TestEffect>(&conversation, &emit)
                .await
                .unwrap(),
        );
        assert!(!step.yield_to_client);
        assert!(
            matches!(&step.effects[..], [TestEffect::Message(message)] if message.content.is_empty() && !message.is_user_visible() && message.is_agent_visible())
        );
    }

    struct EmptyResponseProvider {
        calls: std::sync::atomic::AtomicUsize,
        empty_attempts: usize,
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl Provider for EmptyResponseProvider {
        fn get_name(&self) -> &str {
            "empty-response-test"
        }

        async fn stream(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[rmcp::model::Tool],
        ) -> std::result::Result<MessageStream, ProviderError> {
            let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let text = if call < self.empty_attempts {
                ""
            } else {
                "recovered"
            };
            Ok(Box::pin(futures::stream::iter([Ok((
                Some(Message::assistant().with_text(text)),
                Some(ProviderUsage::new("model".into(), Default::default())),
            ))])))
        }
    }

    #[tokio::test]
    async fn runner_retries_empty_responses_and_keeps_usage_on_recovery() {
        let provider = Arc::new(EmptyResponseProvider {
            calls: std::sync::atomic::AtomicUsize::new(0),
            empty_attempts: 2,
        });
        let runner =
            InferenceRunner::<(), TestEffect>::new(provider.clone(), ModelConfig::new("model"));
        let (emit, _events) = test_emitter();
        let conversation = Conversation::new_unvalidated([Message::user().with_text("hi")]);
        let step = applied_result(
            runner
                .infer(&(), &conversation, InferenceInput::default(), &emit)
                .await
                .unwrap(),
        );
        assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 3);
        assert!(!step.yield_to_client);
        assert!(matches!(
            &step.effects[..],
            [TestEffect::Usage(_), TestEffect::Usage(_), TestEffect::Usage(_), TestEffect::Message(message)]
                if message.as_concat_text() == "recovered" && !is_empty_response_marker(message)
        ));
    }

    #[tokio::test]
    async fn runner_exhausts_empty_response_retries_with_a_hidden_marker() {
        let provider = Arc::new(EmptyResponseProvider {
            calls: std::sync::atomic::AtomicUsize::new(0),
            empty_attempts: usize::MAX,
        });
        let runner =
            InferenceRunner::<(), TestEffect>::new(provider.clone(), ModelConfig::new("model"));
        let (emit, mut events) = test_emitter();
        let conversation = Conversation::new_unvalidated([Message::user().with_text("hi")]);
        let step = applied_result(
            runner
                .infer(&(), &conversation, InferenceInput::default(), &emit)
                .await
                .unwrap(),
        );
        assert_eq!(
            provider.calls.load(std::sync::atomic::Ordering::SeqCst),
            MAX_EMPTY_RESPONSE_RETRIES + 1
        );
        assert!(!step.yield_to_client);
        assert_eq!(
            step.effects
                .iter()
                .filter(|effect| matches!(effect, TestEffect::Usage(_)))
                .count(),
            MAX_EMPTY_RESPONSE_RETRIES + 1
        );
        assert!(matches!(
            step.effects.last(),
            Some(TestEffect::Message(message))
                if is_empty_response_marker(message)
                    && !message.is_user_visible()
                    && !message.is_agent_visible()
                    && message.as_concat_text() == EMPTY_RESPONSE_MESSAGE
        ));
        while let Ok(event) = events.try_recv() {
            if let crate::events::AgentEvent::Message(message) = event {
                assert!(!is_empty_response_marker(&message));
            }
        }
    }

    #[tokio::test]
    async fn runner_does_not_retry_empty_successful_tool_completion() {
        let provider = Arc::new(EmptyResponseProvider {
            calls: std::sync::atomic::AtomicUsize::new(0),
            empty_attempts: usize::MAX,
        });
        let runner =
            InferenceRunner::<(), TestEffect>::new(provider.clone(), ModelConfig::new("model"));
        let (emit, _events) = test_emitter();
        let conversation = Conversation::new_unvalidated([
            Message::user().with_text("hi"),
            Message::assistant()
                .with_tool_request("done", Ok(rmcp::model::CallToolRequestParams::new("tool"))),
            Message::user()
                .with_tool_response("done", Ok(rmcp::model::CallToolResult::success(vec![]))),
        ]);
        let step = applied_result(
            runner
                .infer(&(), &conversation, InferenceInput::default(), &emit)
                .await
                .unwrap(),
        );
        assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(matches!(
            &step.effects[..],
            [TestEffect::Usage(_), TestEffect::Message(message)]
                if message.content.is_empty() && !message.is_user_visible() && message.is_agent_visible()
        ));
    }

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
    fn cancellation_response_includes_requests_from_unconverted_messages() {
        let persisted = [Message::user().with_text("run it")];
        let pending = [Message::assistant().with_tool_request(
            "pending-call",
            Ok(rmcp::model::CallToolRequestParams::new("tool")),
        )];

        let response = cancellation_response(&persisted, &pending).expect("cancellation response");

        assert_eq!(
            response.get_tool_response_ids(),
            std::collections::HashSet::from(["pending-call"])
        );
        let cancellation_text = response
            .content
            .iter()
            .filter_map(MessageContent::as_tool_response)
            .flat_map(|response| {
                response
                    .tool_result
                    .as_ref()
                    .expect("tool result")
                    .content
                    .iter()
            })
            .filter_map(|content| content.as_text())
            .map(|text| text.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(cancellation_text, vec![CANCELLED_TOOL_RESPONSE]);
    }

    #[test]
    fn cancellation_response_skips_answered_requests() {
        let request = Message::assistant().with_tool_request(
            "answered-call",
            Ok(rmcp::model::CallToolRequestParams::new("tool")),
        );
        let response = Message::user().with_tool_response(
            "answered-call",
            Ok(rmcp::model::CallToolResult::success(vec![])),
        );

        assert!(cancellation_response(&[request, response], &[]).is_none());
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
