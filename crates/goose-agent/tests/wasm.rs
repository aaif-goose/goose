#![cfg(target_arch = "wasm32")]
// The GDK takes `Arc` on every target, so native and wasm32 share one API.
#![allow(clippy::arc_with_non_send_sync)]

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    inference::{InferenceEffect, InferenceRunner},
    machine::{EffectHandler, MachineSession, SessionLoader, StateMachine, Step},
    operation::{Emitter, MachineEffect},
    tool::ToolOperation,
};
use goose_provider_types::{
    base::{MessageStream, Provider},
    conversation::{
        message::{Message, MessageContent},
        token_usage::{ProviderUsage, Usage},
        Conversation,
    },
    errors::ProviderError,
    model::ModelConfig,
    retry::{ProviderRetry, RetryConfig},
};
use rmcp::{
    handler::server::router::tool::{AsyncTool, SyncTool, ToolBase},
    model::{CallToolRequestParams, ErrorData, Tool},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;

/// Awaits a JavaScript promise, as a host call would. The future is not `Send`,
/// so everything that awaits it must accept non-`Send` futures.
async fn host_call() {
    JsFuture::from(js_sys::Promise::resolve(&js_sys::Object::new()))
        .await
        .unwrap();
}

#[derive(Default, Deserialize, JsonSchema)]
struct AddInput {
    left: u64,
    right: u64,
}

#[derive(Serialize, JsonSchema)]
struct AddOutput {
    sum: u64,
}

struct Add;

impl ToolBase for Add {
    type Parameter = AddInput;
    type Output = AddOutput;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "add".into()
    }
}

impl SyncTool<Session> for Add {
    fn invoke(_session: &Session, input: AddInput) -> Result<AddOutput, ErrorData> {
        Ok(AddOutput {
            sum: input.left + input.right,
        })
    }
}

#[derive(Default, Deserialize, JsonSchema)]
struct GreetInput {
    name: String,
}

#[derive(Serialize, JsonSchema)]
struct GreetOutput {
    greeting: String,
}

struct Greet;

impl ToolBase for Greet {
    type Parameter = GreetInput;
    type Output = GreetOutput;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "greet".into()
    }
}

impl AsyncTool<Session> for Greet {
    async fn invoke(_session: &Session, input: GreetInput) -> Result<GreetOutput, ErrorData> {
        host_call().await;
        Ok(GreetOutput {
            greeting: format!("Hello, {}!", input.name),
        })
    }
}

/// Requests one tool call, then answers with text once the tool has responded.
struct ScriptedProvider {
    call: CallToolRequestParams,
}

#[async_trait(?Send)]
impl Provider for ScriptedProvider {
    fn get_name(&self) -> &str {
        "scripted"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        host_call().await;
        let reply = if messages.last().is_some_and(Message::is_tool_response) {
            Message::assistant().with_text("done")
        } else {
            Message::assistant().with_tool_request("call-1", Ok(self.call.clone()))
        };
        let usage = ProviderUsage::new("scripted-model".to_string(), Usage::default());
        Ok(Box::pin(futures::stream::once(async move {
            host_call().await;
            Ok((Some(reply), Some(usage)))
        })))
    }
}

enum Effect {
    Message(Message),
    Usage,
}

impl From<Message> for Effect {
    fn from(message: Message) -> Self {
        Effect::Message(message)
    }
}

impl InferenceEffect for Effect {
    fn record_usage(_usage: ProviderUsage) -> Self {
        Effect::Usage
    }
}

impl MachineEffect for Effect {
    fn ensure_message_ids(&mut self) {}
}

#[derive(Clone)]
struct Session {
    conversation: Conversation,
}

impl MachineSession for Session {
    fn id(&self) -> &str {
        "wasm"
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.conversation)
    }
}

/// Stands in for host storage, which is not `Send` or `Sync` on wasm32.
struct Runtime {
    conversation: Rc<RefCell<Conversation>>,
}

#[async_trait(?Send)]
impl SessionLoader<Session> for Runtime {
    async fn load(&self, _session_id: &str) -> Result<Session> {
        host_call().await;
        Ok(Session {
            conversation: self.conversation.borrow().clone(),
        })
    }
}

#[async_trait(?Send)]
impl EffectHandler<Session, Effect> for Runtime {
    async fn apply_effects(
        &self,
        _session: &Session,
        effects: &mut [Effect],
        _emit: &Emitter,
    ) -> Result<()> {
        host_call().await;
        let mut conversation = self.conversation.borrow_mut();
        for effect in effects {
            if let Effect::Message(message) = effect {
                conversation.push(message.clone());
            }
        }
        Ok(())
    }
}

async fn run_turn(call: CallToolRequestParams) -> Conversation {
    let cancel = CancellationToken::new();
    let tools = ToolOperation::new()
        .with_sync_tool::<Add>()
        .with_async_tool::<Greet>();
    let inference = InferenceRunner::<Session, Effect>::new(
        Arc::new(ScriptedProvider { call }),
        ModelConfig::new("scripted-model"),
    );
    let machine = StateMachine::new(
        vec![
            Step::Operation(Arc::new(tools)),
            Step::Inference(Arc::new(inference)),
        ],
        cancel.clone(),
    );
    let runtime = Runtime {
        conversation: Rc::new(RefCell::new(Conversation::new_unvalidated([
            Message::user().with_text("use the tool"),
        ]))),
    };
    let (tx, _rx) = mpsc::channel(16);
    let emit = Emitter::new(tx, cancel);

    machine
        .run(&runtime, "wasm", &emit)
        .await
        .unwrap()
        .conversation
}

fn tool_output(conversation: &Conversation) -> serde_json::Value {
    let messages = conversation.messages();
    assert_eq!(messages.len(), 4);
    let MessageContent::Text(reply) = &messages[3].content[0] else {
        panic!("turn should end with the provider's text reply");
    };
    assert_eq!(reply.text, "done");
    messages[2].content[0]
        .as_tool_response()
        .unwrap()
        .tool_result
        .as_ref()
        .unwrap()
        .structured_content
        .clone()
        .unwrap()
}

#[wasm_bindgen_test]
async fn runs_a_turn_with_an_async_tool() {
    let conversation = run_turn(
        CallToolRequestParams::new("greet")
            .with_arguments(serde_json::from_value(json!({"name": "Goose"})).unwrap()),
    )
    .await;

    assert_eq!(
        tool_output(&conversation),
        json!({"greeting": "Hello, Goose!"})
    );
}

#[wasm_bindgen_test]
async fn runs_a_turn_with_a_sync_tool() {
    let conversation = run_turn(
        CallToolRequestParams::new("add")
            .with_arguments(serde_json::from_value(json!({"left": 2, "right": 3})).unwrap()),
    )
    .await;

    assert_eq!(tool_output(&conversation), json!({"sum": 5}));
}

#[wasm_bindgen_test]
async fn retries_back_off_on_the_host_timer() {
    let provider = ScriptedProvider {
        call: CallToolRequestParams::new("add"),
    };
    let attempts = &Cell::new(0);
    let started = js_sys::Date::now();

    let result = provider
        .with_retry_config(
            move || async move {
                attempts.set(attempts.get() + 1);
                host_call().await;
                if attempts.get() == 1 {
                    Err(ProviderError::ServerError("unavailable".to_string()))
                } else {
                    Ok(())
                }
            },
            RetryConfig::new(1, 50, 1.0, 50),
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(attempts.get(), 2);
    // The delay is jittered down to 80% of the 50ms interval at most.
    assert!(js_sys::Date::now() - started >= 40.0);
}
