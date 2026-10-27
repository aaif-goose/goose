#![cfg(not(target_arch = "wasm32"))]

use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt};
use goose_provider_types::{
    base::{MessageStream, Provider},
    conversation::{
        message::{Message, MessageContent, SystemNotificationType},
        token_usage::{ProviderUsage, Usage},
    },
    errors::ProviderError,
    formats::{anthropic, openai, openai_responses},
    model::ModelConfig,
    retry::{default_recovery_budget, stream_with_recovery, ProviderRetry},
};
use rmcp::model::{CallToolRequestParams, Tool};

type Item = Result<(Option<Message>, Option<ProviderUsage>), ProviderError>;

enum Reply {
    CreateError(ProviderError),
    Items(Vec<Item>),
    PendingCreation,
    PendingStream,
    SlowOutput,
    Lines(&'static str, Vec<&'static str>),
}

struct ScriptedProvider {
    replies: Mutex<VecDeque<Reply>>,
    calls: AtomicUsize,
    requests: AtomicUsize,
    manages_context: bool,
}

impl ScriptedProvider {
    fn new(replies: Vec<Reply>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            calls: AtomicUsize::new(0),
            requests: AtomicUsize::new(0),
            manages_context: false,
        })
    }

    fn stream(self: &Arc<Self>, budget: Duration) -> MessageStream {
        stream_with_recovery(
            self.clone(),
            ModelConfig::new("model"),
            "system".into(),
            vec![],
            vec![],
            budget,
        )
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    fn get_name(&self) -> &str {
        "scripted"
    }

    fn manages_own_context(&self) -> bool {
        self.manages_context
    }

    async fn stream(
        &self,
        _: &ModelConfig,
        _: &str,
        _: &[Message],
        _: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected replay");
        match reply {
            Reply::CreateError(error) => {
                self.with_retry(|| async {
                    self.requests.fetch_add(1, Ordering::SeqCst);
                    Err(error.clone())
                })
                .await
            }
            Reply::Items(items) => Ok(Box::pin(futures::stream::iter(items))),
            Reply::PendingCreation => futures::future::pending().await,
            Reply::PendingStream => Ok(Box::pin(futures::stream::pending())),
            Reply::SlowOutput => Ok(Box::pin(async_stream::stream! {
                yield ok();
                tokio::time::sleep(Duration::from_secs(10)).await;
                yield ok();
            })),
            Reply::Lines(format, lines) => {
                let lines =
                    futures::stream::iter(lines.into_iter().map(|line| Ok(line.to_string())))
                        .chain(futures::stream::once(async {
                            Err(anyhow::anyhow!("connection lost"))
                        }));
                let lines = Box::pin(lines);
                let stream: MessageStream = match format {
                    "openai" => Box::pin(
                        openai::response_to_streaming_message(lines).map_err(ProviderError::from),
                    ),
                    "anthropic" => Box::pin(
                        anthropic::response_to_streaming_message(lines)
                            .map_err(ProviderError::from),
                    ),
                    "responses" => Box::pin(
                        openai_responses::responses_api_to_streaming_message(lines)
                            .map_err(ProviderError::from),
                    ),
                    _ => unreachable!(),
                };
                Ok(stream)
            }
        }
    }
}

fn failure() -> ProviderError {
    ProviderError::NetworkError("offline".into())
}
fn ok() -> Item {
    Ok((Some(Message::assistant().with_text("ok")), None))
}

fn assert_progress(item: Item) {
    let (message, usage) = item.unwrap();
    assert!(usage.is_none());
    let message = message.unwrap();
    assert!(!message.is_agent_visible());
    assert!(message.is_user_visible());
    assert!(
        matches!(&message.content[0], MessageContent::SystemNotification(n)
        if n.notification_type == SystemNotificationType::ProgressMessage && n.msg.contains("Retrying"))
    );
}

#[tokio::test(start_paused = true)]
async fn creation_and_stream_errors_share_recovery_with_immediate_feedback() {
    let provider = ScriptedProvider::new(vec![
        Reply::CreateError(failure()),
        Reply::Items(vec![Err(failure())]),
        Reply::Items(vec![ok()]),
    ]);
    let start = tokio::time::Instant::now();
    let mut stream = provider.stream(Duration::from_secs(120));
    assert_progress(stream.next().await.unwrap());
    assert_eq!(tokio::time::Instant::now(), start);
    assert_progress(stream.next().await.unwrap());
    assert_eq!(
        stream
            .next()
            .await
            .unwrap()
            .unwrap()
            .0
            .unwrap()
            .as_concat_text(),
        "ok"
    );
    assert!(stream.next().await.is_none());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        provider.requests.load(Ordering::SeqCst),
        1,
        "provider retries must not nest"
    );
}

#[tokio::test(start_paused = true)]
async fn zero_disables_even_provider_internal_retries() {
    let provider = ScriptedProvider::new(vec![Reply::CreateError(failure())]);
    let items: Vec<_> = provider.stream(Duration::ZERO).collect().await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_ref().unwrap_err(), &failure());
    assert_eq!(provider.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn budget_bounds_backoff_and_both_kinds_of_pending_attempts() {
    for reply in [Reply::PendingCreation, Reply::PendingStream] {
        let provider = ScriptedProvider::new(vec![Reply::CreateError(failure()), reply]);
        let start = tokio::time::Instant::now();
        let mut stream = provider.stream(Duration::from_secs(5));
        assert_progress(stream.next().await.unwrap());
        assert_eq!(stream.next().await.unwrap().unwrap_err(), failure());
        assert_eq!(start.elapsed(), Duration::from_secs(5));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test(start_paused = true)]
async fn retry_after_is_respected_without_extending_the_budget() {
    for delay in [Duration::from_secs(3), Duration::from_secs(30)] {
        let error = ProviderError::RateLimitExceeded {
            details: "busy".into(),
            retry_delay: Some(delay),
        };
        let provider = ScriptedProvider::new(vec![
            Reply::CreateError(error.clone()),
            Reply::Items(vec![ok()]),
        ]);
        let start = tokio::time::Instant::now();
        let mut stream = provider.stream(Duration::from_secs(5));
        assert_progress(stream.next().await.unwrap());
        let result = stream.next().await.unwrap();
        if delay.as_secs() < 5 {
            assert!(result.is_ok());
            assert_eq!(start.elapsed(), delay);
        } else {
            assert_eq!(result.unwrap_err(), error);
            assert_eq!(start.elapsed(), Duration::from_secs(5));
            assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn permanent_errors_are_not_retried() {
    for error in [
        ProviderError::Authentication("bad key".into()),
        ProviderError::RequestFailed("404".into()),
        ProviderError::ContextLengthExceeded("full".into()),
    ] {
        let provider = ScriptedProvider::new(vec![Reply::Items(vec![Err(error.clone())])]);
        let items: Vec<_> = provider.stream(Duration::from_secs(120)).collect().await;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_ref().unwrap_err(), &error);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn usage_and_empty_chunks_do_not_disable_recovery() {
    let usage = ProviderUsage::new("model".into(), Usage::default());
    let provider = ScriptedProvider::new(vec![
        Reply::Items(vec![
            Ok((None, Some(usage.clone()))),
            Ok((Some(Message::assistant()), None)),
            Err(failure()),
        ]),
        Reply::Items(vec![ok()]),
    ]);
    let mut stream = provider.stream(Duration::from_secs(120));
    assert_eq!(
        stream.next().await.unwrap().unwrap().1.unwrap().model,
        usage.model
    );
    assert!(stream.next().await.unwrap().is_ok());
    assert_progress(stream.next().await.unwrap());
    assert_eq!(
        stream
            .next()
            .await
            .unwrap()
            .unwrap()
            .0
            .unwrap()
            .as_concat_text(),
        "ok"
    );
}

#[tokio::test(start_paused = true)]
async fn partial_text_thinking_and_tools_are_preserved_without_replay() {
    for message in [
        Message::assistant().with_text("partial"),
        Message::assistant().with_thinking("thinking", "sig"),
        Message::assistant().with_tool_request("id", Ok(CallToolRequestParams::new("tool"))),
    ] {
        let provider = ScriptedProvider::new(vec![Reply::Items(vec![
            Ok((Some(message.clone()), None)),
            Err(failure()),
        ])]);
        let items: Vec<_> = provider.stream(Duration::from_secs(120)).collect().await;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_ref().unwrap().0.as_ref().unwrap(), &message);
        assert_eq!(items[1].as_ref().unwrap_err(), &failure());
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn buffered_tool_and_thinking_fragments_are_not_replayed() {
    for (format, lines) in [
        (
            "openai",
            vec![
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call","function":{"name":"tool","arguments":"{"}}]}}]}"#,
            ],
        ),
        (
            "openai",
            vec![r#"data: {"choices":[{"delta":{"reasoning_content":"thinking"}}]}"#],
        ),
        (
            "anthropic",
            vec![
                r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call","name":"tool","input":{}}}"#,
            ],
        ),
        (
            "anthropic",
            vec![
                r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"partial"}}"#,
            ],
        ),
        (
            "responses",
            vec![
                r#"data: {"type":"response.function_call_arguments.delta","sequence_number":1,"item_id":"call","output_index":0,"delta":"{"}"#,
            ],
        ),
        (
            "responses",
            vec![r#"data: {"type":"response.reasoning_summary_text.delta","delta":"thinking"}"#],
        ),
    ] {
        let provider = ScriptedProvider::new(vec![Reply::Lines(format, lines)]);
        let items: Vec<_> = provider.stream(Duration::from_secs(120)).collect().await;
        assert!(items.last().unwrap().is_err(), "{format}");
        assert!(!items.iter().any(|item| item
            .as_ref()
            .ok()
            .and_then(|item| item.0.as_ref())
            .is_some_and(|m| m
                .content
                .iter()
                .any(|c| matches!(c, MessageContent::SystemNotification(_))))));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1, "{format}");
    }
}

#[tokio::test(start_paused = true)]
async fn slow_first_request_has_no_recovery_deadline() {
    let provider = ScriptedProvider::new(vec![Reply::PendingCreation]);
    let mut stream = provider.stream(Duration::from_secs(1));
    assert!(tokio::time::timeout(Duration::from_secs(10), stream.next())
        .await
        .is_err());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn dropping_during_backoff_does_not_send_another_request() {
    let provider = ScriptedProvider::new(vec![Reply::CreateError(failure())]);
    let mut stream = provider.stream(Duration::from_secs(120));
    assert_progress(stream.next().await.unwrap());
    assert!(
        tokio::time::timeout(Duration::from_millis(1), stream.next())
            .await
            .is_err()
    );
    drop(stream);
    tokio::time::advance(Duration::from_secs(200)).await;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn successful_output_ends_the_recovery_deadline() {
    let provider = ScriptedProvider::new(vec![Reply::CreateError(failure()), Reply::SlowOutput]);
    let mut stream = provider.stream(Duration::from_secs(5));
    assert_progress(stream.next().await.unwrap());
    assert!(stream.next().await.unwrap().is_ok());
    assert!(stream.next().await.unwrap().is_ok());
    assert!(stream.next().await.is_none());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
}

struct Adapter(Arc<dyn Provider>);

#[async_trait]
impl Provider for Adapter {
    fn get_name(&self) -> &str {
        "adapter"
    }
    async fn stream(
        &self,
        config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        Ok(stream_with_recovery(
            self.0.clone(),
            config.clone(),
            system.into(),
            messages.to_vec(),
            tools.to_vec(),
            Duration::from_secs(120),
        ))
    }
}

#[tokio::test(start_paused = true)]
async fn nested_adapters_share_the_outer_budget_and_do_not_duplicate_progress() {
    let provider = ScriptedProvider::new(vec![Reply::CreateError(failure()), Reply::PendingStream]);
    let mut stream = stream_with_recovery(
        Arc::new(Adapter(provider.clone())),
        ModelConfig::new("model"),
        "system".into(),
        vec![],
        vec![],
        Duration::from_secs(5),
    );
    let start = tokio::time::Instant::now();
    assert_progress(stream.next().await.unwrap());
    assert_eq!(stream.next().await.unwrap().unwrap_err(), failure());
    assert_eq!(start.elapsed(), Duration::from_secs(5));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn concurrent_requests_do_not_share_output_or_retry_state() {
    let first = ScriptedProvider::new(vec![Reply::SlowOutput]);
    let second = ScriptedProvider::new(vec![
        Reply::CreateError(failure()),
        Reply::Items(vec![ok()]),
    ]);
    let first_stream = first.stream(Duration::from_secs(5));
    let second_stream = second.stream(Duration::from_secs(5));
    let (first_items, second_items) = futures::join!(
        first_stream.collect::<Vec<_>>(),
        second_stream.collect::<Vec<_>>()
    );
    assert_eq!(first_items.len(), 2);
    assert_eq!(second_items.len(), 2);
    assert_eq!(second.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn recovery_budget_configuration_defaults_and_zero() {
    for (value, expected) in [
        (None, 120),
        (Some("0"), 0),
        (Some("15"), 15),
        (Some("invalid"), 120),
    ] {
        let _guard = env_lock::lock_env([("GOOSE_PROVIDER_RETRY_TIMEOUT", value)]);
        assert_eq!(default_recovery_budget(), Duration::from_secs(expected));
    }
}
