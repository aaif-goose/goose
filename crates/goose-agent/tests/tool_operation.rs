use std::{
    borrow::Cow,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    events::AgentEvent,
    hook::{Hook, HookDecision},
    machine::{MachineSession, StateMachine, Step},
    operation::{
        ConversationEffect, Emitter, Inference, InferenceInput, Operation, OperationResult,
        RunStatus,
    },
    tool::{
        AfterToolCall, AfterToolCallHook, BeforeToolCall, BeforeToolCallHook, ToolOperation,
        ToolProvider,
    },
};
use goose_provider_types::conversation::{
    message::{Message, MessageContent, SystemNotificationType},
    Conversation,
};
use rmcp::{
    handler::server::router::tool::{AsyncTool, SyncTool, ToolBase},
    model::{CallToolRequestParams, CallToolResult, ErrorData, Tool},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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

    fn description() -> Option<Cow<'static, str>> {
        Some("Add two integers".into())
    }
}

impl SyncTool<()> for Add {
    fn invoke(_session: &(), input: AddInput) -> Result<AddOutput, ErrorData> {
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

impl AsyncTool<()> for Greet {
    async fn invoke(_session: &(), input: GreetInput) -> Result<GreetOutput, ErrorData> {
        Ok(GreetOutput {
            greeting: format!("Hello, {}!", input.name),
        })
    }
}

fn emitter() -> Emitter {
    emitter_with_token(CancellationToken::new())
}

fn emitter_with_token(cancel: CancellationToken) -> Emitter {
    let (tx, _rx) = mpsc::unbounded_channel();
    Emitter::new(tx, cancel)
}

fn operation() -> ToolOperation<()> {
    ToolOperation::new()
        .with_sync_tool::<Add>()
        .with_async_tool::<Greet>()
}

fn add_and_greet() -> Conversation {
    Conversation::new_unvalidated([
        Message::user().with_text("do both"),
        Message::assistant()
            .with_tool_request(
                "call-1",
                Ok(CallToolRequestParams::new("add").with_arguments(
                    serde_json::from_value(json!({"left": 2, "right": 3})).unwrap(),
                )),
            )
            .with_tool_request(
                "call-2",
                Ok(CallToolRequestParams::new("greet")
                    .with_arguments(serde_json::from_value(json!({"name": "Goose"})).unwrap())),
            ),
    ])
}

fn appended_message(result: OperationResult<ConversationEffect>) -> Message {
    let OperationResult::Applied(result) = result else {
        panic!("operation should apply");
    };
    let ConversationEffect::AppendMessage(message) = result.effects.into_iter().next().unwrap()
    else {
        panic!("operation should append a message");
    };
    message
}

#[tokio::test]
async fn advertises_user_defined_tools_directly() {
    let tools = <ToolOperation<()> as Operation<(), ConversationEffect>>::inference_tools(
        &operation(),
        &(),
    )
    .await
    .unwrap();

    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "add");
    assert_eq!(tools[0].description.as_deref(), Some("Add two integers"));
    assert!(tools[0].input_schema["properties"]["left"].is_object());
    assert_eq!(tools[1].name, "greet");
}

#[tokio::test]
async fn dispatches_calls_to_user_defined_tools() {
    let conversation = add_and_greet();

    let message = appended_message(
        <ToolOperation<()> as Operation<(), ConversationEffect>>::run(
            &operation(),
            &(),
            &conversation,
            &emitter(),
        )
        .await
        .unwrap(),
    );
    let responses = message
        .content
        .iter()
        .map(|content| match content {
            MessageContent::ToolResponse(response) => response,
            _ => panic!("expected tool responses"),
        })
        .collect::<Vec<_>>();
    assert_eq!(responses[0].id, "call-1");
    assert_eq!(
        responses[0]
            .tool_result
            .as_ref()
            .unwrap()
            .structured_content,
        Some(json!({"sum": 5}))
    );
    assert_eq!(responses[1].id, "call-2");
    assert_eq!(
        responses[1]
            .tool_result
            .as_ref()
            .unwrap()
            .structured_content,
        Some(json!({"greeting": "Hello, Goose!"}))
    );
}

struct DynamicTools {
    discoveries: Arc<AtomicUsize>,
}

#[async_trait]
impl ToolProvider<bool> for DynamicTools {
    async fn tools(&self, enabled: &bool) -> Result<Vec<Tool>> {
        self.discoveries.fetch_add(1, Ordering::SeqCst);
        Ok(if *enabled {
            vec![Tool::new(
                "dynamic",
                "A session-dependent tool",
                Arc::new(serde_json::from_value(json!({"type": "object"}))?),
            )]
        } else {
            Vec::new()
        })
    }

    async fn call(
        &self,
        _session: &bool,
        request_id: &str,
        call: CallToolRequestParams,
        _emit: &Emitter,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::structured(json!({
            "request_id": request_id,
            "name": call.name
        })))
    }
}

#[tokio::test]
async fn rediscovers_dynamic_tools_for_inference_and_execution() {
    let discoveries = Arc::new(AtomicUsize::new(0));
    let operation = ToolOperation::new().with_provider(Arc::new(DynamicTools {
        discoveries: discoveries.clone(),
    }));
    let tools = <ToolOperation<bool> as Operation<bool, ConversationEffect>>::inference_tools(
        &operation, &true,
    )
    .await
    .unwrap();
    assert_eq!(tools[0].name, "dynamic");

    let conversation = Conversation::new_unvalidated([
        Message::user().with_text("call the dynamic tool"),
        Message::assistant()
            .with_tool_request("dynamic-call", Ok(CallToolRequestParams::new("dynamic"))),
    ]);
    let message = appended_message(
        <ToolOperation<bool> as Operation<bool, ConversationEffect>>::run(
            &operation,
            &true,
            &conversation,
            &emitter(),
        )
        .await
        .unwrap(),
    );

    assert_eq!(discoveries.load(Ordering::SeqCst), 2);
    assert_eq!(
        message.content[0]
            .as_tool_response()
            .unwrap()
            .tool_result
            .as_ref()
            .unwrap()
            .structured_content,
        Some(json!({"request_id": "dynamic-call", "name": "dynamic"}))
    );
}

#[tokio::test]
async fn dynamic_tools_vary_by_session() {
    let operation = ToolOperation::new().with_provider(Arc::new(DynamicTools {
        discoveries: Arc::new(AtomicUsize::new(0)),
    }));
    let enabled = <ToolOperation<bool> as Operation<bool, ConversationEffect>>::inference_tools(
        &operation, &true,
    )
    .await
    .unwrap();
    let disabled = <ToolOperation<bool> as Operation<bool, ConversationEffect>>::inference_tools(
        &operation, &false,
    )
    .await
    .unwrap();

    assert_eq!(enabled[0].name, "dynamic");
    assert!(disabled.is_empty());
}

#[tokio::test]
async fn rejects_duplicate_dynamic_tool_names_at_both_boundaries() {
    let operation = ToolOperation::new()
        .with_provider(Arc::new(DynamicTools {
            discoveries: Arc::new(AtomicUsize::new(0)),
        }))
        .with_provider(Arc::new(DynamicTools {
            discoveries: Arc::new(AtomicUsize::new(0)),
        }));
    let kickoff = Message::user().with_text("use tools");

    let inference_error =
        <ToolOperation<bool> as Operation<bool, ConversationEffect>>::inference_tools(
            &operation, &true,
        )
        .await
        .unwrap_err();
    assert_eq!(
        inference_error.to_string(),
        "multiple tool providers registered 'dynamic'"
    );

    let conversation = Conversation::new_unvalidated([
        kickoff,
        Message::assistant()
            .with_tool_request("dynamic-call", Ok(CallToolRequestParams::new("dynamic"))),
    ]);
    let execution_error = match <ToolOperation<bool> as Operation<bool, ConversationEffect>>::run(
        &operation,
        &true,
        &conversation,
        &emitter(),
    )
    .await
    {
        Ok(_) => panic!("duplicate tools should fail during execution discovery"),
        Err(error) => error,
    };
    assert_eq!(
        execution_error.to_string(),
        "multiple tool providers registered 'dynamic'"
    );
}

#[tokio::test]
async fn ignores_requests_outside_the_current_turn() {
    let conversation = Conversation::new_unvalidated([
        Message::user().with_text("old turn"),
        Message::assistant().with_tool_request(
            "stale-call",
            Ok(CallToolRequestParams::new("add")
                .with_arguments(serde_json::from_value(json!({"left": 2, "right": 3})).unwrap())),
        ),
        Message::user().with_text("new turn"),
    ]);

    let result = <ToolOperation<()> as Operation<(), ConversationEffect>>::run(
        &operation(),
        &(),
        &conversation,
        &emitter(),
    )
    .await
    .unwrap();

    assert!(matches!(result, OperationResult::NotApplicable));
}

#[tokio::test]
async fn responds_to_unparseable_tool_requests() {
    let conversation = Conversation::new_unvalidated([
        Message::user().with_text("call a tool"),
        Message::assistant().with_tool_request(
            "invalid-call",
            Err(ErrorData::invalid_params("malformed arguments", None)),
        ),
    ]);

    let message = appended_message(
        <ToolOperation<()> as Operation<(), ConversationEffect>>::run(
            &operation(),
            &(),
            &conversation,
            &emitter(),
        )
        .await
        .unwrap(),
    );
    let response = message.content[0].as_tool_response().unwrap();
    assert_eq!(response.id, "invalid-call");
    assert_eq!(
        response.tool_result.as_ref().unwrap_err().message,
        "malformed arguments"
    );
}

struct BlockingDiscovery {
    started: Arc<AtomicBool>,
}

#[async_trait]
impl<S: Send + Sync> ToolProvider<S> for BlockingDiscovery {
    async fn tools(&self, _session: &S) -> Result<Vec<Tool>> {
        self.started.store(true, Ordering::SeqCst);
        std::future::pending().await
    }

    async fn call(
        &self,
        _session: &S,
        _request_id: &str,
        _call: CallToolRequestParams,
        _emit: &Emitter,
    ) -> Result<CallToolResult, ErrorData> {
        unreachable!()
    }
}

struct TestSession {
    conversation: Conversation,
}

impl MachineSession for TestSession {
    fn id(&self) -> &str {
        "test"
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.conversation)
    }
}

struct TestInference;

#[async_trait]
impl Operation<TestSession> for TestInference {
    fn name(&self) -> &'static str {
        "inference"
    }
}

#[async_trait]
impl Inference<TestSession> for TestInference {
    fn applies(&self, _conversation: &Conversation) -> bool {
        true
    }

    async fn infer(
        &self,
        _session: &TestSession,
        _conversation: &Conversation,
        _input: InferenceInput,
        _emit: &Emitter,
    ) -> Result<OperationResult<ConversationEffect>> {
        unreachable!()
    }
}

#[tokio::test]
async fn cancellation_interrupts_inference_discovery() {
    let started = Arc::new(AtomicBool::new(false));
    let operation = ToolOperation::new().with_provider(Arc::new(BlockingDiscovery {
        started: started.clone(),
    }));
    let cancel = CancellationToken::new();
    let machine = StateMachine::new(
        vec![
            Step::Operation(Arc::new(operation)),
            Step::Inference(Arc::new(TestInference)),
        ],
        cancel.clone(),
    );
    let session = TestSession {
        conversation: Conversation::new_unvalidated([Message::user().with_text("use tools")]),
    };
    let emit = emitter_with_token(cancel.clone());

    let step = tokio::spawn(async move { machine.step(&session, &emit).await });
    while !started.load(Ordering::SeqCst) {
        tokio::task::yield_now().await;
    }
    cancel.cancel();

    assert_eq!(step.await.unwrap().unwrap().status, RunStatus::Cancelled);
}

#[tokio::test]
async fn ignores_unavailable_answered_and_externally_dispatched_requests() {
    let conversation = Conversation::new_unvalidated([
        Message::user().with_text("call tools"),
        Message::assistant()
            .with_tool_request("answered", Ok(CallToolRequestParams::new("add")))
            .with_tool_request(
                "unavailable",
                Ok(CallToolRequestParams::new("not_registered")),
            )
            .with_tool_request_with_metadata(
                "external",
                Ok(CallToolRequestParams::new("add")),
                None,
                Some(json!({"goose.external_dispatch": true})),
            ),
        Message::user().with_tool_response(
            "answered",
            Ok(CallToolResult::structured(json!({"sum": 5}))),
        ),
    ]);

    let result = <ToolOperation<()> as Operation<(), ConversationEffect>>::run(
        &operation(),
        &(),
        &conversation,
        &emitter(),
    )
    .await
    .unwrap();

    assert!(matches!(result, OperationResult::NotApplicable));
}

fn notice(text: &str) -> Message {
    Message::assistant().with_system_notification(SystemNotificationType::InlineMessage, text)
}

#[derive(Default)]
struct RecordingHook {
    deny: Option<(&'static str, &'static str)>,
    asked: Mutex<Vec<String>>,
    results: Mutex<Vec<(String, Result<CallToolResult, ErrorData>)>>,
}

impl RecordingHook {
    fn allowing() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn denying(tool: &'static str, reason: &'static str) -> Arc<Self> {
        Arc::new(Self {
            deny: Some((tool, reason)),
            ..Self::default()
        })
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }

    fn results(&self) -> Vec<(String, Result<CallToolResult, ErrorData>)> {
        self.results.lock().unwrap().clone()
    }
}

#[async_trait]
impl<'a> Hook<(), BeforeToolCall<'a>> for RecordingHook {
    async fn decide(&self, _session: &(), event: &BeforeToolCall<'a>) -> Result<HookDecision> {
        let call = event.call;
        self.asked
            .lock()
            .unwrap()
            .push(event.request_id.to_string());
        Ok(match self.deny {
            Some((tool, reason)) if tool == call.name => HookDecision::Deny {
                reason: reason.to_string(),
                user_message: None,
            },
            _ => HookDecision::Allow,
        })
    }
}

#[async_trait]
impl<'a> Hook<(), AfterToolCall<'a>> for RecordingHook {
    async fn decide(&self, _session: &(), event: &AfterToolCall<'a>) -> Result<HookDecision> {
        self.results
            .lock()
            .unwrap()
            .push((event.request_id.to_string(), event.result.clone()));
        Ok(HookDecision::Allow)
    }
}

struct CountedTool {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ToolProvider<()> for CountedTool {
    async fn tools(&self, _session: &()) -> Result<Vec<Tool>> {
        Ok(vec![Tool::new(
            "counted",
            "Counts its calls",
            Arc::new(serde_json::from_value(json!({"type": "object"}))?),
        )])
    }

    async fn call(
        &self,
        _session: &(),
        _request_id: &str,
        _call: CallToolRequestParams,
        _emit: &Emitter,
    ) -> Result<CallToolResult, ErrorData> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(CallToolResult::structured(json!({})))
    }
}

fn with_recording(operation: ToolOperation<()>, hooks: &[Arc<RecordingHook>]) -> ToolOperation<()> {
    let before_call_hooks: Vec<Arc<BeforeToolCallHook<()>>> = hooks
        .iter()
        .map(|hook| hook.clone() as Arc<BeforeToolCallHook<()>>)
        .collect();
    let after_call_hooks: Vec<Arc<AfterToolCallHook<()>>> = hooks
        .iter()
        .map(|hook| hook.clone() as Arc<AfterToolCallHook<()>>)
        .collect();
    operation
        .with_before_call_hooks(before_call_hooks)
        .with_after_call_hooks(after_call_hooks)
}

async fn run_with_hooks(
    hooks: &[Arc<RecordingHook>],
    conversation: &Conversation,
) -> Vec<Result<CallToolResult, ErrorData>> {
    run_operation(with_recording(operation(), hooks), conversation).await
}

async fn run_operation(
    operation: ToolOperation<()>,
    conversation: &Conversation,
) -> Vec<Result<CallToolResult, ErrorData>> {
    run_and_collect_notifications(operation, conversation)
        .await
        .0
}

async fn run_and_collect_notifications(
    operation: ToolOperation<()>,
    conversation: &Conversation,
) -> (Vec<Result<CallToolResult, ErrorData>>, Vec<String>) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let message = appended_message(
        <ToolOperation<()> as Operation<(), ConversationEffect>>::run(
            &operation,
            &(),
            conversation,
            &emit,
        )
        .await
        .unwrap(),
    );
    let results = message
        .content
        .iter()
        .map(|content| content.as_tool_response().unwrap().tool_result.clone())
        .collect();
    let notifications = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|event| match event {
            AgentEvent::Message(message) => {
                message.content.iter().find_map(|content| match content {
                    MessageContent::SystemNotification(notification) => {
                        Some(notification.msg.clone())
                    }
                    _ => None,
                })
            }
            _ => None,
        })
        .collect();
    (results, notifications)
}

#[tokio::test]
async fn allowing_hooks_let_calls_run_and_see_their_results() {
    let hook = RecordingHook::allowing();

    let results = run_with_hooks(std::slice::from_ref(&hook), &add_and_greet()).await;

    assert_eq!(
        results[0].as_ref().unwrap().structured_content,
        Some(json!({"sum": 5}))
    );
    assert_eq!(hook.asked(), vec!["call-1", "call-2"]);
    let seen = hook.results();
    assert_eq!(seen[0].0, "call-1");
    assert_eq!(
        seen[0].1.as_ref().unwrap().structured_content,
        Some(json!({"sum": 5}))
    );
    assert_eq!(seen[1].0, "call-2");
}

#[tokio::test]
async fn a_denied_call_is_skipped_and_returns_the_reason() {
    let calls = Arc::new(AtomicUsize::new(0));
    let hook = RecordingHook::denying("counted", "counted is not allowed");
    let operation = with_recording(
        operation().with_provider(Arc::new(CountedTool {
            calls: calls.clone(),
        })),
        std::slice::from_ref(&hook),
    );
    let conversation = Conversation::new_unvalidated([
        Message::user().with_text("do both"),
        Message::assistant()
            .with_tool_request("call-1", Ok(CallToolRequestParams::new("counted")))
            .with_tool_request(
                "call-2",
                Ok(CallToolRequestParams::new("greet")
                    .with_arguments(serde_json::from_value(json!({"name": "Goose"})).unwrap())),
            ),
    ]);

    let results = run_operation(operation, &conversation).await;

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        results[0].as_ref().unwrap_err().message,
        "counted is not allowed"
    );
    assert_eq!(
        results[1].as_ref().unwrap().structured_content,
        Some(json!({"greeting": "Hello, Goose!"}))
    );
    let seen = hook.results();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "call-2");
}

struct DenyResult;

#[async_trait]
impl<'a> Hook<(), AfterToolCall<'a>> for DenyResult {
    async fn decide(&self, _session: &(), event: &AfterToolCall<'a>) -> Result<HookDecision> {
        if event.call.name != "add" {
            return Ok(HookDecision::Allow);
        }
        Ok(HookDecision::Deny {
            reason: "the sum is secret".to_string(),
            user_message: Some(notice("add result was hidden")),
        })
    }
}

#[tokio::test]
async fn a_denied_result_is_replaced_with_the_reason() {
    let (results, shown) = run_and_collect_notifications(
        operation().with_after_call_hooks(vec![Arc::new(DenyResult)]),
        &add_and_greet(),
    )
    .await;

    assert_eq!(
        results[0].as_ref().unwrap_err().message,
        "the sum is secret"
    );
    assert_eq!(
        results[1].as_ref().unwrap().structured_content,
        Some(json!({"greeting": "Hello, Goose!"}))
    );
    assert_eq!(shown, vec!["add result was hidden"]);
}

struct FailOn(&'static str);

#[async_trait]
impl<'a> Hook<(), BeforeToolCall<'a>> for FailOn {
    async fn decide(&self, _session: &(), event: &BeforeToolCall<'a>) -> Result<HookDecision> {
        if event.call.name != self.0 {
            return Ok(HookDecision::Allow);
        }
        anyhow::bail!("before-call hook crashed")
    }
}

#[async_trait]
impl<'a> Hook<(), AfterToolCall<'a>> for FailOn {
    async fn decide(&self, _session: &(), event: &AfterToolCall<'a>) -> Result<HookDecision> {
        if event.call.name != self.0 {
            return Ok(HookDecision::Allow);
        }
        anyhow::bail!("after-call hook crashed")
    }
}

#[tokio::test]
async fn a_failing_before_call_hook_skips_the_call_and_returns_the_error() {
    let calls = Arc::new(AtomicUsize::new(0));
    let operation = operation()
        .with_provider(Arc::new(CountedTool {
            calls: calls.clone(),
        }))
        .with_before_call_hooks(vec![Arc::new(FailOn("counted"))]);
    let conversation = Conversation::new_unvalidated([
        Message::user().with_text("count"),
        Message::assistant().with_tool_request("call-1", Ok(CallToolRequestParams::new("counted"))),
    ]);

    let results = run_operation(operation, &conversation).await;

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        results[0].as_ref().unwrap_err().message,
        "The tool did not run because a hook failed: before-call hook crashed"
    );
}

#[tokio::test]
async fn a_failing_after_call_hook_replaces_the_result_with_the_error() {
    let results = run_operation(
        operation().with_after_call_hooks(vec![Arc::new(FailOn("add"))]),
        &add_and_greet(),
    )
    .await;

    assert_eq!(
        results[0].as_ref().unwrap_err().message,
        "The tool completed, but a hook failed while checking its result: after-call hook crashed"
    );
    assert_eq!(
        results[1].as_ref().unwrap().structured_content,
        Some(json!({"greeting": "Hello, Goose!"}))
    );
}

#[tokio::test]
async fn unparseable_calls_skip_hooks() {
    let hook = RecordingHook::allowing();
    let conversation = Conversation::new_unvalidated([
        Message::user().with_text("call a tool"),
        Message::assistant().with_tool_request(
            "invalid-call",
            Err(ErrorData::invalid_params("malformed arguments", None)),
        ),
    ]);

    let results = run_with_hooks(std::slice::from_ref(&hook), &conversation).await;

    assert_eq!(
        results[0].as_ref().unwrap_err().message,
        "malformed arguments"
    );
    assert!(hook.asked().is_empty());
    assert!(hook.results().is_empty());
}

#[derive(Default)]
struct StuckAfterCall {
    reached: AtomicBool,
}

#[async_trait]
impl<'a> Hook<(), AfterToolCall<'a>> for StuckAfterCall {
    async fn decide(&self, _session: &(), _event: &AfterToolCall<'a>) -> Result<HookDecision> {
        self.reached.store(true, Ordering::SeqCst);
        std::future::pending().await
    }
}

#[tokio::test]
async fn stopping_during_after_call_hooks_records_that_the_tool_ran() {
    let hook = Arc::new(StuckAfterCall::default());
    let operation = operation().with_after_call_hooks(vec![hook.clone()]);
    let conversation = Conversation::new_unvalidated([
        Message::user().with_text("greet"),
        Message::assistant().with_tool_request(
            "call-1",
            Ok(CallToolRequestParams::new("greet")
                .with_arguments(serde_json::from_value(json!({"name": "Goose"})).unwrap())),
        ),
    ]);
    let emit = emitter();

    {
        let mut run = <ToolOperation<()> as Operation<(), ConversationEffect>>::run(
            &operation,
            &(),
            &conversation,
            &emit,
        );
        assert!(futures::poll!(run.as_mut()).is_pending());
    }
    assert!(hook.reached.load(Ordering::SeqCst));

    let saved = <ToolOperation<()> as Operation<(), ConversationEffect>>::finalize_cancellation(
        &operation,
        &(),
        &conversation,
        &emit,
    )
    .await;
    let [ConversationEffect::AppendMessage(message)] = saved.as_slice() else {
        panic!("expected one appended message");
    };
    let response = message.content[0].as_tool_response().unwrap();
    assert_eq!(response.id, "call-1");
    assert_eq!(
        response.tool_result.as_ref().unwrap_err().message,
        "The tool completed, but the run was stopped before a hook checked its result"
    );
}
