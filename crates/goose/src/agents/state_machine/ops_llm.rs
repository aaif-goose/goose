//! Goose integration for the reusable inference operation.

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use goose_agent::inference::InferenceEffect;
pub use goose_agent::inference::InferenceRunner;
use goose_providers::base::{MessageStream, ModelInfo, Provider};
use goose_providers::conversation::message::Message;
use goose_providers::conversation::token_usage::ProviderUsage;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;

use crate::agents::extension_manager::{get_tool_owner, recover_mangled_tool_name};
use crate::agents::state_machine::GooseEffect;

pub(super) use goose_agent::inference::{chat_span, record_chat_usage};

pub(super) const ADVERTISED_TOOLS_NOTE: &str = "advertised_tools";
pub(super) const LLM_OPERATION_NAME: &str = "llm";

pub struct GooseInferenceProvider {
    inner: Arc<dyn Provider>,
}

impl GooseInferenceProvider {
    pub fn new(inner: Arc<dyn Provider>) -> Self {
        Self { inner }
    }
}

impl InferenceEffect for GooseEffect {
    fn record_usage(usage: ProviderUsage) -> Self {
        GooseEffect::RecordUsage(usage)
    }
}

fn enrich_unclaimed_tool_errors(messages: &[Message], tools: &[rmcp::model::Tool]) -> Vec<Message> {
    let mut available_tools = tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect::<Vec<_>>();
    available_tools.sort_unstable();
    available_tools.dedup();
    let available_tools = available_tools.join(", ");
    let mut messages = messages.to_vec();
    for message in &mut messages {
        for content in &mut message.content {
            let goose_providers::conversation::message::MessageContent::ToolResponse(response) =
                content
            else {
                continue;
            };
            let Some(metadata) = &mut response.metadata else {
                continue;
            };
            if metadata
                .remove(super::ops_unknown_tool::UNCLAIMED_TOOL_ERROR)
                .is_none()
            {
                continue;
            }
            let Ok(result) = &mut response.tool_result else {
                continue;
            };
            result.content.push(rmcp::model::ContentBlock::text(format!(
                "Available tools: [{available_tools}]."
            )));
        }
    }
    messages
}

fn prepare_tool_requests(message: &mut Message, advertised_tools: &[rmcp::model::Tool]) {
    let tool_owners = advertised_tools
        .iter()
        .map(|tool| (tool.name.as_ref(), get_tool_owner(tool)))
        .collect::<Vec<_>>();

    for content in &mut message.content {
        let goose_providers::conversation::message::MessageContent::ToolRequest(request) = content
        else {
            continue;
        };
        let Ok(tool_call) = &mut request.tool_call else {
            continue;
        };
        if !advertised_tools
            .iter()
            .any(|tool| tool.name == tool_call.name)
        {
            if let Some(recovered) = recover_mangled_tool_name(
                &tool_call.name,
                tool_owners
                    .iter()
                    .map(|(name, owner)| (*name, owner.as_deref())),
            ) {
                tool_call.name = recovered.into();
            }
        }

        let Some(tool) = advertised_tools
            .iter()
            .find(|tool| tool.name == tool_call.name)
        else {
            continue;
        };
        let schema = serde_json::Value::Object(tool.input_schema.as_ref().clone());
        tool_call.arguments =
            crate::agents::reply_parts::coerce_tool_arguments(tool_call.arguments.clone(), &schema);

        let Some(meta) = &tool.meta else {
            continue;
        };
        let Ok(serde_json::Value::Object(meta)) = serde_json::to_value(meta) else {
            continue;
        };
        match request.tool_meta.as_mut() {
            Some(serde_json::Value::Object(existing)) => {
                for (key, value) in meta {
                    existing.entry(key).or_insert(value);
                }
            }
            None => request.tool_meta = Some(serde_json::Value::Object(meta)),
            Some(_) => {}
        }
    }
}

#[async_trait]
impl Provider for GooseInferenceProvider {
    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    fn provider_session_id(&self) -> Option<String> {
        self.inner.provider_session_id()
    }

    async fn resume(&self, session_id: &str) -> Result<(), ProviderError> {
        self.inner.resume(session_id).await
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[rmcp::model::Tool],
    ) -> Result<MessageStream, ProviderError> {
        let messages = enrich_unclaimed_tool_errors(messages, tools);
        let (tools, toolshim_tools, system_prompt) =
            crate::agents::reply_parts::prepare_tools_for_provider(
                tools.to_vec(),
                system.to_string(),
                model_config,
            );
        let advertised_tools = tools
            .iter()
            .chain(toolshim_tools.iter())
            .cloned()
            .collect::<Vec<_>>();
        let mut advertised_tool_names = advertised_tools
            .iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();
        advertised_tool_names.sort_unstable();
        advertised_tool_names.dedup();
        let advertised_tools_note = serde_json::Value::Array(
            advertised_tool_names
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );
        let session_id = crate::session_context::current_session_id().unwrap_or_default();
        let stream = crate::agents::reply_parts::stream_response_from_provider(
            self.inner.clone(),
            model_config.clone(),
            &session_id,
            &system_prompt,
            &messages,
            &tools,
            &toolshim_tools,
        )
        .await?;
        Ok(Box::pin(stream.map(move |result| {
            result.map(|(message, usage)| {
                let message = message.map(|mut message| {
                    prepare_tool_requests(&mut message, &advertised_tools);
                    if message.role == rmcp::model::Role::Assistant {
                        message.metadata.set_operation_note(
                            LLM_OPERATION_NAME,
                            ADVERTISED_TOOLS_NOTE,
                            advertised_tools_note.clone(),
                        );
                    }
                    message
                });
                (message, usage)
            })
        })))
    }

    async fn get_context_limit(&self, model: &str, override_limit: Option<usize>) -> usize {
        self.inner.get_context_limit(model, override_limit).await
    }

    async fn fetch_model_info(&self, model_name: &str) -> Result<ModelInfo, ProviderError> {
        self.inner.fetch_model_info(model_name).await
    }
}

#[cfg(test)]
mod canonicalization_tests {
    use super::*;
    use rmcp::{model::CallToolRequestParams, object};

    fn request(name: &str) -> Message {
        Message::assistant()
            .with_tool_request("request", Ok(CallToolRequestParams::new(name.to_string())))
    }

    fn tool_request(message: &Message) -> &goose_providers::conversation::message::ToolRequest {
        message.content[0].as_tool_request().unwrap()
    }

    fn tool_name(message: &Message) -> &str {
        tool_request(message)
            .tool_call
            .as_ref()
            .unwrap()
            .name
            .as_ref()
    }

    #[test]
    fn canonicalizes_mangled_names_against_advertised_tools() {
        let advertised = vec![rmcp::model::Tool::new(
            "developer__shell",
            "run a shell command",
            object!({ "type": "object" }),
        )];
        let mut message = request("developer.shell");

        prepare_tool_requests(&mut message, &advertised);

        assert_eq!(tool_name(&message), "developer__shell");
    }

    #[test]
    fn canonicalizes_owner_qualified_unprefixed_tool_aliases() {
        let advertised = vec![rmcp::model::Tool::new(
            "shell",
            "run a shell command",
            object!({ "type": "object" }),
        )
        .with_meta(rmcp::model::MetaObject(object!({
            "goose_extension": "developer"
        })))];
        let mut message = request("developer.shell");

        prepare_tool_requests(&mut message, &advertised);

        assert_eq!(tool_name(&message), "shell");

        let mut message = request("developer__shell");

        prepare_tool_requests(&mut message, &advertised);

        assert_eq!(tool_name(&message), "shell");
    }

    #[test]
    fn leaves_unrecoverable_names_unmodified() {
        let advertised = vec![rmcp::model::Tool::new(
            "developer__shell",
            "run a shell command",
            object!({ "type": "object" }),
        )];
        let mut message = request("developer.shell!");

        prepare_tool_requests(&mut message, &advertised);

        assert_eq!(tool_name(&message), "developer.shell!");
    }

    #[test]
    fn coerces_arguments_and_merges_tool_metadata() {
        use crate::conversation::message::TOOL_META_EXTERNAL_DISPATCH_KEY;

        let advertised = vec![rmcp::model::Tool::new(
            "set_enabled",
            "set a flag",
            object!({
                "type": "object",
                "properties": {
                    "count": { "type": "integer" },
                    "enabled": { "type": "boolean" }
                }
            }),
        )
        .with_meta(rmcp::model::MetaObject(object!({
            "ui": { "visibility": ["model"] }
        })))];
        let mut message = Message::assistant().with_tool_request_with_metadata(
            "request",
            Ok(
                CallToolRequestParams::new("set_enabled").with_arguments(object!({
                    "count": "4",
                    "enabled": "true"
                })),
            ),
            None,
            Some(serde_json::json!({ TOOL_META_EXTERNAL_DISPATCH_KEY: true })),
        );

        prepare_tool_requests(&mut message, &advertised);

        let request = tool_request(&message);
        let arguments = request
            .tool_call
            .as_ref()
            .unwrap()
            .arguments
            .as_ref()
            .unwrap();
        assert_eq!(arguments["count"], 4);
        assert_eq!(arguments["enabled"], true);
        assert!(request.was_executed_externally());
        assert_eq!(
            request.tool_meta.as_ref().unwrap()["ui"]["visibility"][0],
            "model"
        );
    }

    #[test]
    fn preserves_arguments_for_map_schemas() {
        let advertised = vec![rmcp::model::Tool::new(
            "add_values",
            "add named values",
            object!({
                "type": "object",
                "additionalProperties": { "type": "integer" }
            }),
        )];
        let mut message = Message::assistant().with_tool_request(
            "request",
            Ok(CallToolRequestParams::new("add_values")
                .with_arguments(object!({ "left": 2, "right": 3 }))),
        );

        prepare_tool_requests(&mut message, &advertised);

        assert_eq!(
            tool_request(&message)
                .tool_call
                .as_ref()
                .unwrap()
                .arguments
                .as_ref()
                .unwrap(),
            &object!({ "left": 2, "right": 3 })
        );
    }
}
