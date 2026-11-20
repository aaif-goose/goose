use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use oi_kit_agent::inference::{InferenceEffect, InferenceRunner};
use oi_kit_agent::machine::MachineSession;
use oi_kit_agent::operation::{Emitter, Inference, InferenceInput, OperationResult};
use oi_kit_provider_types::base::{MessageStream, Provider};
use oi_kit_provider_types::conversation::message::{
    Message, MessageContent, TOOL_META_EXTERNAL_DISPATCH_KEY,
};
use oi_kit_provider_types::conversation::token_usage::ProviderUsage;
use oi_kit_provider_types::conversation::Conversation;
use oi_kit_provider_types::errors::ProviderError;
use oi_kit_provider_types::model::ModelConfig;
use rmcp::model::{CallToolRequestParams, ErrorCode, ErrorData, Role, Tool};
use serde_json::json;
use tokio_util::sync::CancellationToken;

struct Session(Conversation);

impl MachineSession for Session {
    fn id(&self) -> &str {
        "unadvertised-tools-test"
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.0)
    }
}

enum Effect {
    Message(Box<Message>),
    Usage,
}

impl From<Message> for Effect {
    fn from(message: Message) -> Self {
        Self::Message(Box::new(message))
    }
}

impl InferenceEffect for Effect {
    fn record_usage(_usage: ProviderUsage) -> Self {
        Self::Usage
    }
}

struct ReplyProvider {
    replies: Mutex<Vec<Result<Message, ProviderError>>>,
}

#[async_trait]
impl Provider for ReplyProvider {
    fn get_name(&self) -> &str {
        "unadvertised-tools-test"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let replies = std::mem::take(&mut *self.replies.lock().unwrap());
        Ok(Box::pin(futures::stream::iter(
            replies
                .into_iter()
                .map(|reply| reply.map(|message| (Some(message), None))),
        )))
    }
}

fn tool(name: &'static str) -> Tool {
    Tool::new(name, "", serde_json::Map::new())
}

async fn infer(
    replies: Vec<Result<Message, ProviderError>>,
    offered: &[&'static str],
) -> Vec<Message> {
    let provider = Arc::new(ReplyProvider {
        replies: Mutex::new(replies),
    });
    let runner = InferenceRunner::<Session, Effect>::new(provider, ModelConfig::new("test"));
    let session = Session(Conversation::new_unvalidated(vec![
        Message::user().with_text("hello")
    ]));
    let input = InferenceInput {
        tools: offered.iter().copied().map(tool).collect(),
        ..InferenceInput::default()
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let OperationResult::Applied(result) = runner
        .infer(&session, &session.0, input, &emit)
        .await
        .unwrap()
    else {
        panic!("inference did not apply");
    };
    result
        .effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Message(message) => Some(*message),
            Effect::Usage => None,
        })
        .collect()
}

fn tool_errors(messages: &[Message]) -> Vec<(String, String)> {
    messages
        .iter()
        .filter(|message| message.role == Role::User)
        .flat_map(|message| &message.content)
        .filter_map(MessageContent::as_tool_response)
        .map(|response| {
            let result = response.tool_result.as_ref().unwrap();
            assert_eq!(result.is_error, Some(true));
            assert_eq!(result.content.len(), 1);
            (
                response.id.clone(),
                result.content[0].as_text().unwrap().text.clone(),
            )
        })
        .collect()
}

#[tokio::test]
async fn answers_calls_to_tools_that_were_not_offered() {
    let reply = Message::assistant()
        .with_tool_request("offered", Ok(CallToolRequestParams::new("a")))
        .with_tool_request("not_offered", Ok(CallToolRequestParams::new("b")))
        .with_tool_request_with_metadata(
            "external",
            Ok(CallToolRequestParams::new("provider_tool")),
            None,
            Some(json!({ TOOL_META_EXTERNAL_DISPATCH_KEY: true })),
        )
        .with_tool_request(
            "unparsed",
            Err(ErrorData::new(ErrorCode::INVALID_PARAMS, "bad json", None)),
        );

    let messages = infer(vec![Ok(reply)], &["a"]).await;

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, Role::Assistant);
    assert_eq!(
        tool_errors(&messages),
        vec![(
            "not_offered".to_string(),
            "Tool 'b' was not available in this request. Available tools: [a].".to_string()
        )]
    );
}

#[tokio::test]
async fn answers_calls_to_tools_that_were_not_offered_when_the_stream_fails() {
    let reply =
        Message::assistant().with_tool_request("not_offered", Ok(CallToolRequestParams::new("b")));
    let failure = ProviderError::NetworkError("connection dropped".to_string());

    let messages = infer(vec![Ok(reply), Err(failure)], &["a"]).await;

    assert_eq!(messages.len(), 3);
    assert_eq!(
        tool_errors(&messages),
        vec![(
            "not_offered".to_string(),
            "Tool 'b' was not available in this request. Available tools: [a].".to_string()
        )]
    );
    assert!(messages[2].error_kind().is_some());
}

#[tokio::test]
async fn lists_the_tools_offered_to_the_inference_that_made_the_call() {
    let reply =
        Message::assistant().with_tool_request("not_offered", Ok(CallToolRequestParams::new("b")));

    let messages = infer(vec![Ok(reply)], &["c", "a", "c"]).await;

    assert_eq!(
        tool_errors(&messages),
        vec![(
            "not_offered".to_string(),
            "Tool 'b' was not available in this request. Available tools: [a, c].".to_string()
        )]
    );
}

#[tokio::test]
async fn leaves_replies_alone_when_every_tool_was_offered() {
    let reply = Message::assistant()
        .with_tool_request("first", Ok(CallToolRequestParams::new("a")))
        .with_tool_request("second", Ok(CallToolRequestParams::new("b")));

    let messages = infer(vec![Ok(reply)], &["a", "b"]).await;

    assert_eq!(messages.len(), 1);
    assert!(tool_errors(&messages).is_empty());
}
