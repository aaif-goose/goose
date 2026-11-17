use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use goose_agent::inference::{InferenceEffect, InferenceRunner};
use goose_agent::machine::MachineSession;
use goose_agent::operation::{Emitter, Inference, InferenceInput, OperationResult};
use goose_agent::tool_call_state::UNCLAIMED_TOOL_ERROR;
use goose_provider_types::base::{MessageStream, Provider};
use goose_provider_types::conversation::message::{
    Message, MessageContent, TOOL_META_EXTERNAL_DISPATCH_KEY,
};
use goose_provider_types::conversation::token_usage::ProviderUsage;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::errors::ProviderError;
use goose_provider_types::model::ModelConfig;
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
    sent: Mutex<Vec<Message>>,
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
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        *self.sent.lock().unwrap() = messages.to_vec();
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
    infer_after(vec![Message::user().with_text("hello")], replies, offered)
        .await
        .0
}

async fn infer_after(
    history: Vec<Message>,
    replies: Vec<Result<Message, ProviderError>>,
    offered: &[&'static str],
) -> (Vec<Message>, Vec<Message>) {
    let provider = Arc::new(ReplyProvider {
        replies: Mutex::new(replies),
        sent: Mutex::new(Vec::new()),
    });
    let runner =
        InferenceRunner::<Session, Effect>::new(provider.clone(), ModelConfig::new("test"));
    let session = Session(Conversation::new_unvalidated(history));
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
    let messages = result
        .effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Message(message) => Some(*message),
            Effect::Usage => None,
        })
        .collect();
    let sent = provider.sent.lock().unwrap().clone();
    (messages, sent)
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
            "Tool 'b' is not available.".to_string()
        )]
    );
    let unclaimed = messages[1]
        .content
        .iter()
        .filter_map(MessageContent::as_tool_response)
        .all(|response| {
            response
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get(UNCLAIMED_TOOL_ERROR))
                == Some(&json!(true))
        });
    assert!(unclaimed);
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
            "Tool 'b' is not available.".to_string()
        )]
    );
    assert!(messages[2].error_kind().is_some());
}

#[tokio::test]
async fn lists_the_offered_tools_next_to_unavailable_tool_errors() {
    let reply =
        Message::assistant().with_tool_request("not_offered", Ok(CallToolRequestParams::new("b")));
    let mut history = infer(vec![Ok(reply)], &["a"]).await;
    history.insert(0, Message::user().with_text("hello"));

    let (_, sent) = infer_after(
        history,
        vec![Ok(Message::assistant().with_text("ok"))],
        &["c", "a"],
    )
    .await;

    let response = sent
        .iter()
        .flat_map(|message| &message.content)
        .find_map(MessageContent::as_tool_response)
        .unwrap();
    let text: Vec<_> = response
        .tool_result
        .as_ref()
        .unwrap()
        .content
        .iter()
        .map(|content| content.as_text().unwrap().text.as_str())
        .collect();
    assert_eq!(
        text,
        ["Tool 'b' is not available.", "Available tools: [a, c]."]
    );
    assert!(!response
        .metadata
        .as_ref()
        .is_some_and(|metadata| metadata.contains_key(UNCLAIMED_TOOL_ERROR)));
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
