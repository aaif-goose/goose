//! ACP inference without standard-provider request normalization.

use crate::agents::extension_manager::{ExtensionLease, ExtensionManager};
use std::sync::{Arc, Mutex as StdMutex};

use super::ops_llm::record_chat_usage;
use super::{
    applied, messages_since_kickoff, not_applicable, trailing_error, yielded_with, Emitter,
    GooseEffect, Inference, InferenceInput, Operation, OperationResult,
};
use crate::acp::AcpProvider;
use crate::agents::latest_provider_session_id;
use crate::conversation::message::{InferenceMetadata, Message, MessageContent};
use crate::conversation::{effective_role, Conversation, EffectiveRole};
use crate::session::Session;
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;

pub struct AcpInferenceRunner {
    provider: Arc<AcpProvider>,
    model_config: ModelConfig,
    extension_manager: Arc<ExtensionManager>,
    extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
}

const EMPTY_RESPONSE_MESSAGE: &str =
    "The model returned an empty response. Please resend your message to continue.";
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

fn is_empty_response(message: &Message) -> bool {
    message.content.iter().all(|content| match content {
        MessageContent::Text(text) => text.text.trim().is_empty(),
        MessageContent::Thinking(thinking) => {
            thinking.thinking.trim().is_empty() && thinking.signature.is_empty()
        }
        _ => false,
    })
}

fn ends_with_successful_tool_response(messages: &[Message]) -> bool {
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

fn messages_for_acp(conversation: &Conversation, keep_empty_messages: bool) -> Vec<Message> {
    conversation
        .messages()
        .iter()
        .filter(|message| message.is_agent_visible())
        .map(Message::agent_visible_content)
        .filter(|message| keep_empty_messages || !message.content.is_empty())
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

fn should_infer(conversation: &Conversation) -> bool {
    let projected = messages_for_acp(conversation, true);
    if projected
        .last()
        .is_some_and(|message| message.content.is_empty())
    {
        return false;
    }
    let messages = messages_for_acp(conversation, false);
    if messages.last().is_some_and(|message| {
        !message.content.is_empty()
            && message.content.iter().all(|content| {
                content.as_tool_response().is_some_and(|response| {
                    conversation
                        .messages()
                        .iter()
                        .flat_map(|message| &message.content)
                        .filter_map(MessageContent::as_tool_request)
                        .any(|request| {
                            request.id == response.id && request.was_executed_externally()
                        })
                })
            })
    }) {
        return false;
    }
    ends_with_provider_turn(&messages)
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

impl AcpInferenceRunner {
    pub fn new(
        provider: Arc<AcpProvider>,
        model_config: ModelConfig,
        extension_manager: Arc<ExtensionManager>,
        extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    ) -> Self {
        Self {
            provider,
            model_config,
            extension_manager,
            extension_lease,
        }
    }

    async fn error_outcome(&self, err: &ProviderError, emit: &Emitter) -> Vec<GooseEffect> {
        tracing::Span::current().record("error.type", err.telemetry_type());
        tracing::error!("LLM provider error: {err}");
        let message = Message::from_provider_error(err);
        let message = emit.message(message).await;
        vec![GooseEffect::from(message)]
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for AcpInferenceRunner {
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
        let OperationResult::NotApplicable = result else {
            return Ok(result);
        };
        let Some(response) = cancellation_response(messages_since_kickoff(conversation)?, &[])
        else {
            return Ok(OperationResult::NotApplicable);
        };
        let response = emit.message(response).await;
        applied([GooseEffect::from(response)])
    }
}

#[async_trait]
impl Inference<Session, GooseEffect> for AcpInferenceRunner {
    fn applies(&self, conversation: &Conversation) -> bool {
        let Ok(_) = messages_since_kickoff(conversation) else {
            return false;
        };
        trailing_error(conversation).is_none() && should_infer(conversation)
    }

    async fn prepare_session(&self, session: &Session) -> Result<Option<Session>> {
        let (session, lease) = self
            .extension_manager
            .current_session_snapshot(session)
            .await;
        let awaiting = match &session.conversation {
            Some(conversation) => {
                super::awaits_tool_responses(messages_since_kickoff(conversation)?)
            }
            None => false,
        };
        if !awaiting {
            *self
                .extension_lease
                .lock()
                .expect("extension lease unavailable") = Some(Arc::new(lease));
        }
        Ok(Some(session))
    }

    async fn infer(
        &self,
        session: &Session,
        conversation: &Conversation,
        _input: InferenceInput,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        if trailing_error(conversation).is_some() {
            return not_applicable();
        }

        if !should_infer(conversation) {
            return not_applicable();
        }
        let messages_for_provider = messages_for_acp(conversation, false);

        {
            let mut usage_effects: Vec<GooseEffect> = Vec::new();

            let provider_name = self.provider.name();
            if let Some(session_id) =
                latest_provider_session_id(conversation.messages(), provider_name)
            {
                let resumed = tokio::select! {
                    biased;
                    _ = emit.cancelled() => {
                        return self.cancel(session, conversation, OperationResult::NotApplicable, emit).await;
                    },
                    resumed = self.provider.resume(session_id) => resumed,
                };
                if let Err(error) = resumed {
                    tracing::warn!(
                        provider = provider_name,
                        %error,
                        "Could not resume provider session; continuing with a handoff"
                    );
                }
            }

            let stream = tokio::select! {
                biased;
                _ = emit.cancelled() => {
                    return self.cancel(session, conversation, OperationResult::NotApplicable, emit).await;
                },
                stream = crate::agents::reply_parts::stream_response_from_acp(
                    self.provider.clone(), self.model_config.clone(), &session.id, &messages_for_provider,
                ) => stream,
            };

            let mut stream = match stream {
                Ok(stream) => stream,
                Err(err) => {
                    usage_effects.extend(self.error_outcome(&err, emit).await);
                    return applied(usage_effects);
                }
            };

            let requested_model = self.model_config.model_name.clone();
            let resolved_model = None;
            let provider_session_id = Some(self.provider.session_id());
            let inference = InferenceMetadata {
                provider: self.provider.name().to_string(),
                requested_model,
                resolved_model,
                provider_session_id,
            };

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
                                if let Some(usage) = provider_usage {
                                    usage_effects.push(GooseEffect::RecordUsage(usage));
                                }
                                usage_effects.extend(accumulator.into_iter().map(GooseEffect::from));
                                usage_effects.extend(self.error_outcome(&err, emit).await);
                                return applied(usage_effects);
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
                            if chunk.content.iter().any(|content| matches!(content, MessageContent::ActionRequired(_))) {
                                chunk.metadata.set_operation_note("llm", "acp_live_permission", serde_json::json!(true));
                            }
                            let chunk = emit.message(chunk).await;
                            accumulator.push(chunk);
                        }
                    }
                }
            }

            drop(stream);
            if let Some(usage) = provider_usage {
                usage_effects.push(GooseEffect::RecordUsage(usage));
            }

            if cancelled || emit.cancel_token().is_cancelled() {
                if let Some(response) = cancellation_response(messages, accumulator.messages()) {
                    let response = emit.message(response).await;
                    accumulator.push(response);
                }
            }

            if !cancelled
                && !emit.cancel_token().is_cancelled()
                && accumulator.last().is_some_and(Message::is_tool_response)
            {
                let message = Message::assistant()
                    .with_visibility(false, true)
                    .with_inference(inference.clone());
                let message = emit.message(message).await;
                // Conversation::push folds empty inference markers into prior assistant
                // chunks; this marker must remain a distinct completed turn after tools.
                accumulator.messages_mut().push(message);
            }

            let empty_response = !cancelled
                && !emit.cancel_token().is_cancelled()
                && !ends_with_successful_tool_response(conversation.messages())
                && !accumulator
                    .iter()
                    .any(|message| message.metadata.output_token_limit_reached)
                && accumulator.iter().all(is_empty_response);
            if empty_response {
                let message = Message::assistant().with_text(EMPTY_RESPONSE_MESSAGE);
                let message = emit.message(message).await;
                usage_effects.push(GooseEffect::from(message));
                return yielded_with(usage_effects);
            }

            if ends_with_successful_tool_response(conversation.messages())
                && !accumulator
                    .iter()
                    .any(|message| message.metadata.output_token_limit_reached)
                && accumulator.iter().all(is_empty_response)
            {
                let mut message = accumulator
                    .into_iter()
                    .last()
                    .unwrap_or_else(Message::assistant);
                message.content.clear();
                message.metadata.user_visible = false;
                message.metadata.agent_visible = true;
                let message = emit.message(message).await;
                usage_effects.push(GooseEffect::from(message));
            } else {
                usage_effects.extend(accumulator.into_iter().map(GooseEffect::from));
            }
            applied(usage_effects)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    #[test]
    fn readiness_stops_at_agent_visible_empty_tail() {
        let mut tail = Message::assistant();
        tail.metadata.user_visible = false;
        tail.metadata.agent_visible = true;
        let conversation =
            Conversation::new_unvalidated([Message::user().with_text("hello"), tail]);
        assert!(!should_infer(&conversation));
    }

    #[test]
    fn projection_preserves_original_roles_and_unanswered_tools() {
        let messages = [
            Message::user().with_text("first"),
            Message::user().with_text("second"),
            Message::assistant().with_tool_request(
                "unanswered",
                Ok(rmcp::model::CallToolRequestParams::new("tool")),
            ),
            Message::user().with_text("new turn"),
        ];
        let conversation = Conversation::new_unvalidated(messages.clone());
        assert_eq!(messages_for_acp(&conversation, false), messages.to_vec());
    }

    #[test]
    fn successful_external_tool_tail_does_not_prompt_again() {
        let mut request = Message::assistant().with_tool_request(
            "external",
            Ok(rmcp::model::CallToolRequestParams::new("tool")),
        );
        let MessageContent::ToolRequest(tool_request) = &mut request.content[0] else {
            panic!("expected tool request");
        };
        tool_request.tool_meta = Some(serde_json::json!({
            "goose.external_dispatch": true,
        }));
        let conversation = Conversation::new_unvalidated([
            Message::user().with_text("run it"),
            request,
            Message::user()
                .with_tool_response("external", Ok(rmcp::model::CallToolResult::success(vec![]))),
        ]);
        assert!(!should_infer(&conversation));
    }
}
