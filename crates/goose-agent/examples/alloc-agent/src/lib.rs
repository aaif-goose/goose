#![no_std]
extern crate alloc;

use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use anyhow::Result;
use async_trait::async_trait;
use core::{
    cell::Cell,
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
    time::Duration,
};
use goose_agent::{
    events::AgentEvent,
    inference::{InferenceEffect, InferenceRunner},
    machine::{EffectHandler, MachineSession, SessionLoader, StateMachine, Step},
    operation::{ConversationEffect, Emitter, EventSink, MachineEffect},
    runtime::CancellationToken,
    tool::{LocalTool, ToolOperation},
    tool_approval::{ApprovalDecision, ToolApproval, ToolApprovalOperation},
};
use goose_provider_types::{
    base::{MessageStream, Provider},
    conversation::{message::Message, token_usage::ProviderUsage, Conversation},
    data::{CallToolResult, ErrorData, JsonObject, Tool},
    errors::ProviderError,
    formats::openai,
    model::ModelConfig,
    runtime::{BoxFuture, Cell as SharedCell, HostServices, Shared},
};

#[derive(Default)]
pub struct Host {
    ids: Cell<u64>,
}
impl HostServices for Host {
    fn unix_timestamp(&self) -> i64 {
        1_700_000_000
    }
    fn new_id(&self) -> String {
        let id = self.ids.get();
        self.ids.set(id + 1);
        format!("device-boot-1-{id}")
    }
    fn sleep(&self, _duration: Duration) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
    fn jitter_ms(&self, _upper_bound: u64) -> u64 {
        0
    }
}

/// A board integration replaces this object with its own network stack and framing decoder.
struct Network {
    requests: Cell<usize>,
}
impl Network {
    async fn request(&self, messages: &[Message]) -> Vec<Result<String>> {
        self.requests.set(self.requests.get() + 1);
        if messages.iter().any(Message::is_tool_response) {
            vec![
                Ok(r#"data: {"id":"reply-2","choices":[{"delta":{"content":"Temperature: "}}]}"#.into()),
                Ok(r#"data: {"id":"reply-2","choices":[{"delta":{"content":"21 C"},"finish_reason":"stop"}]}"#.into()),
            ]
        } else {
            vec![Ok(r#"data: {"id":"reply-1","choices":[{"delta":{"tool_calls":[{"index":0,"id":"sensor-1","type":"function","function":{"name":"read_temperature","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}"#.into())]
        }
    }
}
struct DeviceProvider {
    host: Shared<Host>,
    network: Network,
}
#[async_trait(?Send)]
impl Provider for DeviceProvider {
    fn get_name(&self) -> &str {
        "device"
    }
    async fn stream(
        &self,
        config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream<'_>, ProviderError> {
        let _payload = openai::create_request(
            config,
            system,
            messages,
            tools,
            &goose_provider_types::images::ImageFormat::OpenAi,
            false,
        )
        .map_err(|e| ProviderError::InvalidValue(e.to_string()))?;
        let frames = self.network.request(messages).await;
        let stream = openai::response_to_streaming_message_with_host(
            self.host.as_ref(),
            futures::stream::iter(frames),
        );
        use futures::StreamExt;
        Ok(Box::pin(stream.map(|item| {
            item.map_err(ProviderError::from_stream_error)
        })))
    }
}

struct AllowLocalTools;
#[async_trait(?Send)]
impl ToolApproval<Session> for AllowLocalTools {
    async fn check(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        requests: &[&goose_provider_types::conversation::message::ToolRequest],
    ) -> Result<Vec<ApprovalDecision>> {
        Ok(requests.iter().map(|_| ApprovalDecision::Allow).collect())
    }
}

struct Temperature {
    calls: Shared<Cell<usize>>,
}
#[async_trait(?Send)]
impl LocalTool<Session> for Temperature {
    fn definition(&self) -> Tool {
        Tool::new("read_temperature", "Read the local temperature sensor", Shared::new(serde_json::from_value(serde_json::json!({"type":"object", "properties":{}, "additionalProperties":false})).unwrap()))
    }
    async fn call(
        &self,
        _session: &Session,
        _arguments: JsonObject,
    ) -> Result<CallToolResult, ErrorData> {
        self.calls.set(self.calls.get() + 1);
        Ok(CallToolResult::structured(
            serde_json::json!({"celsius":21}),
        ))
    }
}

#[derive(Clone)]
pub struct Session {
    pub conversation: Conversation,
}
impl MachineSession for Session {
    fn id(&self) -> &str {
        "device-session"
    }
    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.conversation)
    }
}
enum Effect {
    Conversation(ConversationEffect),
    Usage(ProviderUsage),
}
impl From<Message> for Effect {
    fn from(message: Message) -> Self {
        Self::Conversation(message.into())
    }
}
impl From<ConversationEffect> for Effect {
    fn from(effect: ConversationEffect) -> Self {
        Self::Conversation(effect)
    }
}
impl MachineEffect for Effect {
    fn ensure_message_ids(&mut self, host: &dyn HostServices) {
        if let Self::Conversation(effect) = self {
            effect.ensure_message_ids(host);
        }
    }
}
impl InferenceEffect for Effect {
    fn record_usage(usage: ProviderUsage) -> Self {
        Self::Usage(usage)
    }
}
struct Runtime {
    session: SharedCell<Session>,
}
#[async_trait(?Send)]
impl SessionLoader<Session> for Runtime {
    async fn load(&self, _id: &str) -> Result<Session> {
        Ok(self.session.borrow_mut().clone())
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
        let mut session = self.session.borrow_mut();
        for effect in effects {
            match effect {
                Effect::Conversation(ConversationEffect::AppendMessage(message)) => {
                    session.conversation.push(message.clone())
                }
                Effect::Conversation(ConversationEffect::PatchToolRequestMeta {
                    tool_call_id,
                    patch,
                }) => {
                    for message in session.conversation.messages_mut() {
                        for content in &mut message.content {
                            if let goose_provider_types::conversation::message::MessageContent::ToolRequest(request) = content {
                                if request.id == *tool_call_id { request.tool_meta = Some(patch.clone()); }
                            }
                        }
                    }
                }
                Effect::Usage(usage) => {
                    let _ = usage;
                }
                _ => anyhow::bail!("unexpected effect in the basic device example"),
            }
        }
        Ok(())
    }
}
#[derive(Default)]
struct Events {
    messages: SharedCell<Vec<Message>>,
}
impl EventSink for Events {
    fn emit(&self, event: AgentEvent) {
        if let AgentEvent::Message(message) = event {
            self.messages.borrow_mut().push(message);
        }
    }
}

pub async fn demo() -> Result<Session> {
    let host = Shared::new(Host::default());
    let provider = Shared::new(DeviceProvider {
        host: host.clone(),
        network: Network {
            requests: Cell::new(0),
        },
    });
    let tool_calls = Shared::new(Cell::new(0));
    let tools = ToolOperation::new().with_local_tool(Temperature {
        calls: tool_calls.clone(),
    });
    let inference = InferenceRunner::new(provider.clone(), ModelConfig::new("device-model"));
    let steps: Vec<Step<'_, Session, Effect>> = vec![
        Step::Operation(Shared::new(ToolApprovalOperation::new(Shared::new(
            AllowLocalTools,
        )))),
        Step::Operation(Shared::new(tools)),
        Step::Inference(Shared::new(inference)),
    ];
    let cancel = CancellationToken::new();
    let machine = StateMachine::new(steps, cancel.clone());
    let events = Shared::new(Events::default());
    let emit = Emitter::with_host(events.clone(), cancel, host.clone());
    let runtime = Runtime {
        session: SharedCell::new(Session {
            conversation: Conversation::new_unvalidated(vec![Message::user_with_host(
                host.as_ref(),
            )
            .with_text("Read the temperature")]),
        }),
    };
    let session = machine.run(&runtime, "device-session", &emit).await?;
    anyhow::ensure!(
        provider.network.requests.get() == 2,
        "expected two provider requests"
    );
    anyhow::ensure!(tool_calls.get() == 1, "local tool must run exactly once");
    anyhow::ensure!(
        session
            .conversation
            .last()
            .is_some_and(|m| m.as_concat_text() == "Temperature: 21 C"),
        "missing final response"
    );
    anyhow::ensure!(
        !events.messages.borrow_mut().is_empty(),
        "missing streamed events"
    );
    Ok(session)
}

/// Sufficient for this fake transport, whose futures never wait for external I/O.
/// Real boards run `demo` on their own executor.
pub fn run_demo() -> Result<Session> {
    let mut future = pin!(demo());
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(result) = future.as_mut().poll(&mut cx) {
            return result;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_tool_provider_loop() {
        run_demo().unwrap();
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use goose_agent::operation::{Operation, OperationResult, RunStatus};
    struct Waiting {
        started: Shared<Cell<bool>>,
        finalized: Shared<Cell<usize>>,
    }
    #[async_trait(?Send)]
    impl Operation<Session, Effect> for Waiting {
        fn name(&self) -> &'static str {
            "waiting"
        }
        async fn run(
            &self,
            _session: &Session,
            _conversation: &Conversation,
            _emit: &Emitter,
        ) -> Result<OperationResult<Effect>> {
            self.started.set(true);
            core::future::pending().await
        }
        async fn finalize_cancellation(
            &self,
            _session: &Session,
            _conversation: &Conversation,
            emit: &Emitter,
        ) -> Vec<Effect> {
            self.finalized.set(self.finalized.get() + 1);
            vec![emit
                .message(Message::assistant_with_host(emit.host()).with_text("cancelled"))
                .into()]
        }
    }
    #[test]
    fn cancellation_interrupts_pending_work_and_finalizes_it() {
        let host = Shared::new(Host::default());
        let started = Shared::new(Cell::new(false));
        let finalized = Shared::new(Cell::new(0));
        let cancel = CancellationToken::new();
        let machine = StateMachine::new(
            vec![Step::Operation(Shared::new(Waiting {
                started: started.clone(),
                finalized: finalized.clone(),
            }))],
            cancel.clone(),
        );
        let emit = Emitter::with_host(Shared::new(Events::default()), cancel.clone(), host.clone());
        let runtime = Runtime {
            session: SharedCell::new(Session {
                conversation: Conversation::new_unvalidated(vec![Message::user_with_host(
                    host.as_ref(),
                )
                .with_text("wait")]),
            }),
        };
        let session = runtime.session.borrow_mut().clone();
        let mut step = pin!(machine.step(&session, &emit));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(step.as_mut().poll(&mut cx).is_pending());
        assert!(started.get());
        cancel.cancel();
        let Poll::Ready(result) = step.as_mut().poll(&mut cx) else {
            panic!("cancel did not wake pending step")
        };
        assert_eq!(result.unwrap().status, RunStatus::Cancelled);
        let mut finalize = pin!(machine.finalize(&runtime, "device-session", &emit));
        let Poll::Ready(session) = finalize.as_mut().poll(&mut cx) else {
            panic!("finalization remained pending")
        };
        assert_eq!(finalized.get(), 1);
        assert_eq!(
            session
                .unwrap()
                .conversation
                .last()
                .unwrap()
                .as_concat_text(),
            "cancelled"
        );
    }
    #[test]
    fn parent_cancellation_wakes_children_and_dropped_waiters_are_reusable() {
        use alloc::{sync::Arc, task::Wake};
        use core::sync::atomic::{AtomicUsize, Ordering};
        struct Count(AtomicUsize);
        impl Wake for Count {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let count = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(count.clone());
        let mut cx = Context::from_waker(&waker);
        let parent = CancellationToken::new();
        let child = parent.child_token();
        for _ in 0..10 {
            let mut waiter = pin!(child.cancelled());
            assert!(waiter.as_mut().poll(&mut cx).is_pending());
        }
        let mut waiting = pin!(child.cancelled());
        assert!(waiting.as_mut().poll(&mut cx).is_pending());
        parent.cancel();
        assert_eq!(count.0.load(Ordering::Relaxed), 1);
        assert!(child.is_cancelled());
        assert!(waiting.as_mut().poll(&mut cx).is_ready());
        let independent = CancellationToken::new();
        independent.child_token().cancel();
        assert!(!independent.is_cancelled());
    }
    #[test]
    fn explicit_services_produce_stable_ids_and_preserve_provider_ids() {
        let host = Host::default();
        let message = Message::assistant_with_host(&host).with_generated_id_with_host(&host);
        assert_eq!(message.created, 1_700_000_000);
        let id = message.id.clone();
        assert_eq!(message.with_generated_id_if_missing_with_host(&host).id, id);
        assert_eq!(
            Message::user_with_host(&host)
                .with_id("provider-id")
                .with_generated_id_if_missing_with_host(&host)
                .id
                .as_deref(),
            Some("provider-id")
        );
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    use futures::StreamExt;

    #[test]
    fn producer_advances_only_as_items_are_consumed_and_drop_stops_work() {
        let produced = Shared::new(Cell::new(0));
        let counter = produced.clone();
        let mut stream = goose_provider_types::stream::try_stream(|mut emit| async move {
            for value in 0..3 {
                counter.set(counter.get() + 1);
                emit.send(value).await;
            }
            Ok::<(), ProviderError>(())
        });
        assert_eq!(produced.get(), 0);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            pin!(stream.next()).poll(&mut cx),
            Poll::Ready(Some(Ok(0)))
        ));
        assert_eq!(produced.get(), 1);
        drop(stream);
        assert_eq!(produced.get(), 1);
    }

    #[test]
    fn stream_error_is_emitted_once_then_terminates() {
        let mut stream = goose_provider_types::stream::try_stream(
            |_emit: goose_provider_types::stream::Yield<()>| async {
                Err(ProviderError::NetworkError("offline".into()))
            },
        );
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            pin!(stream.next()).poll(&mut cx),
            Poll::Ready(Some(Err(ProviderError::NetworkError(_))))
        ));
        assert!(matches!(
            pin!(stream.next()).poll(&mut cx),
            Poll::Ready(None)
        ));
    }
}

#[cfg(test)]
mod owned_stream_tests {
    use super::*;
    use futures::StreamExt;

    #[test]
    fn owned_stream_keeps_borrowed_provider_alive_until_drop() {
        let host = Shared::new(Host::default());
        let provider = Shared::new(DeviceProvider {
            host: host.clone(),
            network: Network {
                requests: Cell::new(0),
            },
        });
        let weak = Shared::downgrade(&provider);
        let mut create = pin!(goose_provider_types::base::stream_owned(
            provider,
            ModelConfig::new("device-model"),
            String::new(),
            vec![Message::user_with_host(host.as_ref()).with_text("read")],
            vec![]
        ));
        let mut cx = Context::from_waker(Waker::noop());
        let Poll::Ready(Ok(mut stream)) = create.as_mut().poll(&mut cx) else {
            panic!("initialization did not complete")
        };
        assert_eq!(weak.upgrade().unwrap().network.requests.get(), 1);
        assert!(matches!(
            pin!(stream.next()).poll(&mut cx),
            Poll::Ready(Some(Ok(_)))
        ));
        drop(stream);
        assert!(weak.upgrade().is_none());
    }
}

#[cfg(test)]
mod borrowed_network_tests {
    use super::*;

    struct BorrowedProvider<'a> {
        host: &'a Host,
        network: &'a Network,
    }
    #[async_trait(?Send)]
    impl Provider for BorrowedProvider<'_> {
        fn get_name(&self) -> &str {
            "borrowed-device"
        }
        async fn stream(
            &self,
            _config: &ModelConfig,
            _system: &str,
            messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream<'_>, ProviderError> {
            use futures::StreamExt;
            let frames = self.network.request(messages).await;
            Ok(Box::pin(
                openai::response_to_streaming_message_with_host(
                    self.host,
                    futures::stream::iter(frames),
                )
                .map(|item| item.map_err(ProviderError::from_stream_error)),
            ))
        }
    }

    #[test]
    fn inference_can_borrow_a_stack_owned_network_driver() {
        let host = Shared::new(Host::default());
        let network = Network {
            requests: Cell::new(0),
        };
        let provider = Shared::new(BorrowedProvider {
            host: host.as_ref(),
            network: &network,
        });
        let calls = Shared::new(Cell::new(0));
        let steps: Vec<Step<'_, Session, Effect>> = vec![
            Step::Operation(Shared::new(ToolApprovalOperation::new(Shared::new(
                AllowLocalTools,
            )))),
            Step::Operation(Shared::new(ToolOperation::new().with_local_tool(
                Temperature {
                    calls: calls.clone(),
                },
            ))),
            Step::Inference(Shared::new(InferenceRunner::new(
                provider,
                ModelConfig::new("device-model"),
            ))),
        ];
        let cancel = CancellationToken::new();
        let machine = StateMachine::new(steps, cancel.clone());
        let emit = Emitter::with_host(Shared::new(Events::default()), cancel, host.clone());
        let runtime = Runtime {
            session: SharedCell::new(Session {
                conversation: Conversation::new_unvalidated(vec![Message::user_with_host(
                    host.as_ref(),
                )
                .with_text("read")]),
            }),
        };
        let mut run = pin!(machine.run(&runtime, "device-session", &emit));
        let mut cx = Context::from_waker(Waker::noop());
        let Poll::Ready(Ok(session)) = run.as_mut().poll(&mut cx) else {
            panic!("borrowed driver did not complete")
        };
        assert_eq!(network.requests.get(), 2);
        assert_eq!(calls.get(), 1);
        assert_eq!(
            session.conversation.last().unwrap().as_concat_text(),
            "Temperature: 21 C"
        );
    }
}
