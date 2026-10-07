use anyhow::Result;
use goose_providers::errors::ProviderError;
use regex::Regex;
use std::sync::Arc;

use async_stream::try_stream;
use futures::stream::StreamExt;
use serde_json::{json, Value};
use tracing::debug;

use super::super::agents::Agent;
use super::gen_ai_telemetry;
use crate::config::Config;
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::{fix_conversation, merge_consecutive_messages_for_request, Conversation};
#[cfg(test)]
use crate::providers::base::stream_from_single_message;
use crate::providers::base::{MessageStream, Provider};
use crate::providers::toolshim::{
    augment_message_with_selected_tool_interpreter, convert_tool_messages_to_text,
    modify_system_prompt_for_tool_json, sanitize_residual_markers,
};
use goose_providers::conversation::token_usage::{ProviderStats, ProviderUsage};
use goose_providers::model::ModelConfig;
use rmcp::model::Tool;
use tracing::warn;

async fn enhance_model_error(
    error: ProviderError,
    provider: &Arc<dyn Provider>,
    toolshim: bool,
) -> ProviderError {
    let ProviderError::RequestFailed(ref msg) = error else {
        return error;
    };

    let re = Regex::new(r"(?i)\b4\d{2}\b.*model|model.*\b4\d{2}\b").unwrap();
    if !re.is_match(msg) {
        return error;
    }

    let Ok(models) = provider.fetch_recommended_models(toolshim).await else {
        return error;
    };
    if models.is_empty() {
        return error;
    }

    ProviderError::RequestFailed(format!(
        "{}. Available models for this provider: {}",
        msg,
        models.join(", ")
    ))
}

fn coerce_value(s: &str, schema: &Value) -> Value {
    let type_str = schema.get("type");

    match type_str {
        Some(Value::String(t)) => match t.as_str() {
            "number" | "integer" => try_coerce_number(s),
            "boolean" => try_coerce_boolean(s),
            _ => Value::String(s.to_string()),
        },
        Some(Value::Array(types)) => {
            // Try each type in order
            for t in types {
                if let Value::String(type_name) = t {
                    match type_name.as_str() {
                        "number" | "integer" if s.parse::<f64>().is_ok() => {
                            return try_coerce_number(s)
                        }
                        "boolean" if matches!(s.to_lowercase().as_str(), "true" | "false") => {
                            return try_coerce_boolean(s)
                        }
                        _ => continue,
                    }
                }
            }
            Value::String(s.to_string())
        }
        _ => Value::String(s.to_string()),
    }
}

fn try_coerce_number(s: &str) -> Value {
    if let Ok(n) = s.parse::<f64>() {
        if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
            json!(n as i64)
        } else {
            json!(n)
        }
    } else {
        Value::String(s.to_string())
    }
}

fn try_coerce_boolean(s: &str) -> Value {
    match s.to_lowercase().as_str() {
        "true" => json!(true),
        "false" => json!(false),
        _ => Value::String(s.to_string()),
    }
}

pub(crate) fn coerce_tool_arguments(
    arguments: Option<serde_json::Map<String, Value>>,
    tool_schema: &Value,
) -> Option<serde_json::Map<String, Value>> {
    let args = arguments?;

    let properties = tool_schema.get("properties").and_then(|p| p.as_object())?;

    let mut coerced = serde_json::Map::new();

    for (key, value) in args.iter() {
        let coerced_value =
            if let (Value::String(s), Some(prop_schema)) = (value, properties.get(key)) {
                coerce_value(s, prop_schema)
            } else {
                value.clone()
            };
        coerced.insert(key.clone(), coerced_value);
    }

    Some(coerced)
}

pub(crate) async fn toolshim_postprocess(
    response: Message,
    toolshim_tools: &[Tool],
) -> Result<Message, ProviderError> {
    match augment_message_with_selected_tool_interpreter(response.clone(), toolshim_tools).await {
        Ok(message) => Ok(message),
        Err(e) => {
            warn!(
                "Toolshim augmentation failed, skipping tool augmentation: {}",
                e
            );
            Ok(sanitize_residual_markers(response))
        }
    }
}

/// Fill `usage.stats` timing fields measured by the stream wrapper, keeping any
/// values the provider already reported (e.g. MLX's own `elapsed_ms`).
fn fill_stream_timing(
    usage: &mut ProviderUsage,
    request_started: std::time::Instant,
    first_content_at: Option<std::time::Instant>,
) {
    let stats = usage.stats.get_or_insert_with(ProviderStats::default);
    if stats.time_to_first_token_ms.is_none() {
        if let Some(first) = first_content_at {
            stats.time_to_first_token_ms = Some((first - request_started).as_millis() as u64);
        }
    }
    if stats.elapsed_ms.is_none() {
        stats.elapsed_ms = Some(request_started.elapsed().as_millis() as u64);
    }
}

fn message_has_timing_content(message: &Message) -> bool {
    message
        .content
        .iter()
        .any(|content| !matches!(content, MessageContent::SystemNotification(_)))
}

fn is_mergeable_assistant_chunk(message: &Message) -> bool {
    message.role == rmcp::model::Role::Assistant
        && !message.content.is_empty()
        && message.content.iter().all(|content| {
            matches!(
                content,
                MessageContent::Text(_)
                    | MessageContent::Thinking(_)
                    | MessageContent::RedactedThinking(_)
            )
        })
}

impl Agent {}

pub(crate) fn prepare_inference_tools(
    mut tools: Vec<Tool>,
    code_execution_active: bool,
) -> Vec<Tool> {
    #[cfg(feature = "code-mode")]
    if code_execution_active {
        let disclosure_style =
            crate::agents::platform_extensions::code_execution::get_tool_disclosure();

        tools = tools
            .into_iter()
            .filter_map(|mut tool| match disclosure_style {
                pctx_code_mode::config::ToolDisclosure::Catalog
                | pctx_code_mode::config::ToolDisclosure::Filesystem => {
                    // in catalog & filesystem styles, progressive search is handled
                    // by pctx, so we want to omit all non-first-class extensions
                    // from the standard tool list
                    if crate::agents::extension_manager::get_tool_owner(&tool).is_some_and(
                        |owner| crate::agents::extension_manager::is_first_class_extension(&owner),
                    ) || crate::agents::extension_manager::get_tool_resource_uri(&tool).is_some()
                    {
                        Some(tool)
                    } else {
                        None
                    }
                }
                pctx_code_mode::config::ToolDisclosure::Sidecar => {
                    // in sidecar style there is no progressive search, just a way to chain tools
                    // together with typescript
                    // add output schema to description since many model providers drop the
                    // output schema when presenting tools to the model
                    let output_schema = tool
                        .output_schema
                        .as_ref()
                        .map(|schema| serde_json::json!(schema).to_string())
                        .unwrap_or("unknown".to_string());
                    let description =
                        format!("The successful return schema of this tool is:\n{output_schema}");
                    tool.description = Some(
                        tool.description
                            .map(|current| format!("{current}\n{description}"))
                            .unwrap_or(description)
                            .into(),
                    );
                    Some(tool)
                }
            })
            .collect();
    }

    #[cfg(not(feature = "code-mode"))]
    let _ = code_execution_active;

    // Filter out tools not visible to the model per MCP Apps visibility spec.
    // Tools with `_meta.ui.visibility` that doesn't include "model" are app-only.
    tools.retain(is_tool_visible_to_model);

    // Stable tool ordering is important for multi session prompt caching.
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

pub(crate) fn prepare_tools_for_provider(
    tools: Vec<Tool>,
    system_prompt: String,
    model_config: &ModelConfig,
) -> (Vec<Tool>, Vec<Tool>, String) {
    if model_config.toolshim {
        let system_prompt = modify_system_prompt_for_tool_json(&system_prompt, &tools);
        (Vec::new(), tools, system_prompt)
    } else {
        (tools, Vec::new(), system_prompt)
    }
}

#[tracing::instrument(
    skip(provider, model_config, session_id, system_prompt, messages, tools, toolshim_tools),
    fields(
        session.id = %session_id,
        gen_ai.conversation.id = %session_id,
        gen_ai.operation.name = "chat",
        gen_ai.provider.name = %provider.get_name(),
        gen_ai.request.model = %model_config.model_name,
        gen_ai.request.stream = true,
        gen_ai.request.temperature = tracing::field::Empty,
        gen_ai.request.max_tokens = tracing::field::Empty,
        gen_ai.response.model = tracing::field::Empty,
        gen_ai.response.finish_reasons = tracing::field::Empty,
        gen_ai.response.id = tracing::field::Empty,
        gen_ai.usage.input_tokens = tracing::field::Empty,
        gen_ai.usage.output_tokens = tracing::field::Empty,
        gen_ai.usage.cache_read.input_tokens = tracing::field::Empty,
        gen_ai.usage.cache_creation.input_tokens = tracing::field::Empty,
        gen_ai.input.messages = tracing::field::Empty,
        gen_ai.output.messages = tracing::field::Empty,
    )
)]
pub(crate) async fn stream_response_from_provider(
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    session_id: &str,
    system_prompt: &str,
    messages: &[Message],
    tools: &[Tool],
    toolshim_tools: &[Tool],
) -> Result<MessageStream, ProviderError> {
    let config = model_config.clone();

    let projected_messages =
        Conversation::new_unvalidated(messages.iter().cloned()).agent_visible_messages();
    let (filtered_messages, _) =
        fix_conversation(Conversation::new_unvalidated(projected_messages));
    let filtered_messages = Conversation::new_unvalidated(merge_consecutive_messages_for_request(
        filtered_messages.messages().clone(),
    ));

    // Convert tool messages to text if toolshim is enabled
    let messages_for_provider = if config.toolshim {
        convert_tool_messages_to_text(filtered_messages.messages())
    } else {
        filtered_messages
    };
    let span = tracing::Span::current();
    gen_ai_telemetry::record_request_params(&span, &model_config);
    let capture_message_content = gen_ai_telemetry::capture_message_content();
    if capture_message_content {
        let input_messages =
            gen_ai_telemetry::input_messages_json(messages_for_provider.messages());
        span.record("gen_ai.input.messages", input_messages.as_str());
    }

    // Clone owned data to move into the async stream
    let system_prompt = system_prompt.to_owned();
    let session_id = session_id.to_owned();
    let tools = tools.to_owned();
    let toolshim_tools = toolshim_tools.to_owned();
    let provider = provider.clone();

    // Capture errors during stream creation and return them as part of the stream
    // so they can be handled by the existing error handling logic in the agent
    let model_config =
        model_config.with_default_thinking_effort(Config::global().get_goose_thinking_effort());
    let request_started = std::time::Instant::now();
    debug!("WAITING_LLM_STREAM_START");
    let stream_result = crate::session_context::with_session_id(
        Some(session_id.clone()),
        provider.stream(
            &model_config,
            system_prompt.as_str(),
            messages_for_provider.messages(),
            &tools,
        ),
    )
    .await;
    debug!("WAITING_LLM_STREAM_END");

    // If there was an error creating the stream, return a stream that yields that error
    let mut stream = match stream_result {
        Ok(s) => s,
        Err(e) => {
            let enhanced_error = enhance_model_error(e, &provider, config.toolshim).await;
            // Return a stream that immediately yields the error
            // This allows the error to be caught by existing error handling in agent.rs
            return Ok(Box::pin(try_stream! {
                yield Err(enhanced_error)?;
            }));
        }
    };

    Ok(Box::pin(try_stream! {
        if !provider.manages_own_context() {
            let retry_config = provider.retry_config().transient_only();
            let mut attempts = 0;

            loop {
                match stream.next().await {
                    None => break,
                    Some(Ok(item)) => {
                        stream = Box::pin(
                            futures::stream::once(std::future::ready(Ok(item))).chain(stream),
                        );
                        break;
                    }
                    Some(Err(error))
                        if goose_providers::retry::should_retry(&error, &retry_config)
                            && attempts < retry_config.max_retries =>
                    {
                        attempts += 1;
                        let delay = match &error {
                            ProviderError::RateLimitExceeded {
                                retry_delay: Some(provider_delay),
                                ..
                            } => *provider_delay,
                            _ => retry_config.delay_for_attempt(attempts),
                        };
                        warn!(
                            "Provider stream failed before its first item, retrying ({}/{}): {:?}",
                            attempts, retry_config.max_retries, error
                        );

                        let skip_backoff = std::env::var("GOOSE_PROVIDER_SKIP_BACKOFF")
                            .unwrap_or_default()
                            .parse::<bool>()
                            .unwrap_or(false);
                        if !skip_backoff {
                            tokio::time::sleep(delay).await;
                        }

                        stream = match crate::session_context::with_session_id(
                            Some(session_id.clone()),
                            provider.stream(
                                &model_config,
                                system_prompt.as_str(),
                                messages_for_provider.messages(),
                                &tools,
                            ),
                        )
                        .await
                        {
                            Ok(stream) => stream,
                            Err(error) => {
                                Err(enhance_model_error(error, &provider, config.toolshim).await)?
                            }
                        };
                    }
                    Some(Err(error)) => Err(error)?,
                }
            }
        }

        if config.toolshim {
            // Toolshim mode: accumulate the full response before processing
            // so that tool-use markers spanning multiple chunks are detected
            // and stripped before any output reaches the UI.
            let mut accumulated_message: Option<Message> = None;
            let mut final_usage: Option<ProviderUsage> = None;
            let mut first_content_at: Option<std::time::Instant> = None;

            while let Some(result) = stream.next().await {
                let (msg_opt, usage_opt) = result?;

                if let Some(msg) = msg_opt {
                    if first_content_at.is_none() && message_has_timing_content(&msg) {
                        first_content_at = Some(std::time::Instant::now());
                    }
                    accumulated_message = Some(match accumulated_message {
                        Some(mut prev) => {
                            for new_content in msg.content {
                                match (&mut prev.content.last_mut(), &new_content) {
                                    (
                                        Some(MessageContent::Text(last_text)),
                                        MessageContent::Text(new_text),
                                    ) if last_text.annotations.as_ref().and_then(|a| a.audience.as_ref())
                                        == new_text.annotations.as_ref().and_then(|a| a.audience.as_ref()) => {
                                        last_text.text.push_str(&new_text.text);
                                    }
                                    _ => {
                                        prev.content.push(new_content);
                                    }
                                }
                            }
                            prev
                        }
                        None => msg,
                    });
                }

                if let Some(usage) = usage_opt {
                    final_usage = Some(usage);
                }

                // Yield empty item so the agent loop can check cancellation
                yield (None, None);
            }

            // The toolshim interpreter call below must not count toward elapsed time.
            if let Some(usage) = final_usage.as_mut() {
                fill_stream_timing(usage, request_started, first_content_at);
                gen_ai_telemetry::record_provider_usage(&span, usage);
            }

            if let Some(msg) = accumulated_message {
                let processed = toolshim_postprocess(msg, &toolshim_tools)
                    .await?
                    .with_generated_id_if_missing();
                if capture_message_content {
                    let output_messages = gen_ai_telemetry::output_message_json(&processed);
                    span.record("gen_ai.output.messages", output_messages.as_str());
                }
                yield (Some(processed), final_usage);
            } else if final_usage.is_some() {
                // Preserve usage-only responses (no message content)
                yield (None, final_usage);
            }
        } else {
            let mut first_content_at: Option<std::time::Instant> = None;
            let mut active_mergeable_assistant_id: Option<String> = None;
            let mut output_message: Option<Message> = None;
            while let Some(result) = stream.next().await {
                let (message, mut usage) = result?;

                if first_content_at.is_none()
                    && message.as_ref().is_some_and(message_has_timing_content)
                {
                    first_content_at = Some(std::time::Instant::now());
                }
                if let Some(usage) = usage.as_mut() {
                    fill_stream_timing(usage, request_started, first_content_at);
                    gen_ai_telemetry::record_provider_usage(&span, usage);
                }
                if capture_message_content {
                    if let Some(message) = message.as_ref() {
                        gen_ai_telemetry::append_message(&mut output_message, message);
                    }
                }

                let message = message.map(|message| {
                    if message.id.is_some() {
                        active_mergeable_assistant_id = None;
                        message
                    } else if is_mergeable_assistant_chunk(&message) {
                        let id = active_mergeable_assistant_id
                            .get_or_insert_with(|| format!("msg_{}", uuid::Uuid::new_v4()))
                            .clone();
                        message.with_id(id)
                    } else {
                        active_mergeable_assistant_id = None;
                        message.with_generated_id()
                    }
                });

                yield (message, usage);
            }
            if let Some(output_message) = output_message {
                let output_messages = gen_ai_telemetry::output_message_json(&output_message);
                span.record("gen_ai.output.messages", output_messages.as_str());
            }
        }
    }))
}

impl Agent {}

/// Check whether a tool should be callable by an app based on MCP Apps visibility metadata.
///
/// Per the MCP Apps spec (2026-01-26), if `_meta.ui.visibility` is present and does not
/// include `"app"`, the tool is model-only and must not be callable by app UIs.
/// If the field is absent, the tool defaults to visible to both model and app.
pub fn is_tool_visible_to_app(tool: &Tool) -> bool {
    let Some(meta) = &tool.meta else {
        return true;
    };
    let Some(ui) = meta.0.get("ui") else {
        return true;
    };
    let Some(visibility) = ui.get("visibility") else {
        return true;
    };
    let Some(arr) = visibility.as_array() else {
        return false;
    };
    arr.iter().any(|v| v.as_str() == Some("app"))
}

/// Check whether a tool should be visible to the model based on MCP Apps visibility metadata.
///
/// Per the MCP Apps spec (2026-01-26), tools may declare `_meta.ui.visibility` as an array
/// of `"model"` and/or `"app"`. If the field is absent, the tool defaults to visible to both.
/// If present and does not include `"model"`, the tool is app-only and must not be sent to the LLM.
pub fn is_tool_visible_to_model(tool: &Tool) -> bool {
    let Some(meta) = &tool.meta else {
        return true;
    };
    let Some(ui) = meta.0.get("ui") else {
        return true;
    };
    let Some(visibility) = ui.get("visibility") else {
        return true;
    };
    let Some(arr) = visibility.as_array() else {
        return false;
    };
    arr.iter().any(|v| v.as_str() == Some("model"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::gen_ai_telemetry::{self, test_support::SpanFieldCapture};
    use crate::conversation::message::{Message, SystemNotificationType};
    use crate::providers::base::Provider;
    use async_trait::async_trait;
    use goose_providers::conversation::token_usage::{ProviderStats, ProviderUsage, Usage};
    use goose_providers::model::ModelConfig;
    use rmcp::model::{Annotations, Role, TextContent};
    use rmcp::object;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[derive(Clone)]
    struct GenAiTracingProvider;

    #[async_trait]
    impl Provider for GenAiTracingProvider {
        fn get_name(&self) -> &str {
            "test-provider"
        }

        async fn stream(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            let usage = ProviderUsage::new(
                "resolved-model".to_string(),
                Usage::new(Some(11), Some(7), Some(18)).with_cache_tokens(Some(3), Some(2)),
            );
            Ok(Box::pin(futures::stream::iter(vec![
                Ok((Some(Message::assistant().with_text("hello ")), None)),
                Ok((Some(Message::assistant().with_text("world")), Some(usage))),
            ])))
        }
    }

    #[derive(Clone)]
    struct CapturingProvider {
        messages: Arc<Mutex<Vec<Message>>>,
    }

    #[async_trait]
    impl Provider for CapturingProvider {
        fn get_name(&self) -> &str {
            "capturing"
        }

        async fn stream(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            *self.messages.lock().unwrap() = messages.to_vec();
            let message = Message::assistant().with_text("ok");
            let usage = ProviderUsage::new("capturing".to_string(), Usage::default());
            Ok(stream_from_single_message(message, usage))
        }
    }

    #[tokio::test]
    async fn provider_stream_records_gen_ai_span_attributes() {
        use futures::StreamExt;
        use goose_test_support::otel::clear_otel_env;

        let _env = clear_otel_env(&[(gen_ai_telemetry::CAPTURE_MESSAGE_CONTENT_ENV, "true")]);
        let capture = SpanFieldCapture::new("stream_response_from_provider");
        let _subscriber = capture.clone().set_default();
        let messages = vec![Message::user().with_text("Say hello")];

        let mut stream = stream_response_from_provider(
            Arc::new(GenAiTracingProvider),
            ModelConfig::new("requested-model"),
            "test-session",
            "system",
            &messages,
            &[],
            &[],
        )
        .await
        .unwrap();
        while let Some(item) = stream.next().await {
            item.unwrap();
        }
        drop(stream);

        let fields = capture.fields();
        assert_eq!(fields["gen_ai.operation.name"], "chat");
        assert_eq!(fields["gen_ai.provider.name"], "test-provider");
        assert_eq!(fields["gen_ai.request.model"], "requested-model");
        assert_eq!(fields["gen_ai.request.stream"], true);
        assert_eq!(fields["gen_ai.conversation.id"], "test-session");
        assert_eq!(fields["gen_ai.response.model"], "resolved-model");
        assert_eq!(fields["gen_ai.usage.input_tokens"], 11);
        assert_eq!(fields["gen_ai.usage.output_tokens"], 7);
        assert_eq!(fields["gen_ai.usage.cache_read.input_tokens"], 3);
        assert_eq!(fields["gen_ai.usage.cache_creation.input_tokens"], 2);

        let input: Value =
            serde_json::from_str(fields["gen_ai.input.messages"].as_str().unwrap()).unwrap();
        assert_eq!(input[0]["parts"][0]["content"], "Say hello");

        let output: Value =
            serde_json::from_str(fields["gen_ai.output.messages"].as_str().unwrap()).unwrap();
        assert_eq!(output[0]["finish_reason"], "stop");
        assert_eq!(output[0]["parts"][0]["content"], "hello world");
    }

    #[tokio::test]
    async fn provider_input_drops_rows_empty_after_agent_projection() {
        let user_only = TextContent::new("user-only ACP output")
            .with_annotations(Annotations::default().with_audience(vec![Role::User]));
        let messages = vec![
            Message::assistant().with_content(MessageContent::Text(user_only)),
            Message::user().with_text("current request"),
        ];
        let captured = Arc::new(Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingProvider {
            messages: captured.clone(),
        });

        let _stream = stream_response_from_provider(
            provider,
            ModelConfig::new("test-model"),
            "test-session",
            "system",
            &messages,
            &[],
            &[],
        )
        .await
        .unwrap();

        let captured = captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].role, Role::User);
        assert_eq!(captured[0].as_concat_text(), "current request");
    }

    #[tokio::test]
    async fn provider_input_refixes_roles_after_agent_projection() {
        let user_only = TextContent::new("hidden separator")
            .with_annotations(Annotations::default().with_audience(vec![Role::User]));
        let messages = vec![
            Message::user().with_text("first request"),
            Message::assistant().with_content(MessageContent::Text(user_only)),
            Message::user().with_text("second request"),
        ];
        let captured = Arc::new(Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingProvider {
            messages: captured.clone(),
        });

        let _stream = stream_response_from_provider(
            provider,
            ModelConfig::new("test-model"),
            "test-session",
            "system",
            &messages,
            &[],
            &[],
        )
        .await
        .unwrap();

        let captured = captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].role, Role::User);
        assert_eq!(
            captured[0].as_concat_text(),
            "first request\nsecond request"
        );
        assert!(!captured[0].as_concat_text().contains("hidden separator"));
    }

    #[tokio::test]
    async fn provider_input_refixes_tool_result_emptied_by_agent_projection() {
        let user_only_result = rmcp::model::ContentBlock::Text(
            TextContent::new("hidden result")
                .with_annotations(Annotations::default().with_audience(vec![Role::User])),
        );
        let messages = vec![
            Message::user().with_text("run the tool"),
            Message::assistant().with_tool_request(
                "tool-1",
                Ok(rmcp::model::CallToolRequestParams::new("test_tool")),
            ),
            Message::user().with_tool_response(
                "tool-1",
                Ok(rmcp::model::CallToolResult::success(vec![user_only_result])),
            ),
        ];
        let captured = Arc::new(Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingProvider {
            messages: captured.clone(),
        });

        let _stream = stream_response_from_provider(
            provider,
            ModelConfig::new("test-model"),
            "test-session",
            "system",
            &messages,
            &[],
            &[],
        )
        .await
        .unwrap();

        let captured = captured.lock().unwrap();
        let tool_response = captured
            .iter()
            .flat_map(|message| &message.content)
            .find_map(|content| match content {
                MessageContent::ToolResponse(response) => Some(response),
                _ => None,
            })
            .expect("projected tool response should remain paired");
        let result = tool_response
            .tool_result
            .as_ref()
            .expect("tool response should remain successful");
        assert_eq!(result.content.len(), 1);
        assert_eq!(
            result.content[0]
                .as_text()
                .expect("placeholder should be text")
                .text,
            "(empty result)"
        );
    }

    #[tokio::test]
    async fn test_stream_error_propagation() {
        use futures::StreamExt;

        type StreamItem = Result<(Option<Message>, Option<ProviderUsage>), ProviderError>;
        let stream = futures::stream::iter(vec![
            Ok((Some(Message::assistant().with_text("chunk1")), None)),
            Ok((Some(Message::assistant().with_text("chunk2")), None)),
            Err(ProviderError::RequestFailed(
                "simulated stream error".to_string(),
            )),
        ] as Vec<StreamItem>);

        let mut pinned = Box::pin(stream);
        let mut results = Vec::new();
        let mut error_seen = false;

        while let Some(result) = pinned.next().await {
            match result {
                Ok((message, _usage)) => {
                    if let Some(msg) = message {
                        results.push(msg.as_concat_text());
                    }
                }
                Err(_e) => {
                    error_seen = true;
                    break;
                }
            }
        }

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], "chunk1");
        assert_eq!(results[1], "chunk2");
        assert!(
            error_seen,
            "Error should have been propagated, not silently ignored"
        );
    }

    struct MixedMessageIdStreamProvider;

    #[async_trait]
    impl Provider for MixedMessageIdStreamProvider {
        fn get_name(&self) -> &str {
            "mixed-message-id-stream"
        }

        async fn stream(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            type StreamItem = Result<(Option<Message>, Option<ProviderUsage>), ProviderError>;
            Ok(Box::pin(futures::stream::iter(vec![
                Ok((Some(Message::assistant().with_text("Hel")), None)),
                Ok((Some(Message::assistant().with_text("lo")), None)),
                Ok((
                    Some(Message::assistant().with_action_required(
                        "permission-a",
                        "shell".to_string(),
                        object!({}),
                        Some("Approve A?".to_string()),
                    )),
                    None,
                )),
                Ok((
                    Some(Message::assistant().with_action_required(
                        "permission-b",
                        "shell".to_string(),
                        object!({}),
                        Some("Approve B?".to_string()),
                    )),
                    None,
                )),
                Ok((
                    Some(
                        Message::assistant()
                            .with_id("provider-id")
                            .with_text("done"),
                    ),
                    None,
                )),
                Ok((Some(Message::assistant().with_text("next")), None)),
                Ok((Some(Message::assistant().with_text("a")), None)),
                Ok((
                    Some(Message::assistant().with_tool_request(
                        "tool-t",
                        Ok(rmcp::model::CallToolRequestParams::new("test_tool")),
                    )),
                    None,
                )),
                Ok((Some(Message::assistant().with_text("b")), None)),
            ]
                as Vec<StreamItem>)))
        }
    }

    #[tokio::test]
    async fn normal_provider_stream_groups_only_contiguous_mergeable_chunks() -> anyhow::Result<()>
    {
        let provider = Arc::new(MixedMessageIdStreamProvider);
        let mut stream = stream_response_from_provider(
            provider,
            ModelConfig::new("test-model"),
            "test-session",
            "system",
            &[Message::user().with_text("hi")],
            &[],
            &[],
        )
        .await?;

        let mut messages = Vec::new();
        while let Some(item) = stream.next().await {
            let (message, usage) = item?;
            assert!(usage.is_none());
            if let Some(message) = message {
                messages.push(message);
            }
        }

        assert_eq!(messages.len(), 9);

        let ids = messages
            .iter()
            .map(|message| {
                message
                    .id
                    .as_deref()
                    .expect("streamed provider message should have an ID")
            })
            .collect::<Vec<_>>();

        assert_eq!(messages[0].as_concat_text(), "Hel");
        assert_eq!(messages[1].as_concat_text(), "lo");
        assert_eq!(ids[0], ids[1]);
        assert!(ids[0].starts_with("msg_"));

        assert!(matches!(
            messages[2].content.first(),
            Some(MessageContent::ActionRequired(_))
        ));
        assert!(matches!(
            messages[3].content.first(),
            Some(MessageContent::ActionRequired(_))
        ));
        assert_ne!(ids[2], ids[3]);
        assert_ne!(ids[2], ids[0]);
        assert_ne!(ids[3], ids[0]);

        assert_eq!(messages[4].as_concat_text(), "done");
        assert_eq!(ids[4], "provider-id");

        assert_eq!(messages[5].as_concat_text(), "next");
        assert_eq!(messages[6].as_concat_text(), "a");
        assert_ne!(ids[5], ids[0]);
        assert_eq!(ids[5], ids[6]);

        assert!(matches!(
            messages[7].content.first(),
            Some(MessageContent::ToolRequest(_))
        ));
        assert_ne!(ids[7], ids[5]);

        assert_eq!(messages[8].as_concat_text(), "b");
        assert_ne!(ids[8], ids[5]);
        assert_ne!(ids[8], ids[7]);

        Ok(())
    }

    struct ToolshimMessageIdProvider {
        messages: Vec<Message>,
    }

    #[async_trait]
    impl Provider for ToolshimMessageIdProvider {
        fn get_name(&self) -> &str {
            "toolshim-message-id"
        }

        async fn stream(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            type StreamItem = Result<(Option<Message>, Option<ProviderUsage>), ProviderError>;
            let items = self
                .messages
                .iter()
                .cloned()
                .map(|message| Ok((Some(message), None)))
                .collect::<Vec<StreamItem>>();
            Ok(Box::pin(futures::stream::iter(items)))
        }
    }

    #[tokio::test]
    async fn toolshim_provider_stream_assigns_missing_message_id() -> anyhow::Result<()> {
        let provider = Arc::new(ToolshimMessageIdProvider {
            messages: vec![
                Message::assistant().with_text("Hel"),
                Message::assistant().with_text("lo"),
            ],
        });
        let mut stream = stream_response_from_provider(
            provider,
            ModelConfig::new("test-model").with_toolshim(true),
            "test-session",
            "system",
            &[Message::user().with_text("hi")],
            &[],
            &[],
        )
        .await?;

        let mut messages = Vec::new();
        while let Some(item) = stream.next().await {
            let (message, usage) = item?;
            assert!(usage.is_none());
            if let Some(message) = message {
                messages.push(message);
            }
        }

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].as_concat_text(), "Hello");
        let id = messages[0]
            .id
            .as_deref()
            .expect("toolshim provider message should have an ID");
        assert!(id.starts_with("msg_"));

        Ok(())
    }

    #[tokio::test]
    async fn toolshim_provider_stream_preserves_provider_message_id() -> anyhow::Result<()> {
        let provider = Arc::new(ToolshimMessageIdProvider {
            messages: vec![Message::assistant()
                .with_id("provider-toolshim-id")
                .with_text("hello")],
        });
        let mut stream = stream_response_from_provider(
            provider,
            ModelConfig::new("test-model").with_toolshim(true),
            "test-session",
            "system",
            &[Message::user().with_text("hi")],
            &[],
            &[],
        )
        .await?;

        let mut messages = Vec::new();
        while let Some(item) = stream.next().await {
            let (message, usage) = item?;
            assert!(usage.is_none());
            if let Some(message) = message {
                messages.push(message);
            }
        }

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].as_concat_text(), "hello");
        assert_eq!(messages[0].id.as_deref(), Some("provider-toolshim-id"));

        Ok(())
    }

    fn make_tool_with_meta(meta_json: Option<serde_json::Value>) -> Tool {
        let mut tool = Tool::new("test_tool", "a test tool", object!({ "type": "object" }));
        if let Some(v) = meta_json {
            let obj = v.as_object().unwrap().clone();
            tool = tool.with_meta(rmcp::model::MetaObject(obj));
        }
        tool
    }

    #[test]
    fn test_tool_visible_when_no_meta() {
        let tool = make_tool_with_meta(None);
        assert!(is_tool_visible_to_model(&tool));
    }

    #[test]
    fn test_tool_visible_when_meta_has_no_ui() {
        let tool = make_tool_with_meta(Some(serde_json::json!({"other": "stuff"})));
        assert!(is_tool_visible_to_model(&tool));
    }

    #[test]
    fn test_tool_visible_when_ui_has_no_visibility() {
        let tool = make_tool_with_meta(Some(
            serde_json::json!({"ui": {"resourceUri": "ui://foo/bar"}}),
        ));
        assert!(is_tool_visible_to_model(&tool));
    }

    #[test]
    fn test_tool_visible_when_visibility_includes_model() {
        let tool = make_tool_with_meta(Some(
            serde_json::json!({"ui": {"visibility": ["model", "app"]}}),
        ));
        assert!(is_tool_visible_to_model(&tool));
    }

    #[test]
    fn test_tool_visible_when_visibility_is_model_only() {
        let tool = make_tool_with_meta(Some(serde_json::json!({"ui": {"visibility": ["model"]}})));
        assert!(is_tool_visible_to_model(&tool));
    }

    #[test]
    fn test_tool_hidden_when_visibility_is_app_only() {
        let tool = make_tool_with_meta(Some(serde_json::json!({"ui": {"visibility": ["app"]}})));
        assert!(!is_tool_visible_to_model(&tool));
    }

    #[test]
    fn test_tool_hidden_when_visibility_is_empty() {
        let tool = make_tool_with_meta(Some(serde_json::json!({"ui": {"visibility": []}})));
        assert!(!is_tool_visible_to_model(&tool));
    }

    #[test]
    fn test_tool_hidden_when_visibility_is_not_array() {
        for visibility in [
            serde_json::json!("app"),
            serde_json::json!("model"),
            serde_json::json!({"model": true}),
            serde_json::Value::Null,
        ] {
            let tool =
                make_tool_with_meta(Some(serde_json::json!({"ui": {"visibility": visibility}})));
            assert!(!is_tool_visible_to_model(&tool));
        }
    }

    #[test]
    fn test_app_visible_when_no_meta() {
        let tool = make_tool_with_meta(None);
        assert!(is_tool_visible_to_app(&tool));
    }

    #[test]
    fn test_app_visible_when_visibility_includes_app() {
        let tool = make_tool_with_meta(Some(
            serde_json::json!({"ui": {"visibility": ["model", "app"]}}),
        ));
        assert!(is_tool_visible_to_app(&tool));
    }

    #[test]
    fn test_app_visible_when_visibility_is_app_only() {
        let tool = make_tool_with_meta(Some(serde_json::json!({"ui": {"visibility": ["app"]}})));
        assert!(is_tool_visible_to_app(&tool));
    }

    #[test]
    fn test_app_hidden_when_visibility_is_model_only() {
        let tool = make_tool_with_meta(Some(serde_json::json!({"ui": {"visibility": ["model"]}})));
        assert!(!is_tool_visible_to_app(&tool));
    }

    #[test]
    fn test_app_hidden_when_visibility_is_empty() {
        let tool = make_tool_with_meta(Some(serde_json::json!({"ui": {"visibility": []}})));
        assert!(!is_tool_visible_to_app(&tool));
    }

    #[test]
    fn test_app_hidden_when_visibility_is_not_array() {
        let tool = make_tool_with_meta(Some(serde_json::json!({"ui": {"visibility": "model"}})));
        assert!(!is_tool_visible_to_app(&tool));
    }

    fn usage_with_stats(stats: Option<ProviderStats>) -> ProviderUsage {
        let mut usage = ProviderUsage::new("mock".to_string(), Usage::default());
        usage.stats = stats;
        usage
    }

    #[test]
    fn message_has_timing_content_ignores_system_notification_only_messages() {
        let message = Message::assistant().with_system_notification(
            SystemNotificationType::ProgressMessage,
            "Loading local model test-model...",
        );

        assert!(!message_has_timing_content(&message));
    }

    #[test]
    fn message_has_timing_content_counts_user_visible_messages() {
        let text_message = Message::assistant().with_text("hello");
        let mixed_message = Message::assistant()
            .with_system_notification(SystemNotificationType::ProgressMessage, "Loading...")
            .with_text("ready");

        assert!(message_has_timing_content(&text_message));
        assert!(message_has_timing_content(&mixed_message));
    }

    #[test]
    fn fill_stream_timing_fills_both_fields_when_stats_absent() {
        let request_started = Instant::now() - Duration::from_millis(100);
        let first_content_at = Some(request_started + Duration::from_millis(40));
        let mut usage = usage_with_stats(None);

        fill_stream_timing(&mut usage, request_started, first_content_at);

        let stats = usage.stats.expect("stats must be created when absent");
        assert_eq!(stats.time_to_first_token_ms, Some(40));
        let elapsed = stats.elapsed_ms.expect("elapsed_ms must be filled");
        assert!(
            elapsed >= 100,
            "elapsed_ms ({elapsed}) must cover the full request duration"
        );
        assert!(stats.time_to_first_token_ms.unwrap() <= elapsed);
    }

    #[test]
    fn fill_stream_timing_preserves_provider_reported_values() {
        let request_started = Instant::now() - Duration::from_millis(100);
        let first_content_at = Some(request_started + Duration::from_millis(25));
        let mut usage = usage_with_stats(Some(ProviderStats {
            elapsed_ms: Some(7),
            time_to_first_token_ms: Some(3),
            output_tokens: Some(42),
            ..Default::default()
        }));

        fill_stream_timing(&mut usage, request_started, first_content_at);

        let stats = usage.stats.expect("stats must survive");
        assert_eq!(
            stats.elapsed_ms,
            Some(7),
            "provider-reported elapsed_ms (e.g. MLX) must not be overwritten"
        );
        assert_eq!(
            stats.time_to_first_token_ms,
            Some(3),
            "provider-reported TTFT must not be overwritten"
        );
        assert_eq!(
            stats.output_tokens,
            Some(42),
            "unrelated provider stats must survive the fill"
        );
    }

    #[test]
    fn fill_stream_timing_without_first_content_leaves_ttft_unset() {
        let request_started = Instant::now() - Duration::from_millis(100);
        let mut usage = usage_with_stats(None);

        fill_stream_timing(&mut usage, request_started, None);

        let stats = usage.stats.expect("stats must be created when absent");
        assert_eq!(
            stats.time_to_first_token_ms, None,
            "no content chunk observed means no TTFT"
        );
        assert!(stats.elapsed_ms.expect("elapsed_ms must be filled") >= 100);
    }

    type TestStreamItem = Result<(Option<Message>, Option<ProviderUsage>), ProviderError>;

    struct SequencedProvider {
        responses: Mutex<std::collections::VecDeque<Result<Vec<TestStreamItem>, ProviderError>>>,
        calls: Arc<std::sync::atomic::AtomicUsize>,
        manages_context: bool,
    }

    impl SequencedProvider {
        fn new(responses: Vec<Result<Vec<TestStreamItem>, ProviderError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                manages_context: false,
            }
        }

        fn managing_context(mut self) -> Self {
            self.manages_context = true;
            self
        }
    }

    #[async_trait]
    impl Provider for SequencedProvider {
        fn get_name(&self) -> &str {
            "sequenced"
        }

        fn retry_config(&self) -> goose_providers::retry::RetryConfig {
            goose_providers::retry::RetryConfig::new(2, 0, 1.0, 0)
        }

        fn manages_own_context(&self) -> bool {
            self.manages_context
        }

        async fn stream(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let response = self.responses.lock().unwrap().pop_front().unwrap();
            response.map(|items| Box::pin(futures::stream::iter(items)) as MessageStream)
        }
    }

    fn successful_item() -> TestStreamItem {
        Ok((Some(Message::assistant().with_text("ok")), None))
    }

    fn transient_error() -> ProviderError {
        ProviderError::NetworkError("stream closed".into())
    }

    async fn stream_for_test(provider: Arc<dyn Provider>) -> MessageStream {
        stream_response_from_provider(
            provider,
            ModelConfig::new("test-model"),
            "session",
            "system",
            &[Message::user().with_text("hi")],
            &[],
            &[],
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn first_item_transient_error_retries() {
        let provider = Arc::new(SequencedProvider::new(vec![
            Ok(vec![Err(transient_error())]),
            Ok(vec![successful_item()]),
        ]));
        let calls = provider.calls.clone();
        let mut stream = stream_for_test(provider).await;

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
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn first_item_non_transient_error_does_not_retry() {
        let provider = Arc::new(SequencedProvider::new(vec![Ok(vec![Err(
            ProviderError::ContextLengthExceeded("too long".into()),
        )])]));
        let calls = provider.calls.clone();
        let mut stream = stream_for_test(provider).await;

        assert!(matches!(
            stream.next().await.unwrap(),
            Err(ProviderError::ContextLengthExceeded(_))
        ));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn first_item_retry_exhaustion_returns_last_error() {
        let provider = Arc::new(SequencedProvider::new(vec![
            Ok(vec![Err(transient_error())]),
            Ok(vec![Err(transient_error())]),
            Ok(vec![Err(transient_error())]),
        ]));
        let calls = provider.calls.clone();
        let mut stream = stream_for_test(provider).await;

        assert!(matches!(
            stream.next().await.unwrap(),
            Err(ProviderError::NetworkError(_))
        ));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn provider_managing_context_does_not_retry() {
        let provider = Arc::new(
            SequencedProvider::new(vec![Ok(vec![Err(transient_error())])]).managing_context(),
        );
        let calls = provider.calls.clone();
        let mut stream = stream_for_test(provider).await;

        assert!(matches!(
            stream.next().await.unwrap(),
            Err(ProviderError::NetworkError(_))
        ));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn replacement_stream_creation_error_does_not_retry() {
        let provider = Arc::new(SequencedProvider::new(vec![
            Ok(vec![Err(transient_error())]),
            Err(ProviderError::ServerError("unavailable".into())),
        ]));
        let calls = provider.calls.clone();
        let mut stream = stream_for_test(provider).await;

        assert!(matches!(
            stream.next().await.unwrap(),
            Err(ProviderError::ServerError(_))
        ));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn error_after_first_item_does_not_retry() {
        let provider = Arc::new(SequencedProvider::new(vec![Ok(vec![
            successful_item(),
            Err(transient_error()),
        ])]));
        let calls = provider.calls.clone();
        let mut stream = stream_for_test(provider).await;

        assert!(stream.next().await.unwrap().is_ok());
        assert!(matches!(
            stream.next().await.unwrap(),
            Err(ProviderError::NetworkError(_))
        ));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn empty_stream_is_not_retried() {
        let provider = Arc::new(SequencedProvider::new(vec![Ok(vec![])]));
        let calls = provider.calls.clone();
        let mut stream = stream_for_test(provider).await;

        assert!(stream.next().await.is_none());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    struct PendingProvider;

    #[async_trait]
    impl Provider for PendingProvider {
        fn get_name(&self) -> &str {
            "pending"
        }

        async fn stream(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            Ok(Box::pin(futures::stream::pending()))
        }
    }

    #[tokio::test]
    async fn pending_first_item_does_not_block_stream_creation() {
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            stream_for_test(Arc::new(PendingProvider)),
        )
        .await;

        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod duplicate_tool_tests {}
