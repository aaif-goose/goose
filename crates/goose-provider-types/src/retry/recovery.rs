use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use async_stream::try_stream;
use futures::{future::Either, StreamExt};
use rmcp::model::Tool;

use super::{should_retry, sleep, RetryConfig};
use crate::base::{MessageStream, Provider};
use crate::conversation::message::{Message, MessageContent, SystemNotificationType};
use crate::errors::ProviderError;
use crate::model::ModelConfig;

tokio::task_local! {
    static OUTPUT_STARTED: Arc<AtomicBool>;
}

/// Whether the agent owns retries for the current provider request. Providers
/// with custom retry loops should surface temporary failures in this scope.
pub fn stream_recovery_active() -> bool {
    OUTPUT_STARTED.try_with(|_| ()).is_ok()
}

/// Mark output that a provider buffers before yielding a complete message, such
/// as a tool-call argument fragment. Such requests must never be replayed.
pub fn record_stream_output() {
    let _ = OUTPUT_STARTED.try_with(|output| output.store(true, Ordering::Relaxed));
}

/// Recovery time from the first temporary failure, including waits and attempts.
/// `GOOSE_PROVIDER_RETRY_TIMEOUT` is in seconds; zero disables recovery.
pub fn default_recovery_budget() -> Duration {
    Duration::from_secs(
        std::env::var("GOOSE_PROVIDER_RETRY_TIMEOUT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(120),
    )
}

async fn before_deadline<F, D>(
    future: F,
    deadline: &mut Option<D>,
    output: &AtomicBool,
) -> Option<F::Output>
where
    F: Future,
    D: Future<Output = ()> + Unpin,
{
    if output.load(Ordering::Relaxed) {
        return Some(future.await);
    }
    let Some(deadline) = deadline else {
        return Some(future.await);
    };
    match futures::future::select(deadline, Box::pin(future)).await {
        Either::Left(((), future)) if output.load(Ordering::Relaxed) => Some(future.await),
        Either::Left(_) => None,
        Either::Right((result, _)) => Some(result),
    }
}

/// Retry temporary stream-creation and pre-output failures using one budget.
/// Progress notifications are user-visible only. Dropping the stream cancels
/// both requests and backoff. Nested adapters share the same recovery policy.
pub fn stream_with_recovery(
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    system: String,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    budget: Duration,
) -> MessageStream {
    let parent = OUTPUT_STARTED.try_with(Arc::clone).ok();
    let owns_recovery = parent.is_none() && !provider.manages_own_context();
    let scoped = owns_recovery || parent.is_some();
    let output = parent.unwrap_or_default();
    Box::pin(try_stream! {
        let config = RetryConfig::default().transient_only();
        let mut deadline = None;
        let mut last_error: Option<ProviderError> = None;
        let mut attempt = 0;
        loop {
            let request = provider.stream(&model_config, &system, &messages, &tools);
            let request = async {
                if scoped {
                    OUTPUT_STARTED.scope(output.clone(), request).await
                } else {
                    request.await
                }
            };
            let result = before_deadline(request, &mut deadline, &output)
                .await
                .ok_or_else(|| last_error.clone().expect("recovery starts after a failure"))?;
            let error = match result {
                Err(error) => error,
                Ok(mut stream) => loop {
                    let next = async {
                        if scoped {
                            OUTPUT_STARTED.scope(output.clone(), stream.next()).await
                        } else {
                            stream.next().await
                        }
                    };
                    let item = before_deadline(next, &mut deadline, &output)
                        .await
                        .ok_or_else(|| last_error.clone().expect("recovery starts after a failure"))?;
                    match item {
                        None => return,
                        Some(Err(error)) => break error,
                        Some(Ok((message, usage))) => {
                            if message.as_ref().is_some_and(|message| {
                                message.metadata.output_token_limit_reached
                                    || message.content.iter().any(|content| match content {
                                        MessageContent::SystemNotification(_) => false,
                                        MessageContent::Text(text) => !text.text.is_empty(),
                                        _ => true,
                                    })
                            }) {
                                output.store(true, Ordering::Relaxed);
                            }
                            yield (message, usage);
                        }
                    }
                },
            };
            if !owns_recovery || budget.is_zero() || output.load(Ordering::Relaxed)
                || !should_retry(&error, &config)
            {
                Err(error)?;
                return;
            }
            deadline.get_or_insert_with(|| Box::pin(sleep(budget)));
            attempt += 1;
            let delay = match &error {
                crate::errors::ProviderError::RateLimitExceeded { retry_delay: Some(delay), .. } => *delay,
                _ => config.delay_for_attempt(attempt),
            };
            last_error = Some(error.clone());
            yield (Some(Message::assistant()
                .with_system_notification(
                    SystemNotificationType::ProgressMessage,
                    format!("Provider request failed: {error}. Retrying in {:.1}s (attempt {attempt}).", delay.as_secs_f64()),
                )
                .with_visibility(true, false)), None);
            if before_deadline(sleep(delay), &mut deadline, &output).await.is_none() {
                Err(error)?;
            }
        }
    })
}
