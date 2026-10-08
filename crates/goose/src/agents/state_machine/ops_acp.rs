//! ACP inference without standard-provider request normalization.

use crate::agents::extension_manager::{ExtensionLease, ExtensionManager};
use std::sync::{Arc, Mutex as StdMutex};

use super::{
    applied, messages_since_kickoff, not_applicable, trailing_error, Emitter, GooseEffect,
    Inference, InferenceInput, Operation, OperationResult,
};
use crate::acp::AcpProvider;
use crate::agents::latest_provider_session_id;
use crate::conversation::message::{InferenceMetadata, Message, MessageContent};
use crate::conversation::{effective_role, Conversation, EffectiveRole};
use crate::session::Session;
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use goose_providers::model::ModelConfig;

pub struct AcpInferenceRunner {
    provider: Arc<AcpProvider>,
    model_config: ModelConfig,
    extension_manager: Arc<ExtensionManager>,
    extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
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
        goose_agent::inference::cancel_inference(conversation, result, emit).await
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

    async fn prepare_input(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        _operations: &[&dyn Operation<Session, GooseEffect>],
        _cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<InferenceInput> {
        Ok(InferenceInput::default())
    }

    async fn infer(
        &self,
        session: &Session,
        conversation: &Conversation,
        _input: InferenceInput,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        messages_since_kickoff(conversation)?;
        if trailing_error(conversation).is_some() {
            return not_applicable();
        }

        if !should_infer(conversation) {
            return not_applicable();
        }
        let messages_for_provider = messages_for_acp(conversation, false);

        let provider_name = self.provider.name();
        if let Some(session_id) = latest_provider_session_id(conversation.messages(), provider_name)
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

        let stream = match stream {
            Ok(stream) => stream,
            Err(err) => {
                return applied(
                    goose_agent::inference::inference_error::<GooseEffect>(&err, emit).await,
                );
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

        let stream = Box::pin(stream.map(|result| {
            result.map(|(message, usage)| {
                let message = message.map(|mut message| {
                    if message
                        .content
                        .iter()
                        .any(|content| matches!(content, MessageContent::ActionRequired(_)))
                    {
                        message.metadata.set_operation_note(
                            "llm",
                            "acp_live_permission",
                            serde_json::json!(true),
                        );
                    }
                    message
                });
                (message, usage)
            })
        }));
        let mut response =
            goose_agent::inference::consume_inference_stream(stream, inference.clone(), emit).await;
        if !response.cancelled
            && response.error.is_none()
            && !emit.cancel_token().is_cancelled()
            && response
                .messages
                .last()
                .is_some_and(Message::is_tool_response)
        {
            let message = emit
                .message(
                    Message::assistant()
                        .with_visibility(false, true)
                        .with_inference(inference),
                )
                .await;
            // Empty inference markers must remain distinct from preceding tool messages.
            response.messages.messages_mut().push(message);
        }
        response.finish::<GooseEffect>(conversation, emit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
