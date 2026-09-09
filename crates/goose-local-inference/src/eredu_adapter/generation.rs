use super::{error, settings::generation_settings};
use crate::model::{ModelSettings, ToolCallingMode};
use crate::tool_emulation::{
    build_emulator_tool_description, load_tiny_model_prompt, message_for_emulator_action,
    StreamingEmulatorParser, CODE_EXECUTION_TOOL,
};
use crate::StreamSender;
use eredu::api::{
    LoadedModel, PlannedModel, PreparedChatGenerationRequest, PreparedChatInput,
    PreparedChatSpeculativeGenerationRequest, TextModelError,
};
use eredu::runtime::chat::{
    ChatTemplateRequest, NativeToolSupport, ParallelToolCallPolicy, PreparedChat, SemanticSupport,
    ToolChoice,
};
use eredu_core::{
    AdmissionRequest, AdmissionResult, GenerationCancellationToken, GenerationOutput,
    ModelCapabilityBackend, SemanticEvent, SpeculativeGenerationBackend, SpeculativeStats,
    TextSamplingStrategy,
};
use goose_provider_types::conversation::{
    message::Message,
    token_usage::{DraftStats, ProviderStats, ProviderUsage, Usage},
};
use goose_provider_types::{errors::ProviderError, model::ModelConfig, thinking::ThinkingEffort};
use rmcp::model::{CallToolRequestParams, Tool};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Clone)]
pub struct Request {
    pub model: ModelConfig,
    pub settings: ModelSettings,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    pub message_id: String,
    pub model_load_ms: Option<u64>,
}

fn chat_request(request: &Request, emulated: bool) -> Result<ChatTemplateRequest, ProviderError> {
    let system = if emulated {
        format!(
            "{}\n\n{}{}",
            crate::provider_utils::filter_extensions_from_system_prompt(&request.system),
            load_tiny_model_prompt(),
            build_emulator_tool_description(
                &request.tools,
                request
                    .tools
                    .iter()
                    .any(|tool| tool.name == CODE_EXECUTION_TOOL)
            )
        )
    } else {
        request.system.clone()
    };
    let visible_messages: Vec<_> = request
        .messages
        .iter()
        .filter(|message| message.is_agent_visible())
        .cloned()
        .collect();
    let encoded = if emulated {
        crate::build_openai_text_messages_json(&system, &visible_messages, None)
    } else {
        crate::build_openai_messages_json(&system, &visible_messages, None)
    };
    let mut messages: Vec<Value> = serde_json::from_str(&encoded).map_err(error)?;
    // Templates consume structured argument objects, while OpenAI's wire format uses strings.
    for message in &mut messages {
        if let Some(calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) {
            for call in calls {
                if let Some(arguments) = call.pointer_mut("/function/arguments") {
                    if let Some(encoded) = arguments.as_str() {
                        *arguments = serde_json::from_str(encoded).map_err(error)?;
                    }
                }
            }
        }
    }
    let tools = if emulated {
        Vec::new()
    } else {
        request
            .tools
            .iter()
            .map(|tool| {
                let mut function = json!({"name": tool.name, "parameters": tool.input_schema});
                if let Some(description) = &tool.description {
                    function["description"] = json!(description);
                }
                json!({"type": "function", "function": function})
            })
            .collect()
    };
    Ok(ChatTemplateRequest {
        messages,
        tool_choice: if tools.is_empty() {
            ToolChoice::None
        } else {
            ToolChoice::Auto
        },
        tools,
        parallel_tool_calls: if request.model.request_param("parallel_tool_calls") == Some(false) {
            ParallelToolCallPolicy::Disabled
        } else {
            ParallelToolCallPolicy::Enabled { max_calls: None }
        },
        enable_thinking: request
            .model
            .request_param("enable_thinking")
            .or(request.settings.enable_thinking),
        reasoning_effort: request.model.request_param("reasoning_effort"),
        extra_template_kwargs: request
            .model
            .request_param("chat_template_kwargs")
            .unwrap_or_default(),
        add_generation_prompt: true,
        ..Default::default()
    })
}

fn alternating_text_messages(messages: &[Value]) -> Option<Vec<Value>> {
    let mut adapted: Vec<Value> = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        let object = message.as_object()?;
        if object.keys().any(|key| key != "role" && key != "content") {
            return None;
        }
        let role = match message["role"].as_str()? {
            "system" if index == 0 => "user",
            role @ ("user" | "assistant") => role,
            _ => return None,
        };
        let content = message["content"].as_str()?;
        if let Some(previous) = adapted
            .last_mut()
            .filter(|previous| previous["role"] == role)
        {
            previous["content"] = json!(format!("{}\n\n{content}", previous["content"].as_str()?));
        } else {
            adapted.push(json!({"role": role, "content": content}));
        }
    }
    Some(adapted)
}

fn prepare_chat<B: eredu_core::TextGenerationBackend>(
    model: &mut LoadedModel<B>,
    mut chat: ChatTemplateRequest,
    effort: Option<ThinkingEffort>,
) -> Result<PreparedChat, TextModelError> {
    let mut prepared = match model.prepare_chat(chat.clone()) {
        Ok(prepared) => prepared,
        Err(original @ TextModelError::Template(eredu_text::error::Error::RenderTemplate(_))) => {
            // Some checkpoint templates accept only alternating user/assistant text.
            // Preserve every instruction and turn without rewriting the template.
            let Some(messages) = alternating_text_messages(&chat.messages) else {
                return Err(original);
            };
            chat.messages = messages;
            model.prepare_chat(chat.clone()).map_err(|_| original)?
        }
        Err(err) => return Err(err),
    };
    let Some(effort) = effort else {
        return Ok(prepared);
    };
    if !prepared.capabilities().reasoning_parser.is_supported()
        || chat.reasoning_effort.is_some()
        || !chat.extra_template_kwargs.is_empty()
    {
        return Ok(prepared);
    }

    // Eredu exposes reasoning parsing separately from template-specific controls.
    // Apply generic preferences only when preparation accepts them; explicit controls
    // and checkpoint-specific template kwargs were validated above and take precedence.
    if chat.enable_thinking.is_none() {
        chat.enable_thinking = Some(effort != ThinkingEffort::Off);
        match model.prepare_chat(chat.clone()) {
            Ok(candidate) => prepared = candidate,
            Err(_) => chat.enable_thinking = None,
        }
    }
    if effort != ThinkingEffort::Off && chat.enable_thinking != Some(false) {
        chat.reasoning_effort = Some(match effort {
            ThinkingEffort::Max => "xhigh".into(),
            _ => effort.to_string(),
        });
        if let Ok(candidate) = model.prepare_chat(chat) {
            prepared = candidate;
        }
    }
    Ok(prepared)
}

pub fn prepare<B: eredu_core::TextGenerationBackend>(
    model: &mut LoadedModel<B>,
    request: &Request,
) -> Result<(PreparedChat, bool), ProviderError> {
    let emulated = !request.tools.is_empty()
        && request.settings.tool_calling == ToolCallingMode::ForceEmulated;
    let effort = request.model.thinking_effort();
    let prepared = match prepare_chat(model, chat_request(request, emulated)?, effort) {
        Ok(prepared) => prepared,
        Err(original @ TextModelError::Template(eredu_text::error::Error::RenderTemplate(_)))
            if !request.tools.is_empty()
                && request.settings.tool_calling == ToolCallingMode::Auto =>
        {
            // Tool history must use the emulator's text syntax for text-only templates.
            return match prepare_chat(model, chat_request(request, true)?, effort) {
                Ok(prepared) if !prepared.native_tool_support().is_supported() => {
                    Ok((prepared, true))
                }
                _ => Err(error(original)),
            };
        }
        Err(err) => return Err(error(err)),
    };
    if emulated || request.tools.is_empty() {
        return Ok((prepared, emulated));
    }
    match prepared.native_tool_support() {
        NativeToolSupport::Supported => Ok((prepared, false)),
        NativeToolSupport::Unsupported { reason } => {
            if request.settings.tool_calling == ToolCallingMode::ForceNative {
                return Err(error(format!(
                    "Native tool calling is unavailable for the effective template: {reason}"
                )));
            }
            prepare_chat(model, chat_request(request, true)?, effort)
                .map(|prepared| (prepared, true))
                .map_err(error)
        }
    }
}

pub fn generate<B>(
    planned: &mut PlannedModel<B, B::Drafter>,
    request: &Request,
    tx: &StreamSender,
    cancellation: GenerationCancellationToken,
) -> Result<ProviderUsage, ProviderError>
where
    B: ModelCapabilityBackend + SpeculativeGenerationBackend,
{
    let started = Instant::now();
    let (prepared, emulated) = prepare(planned.model_mut(), request)?;
    let input = planned
        .model()
        .count_prepared_chat(&prepared)
        .map_err(error)?;
    let mut settings = generation_settings(&request.settings, &request.model)?;
    let resolved = planned
        .model()
        .resolve_generation_config(settings.overrides)
        .map_err(error)?;
    if matches!(settings.strategy, TextSamplingStrategy::MirostatV2 { .. })
        && resolved.temperature <= 0.0
    {
        return Err(error("Mirostat V2 requires a positive effective temperature. Set a temperature override or reset sampling to checkpoint defaults."));
    }
    let capabilities = planned.model().capabilities().map_err(error)?;
    let actual_limit = capabilities.effective_max_context.value().copied();
    let explicit_limit = request
        .model
        .context_limit
        .map(|limit| limit as u64)
        .filter(|limit| *limit > 0)
        .or_else(|| {
            request
                .settings
                .context_size
                .map(u64::from)
                .filter(|limit| *limit > 0)
        });
    let context = match (actual_limit, explicit_limit) {
        (Some(actual), Some(explicit)) => Some(actual.min(explicit)),
        (actual, explicit) => actual.or(explicit),
    };
    // Eredu's prepared-chat API uses 256 only when max_new_tokens is unresolved.
    // Keep the override absent unless context admission requires a smaller budget.
    let mut output_budget = resolved.max_new_tokens.unwrap_or(256) as u64;
    if let Some(context) = context {
        if input.model_positions >= context {
            return Err(ProviderError::ContextLengthExceeded(format!(
                "Prompt ({} tokens) exceeds context limit ({context} tokens)",
                input.model_positions
            )));
        }
        let headroom = context - input.model_positions;
        if output_budget > headroom {
            output_budget = headroom;
            settings.overrides.max_new_tokens = Some(headroom as usize);
        }
    }
    let admission = planned
        .model()
        .admit(
            AdmissionRequest {
                input,
                max_output_tokens: output_budget,
                batch_size: 1,
                safety_reserve_bytes: 0,
                application_memory_budget_bytes: None,
                require_complete_estimate: false,
            },
            None,
        )
        .map_err(error)?;
    if let AdmissionResult::Rejected(reason) = admission {
        return Err(match reason {
            eredu_core::AdmissionRejection::PromptExceedsContext { .. }
            | eredu_core::AdmissionRejection::OutputHeadroomExceedsContext { .. } => {
                ProviderError::ContextLengthExceeded(format!(
                    "Eredu context admission rejected the request: {reason:?}"
                ))
            }
            _ => error(format!("Eredu admission rejected the request: {reason:?}")),
        });
    }
    let stops = match request.model.request_param::<Value>("stop") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(stop)) => vec![stop],
        Some(value) => serde_json::from_value::<Vec<String>>(value).map_err(error)?,
    };
    let mut emitter = Emitter::new(request, tx, cancellation.clone(), emulated);
    let speculative = planned.speculative_generation_options().map_err(error)?;
    let (model, drafting) = planned.parts_mut();
    let text_only = matches!(
        prepared.semantic_support(),
        SemanticSupport::Unsupported { .. }
    );
    let usage =
        if let (Some(options), Some(drafting)) = (speculative, drafting.as_speculative_draft()) {
            let generation = PreparedChatSpeculativeGenerationRequest {
                input: PreparedChatInput::rendered_prompt(&prepared),
                drafting,
                settings,
                options,
                caller_stop_sequences: &stops,
                cancellation: cancellation.clone(),
                on_event: |event| emitter.push(event),
            };
            let output = if text_only {
                model.generate_prepared_text_speculative(generation)
            } else {
                model.generate_prepared_chat_speculative(generation)
            }
            .map_err(error)?;
            let stats = output.stats();
            let mut usage = usage(
                request,
                input.text_tokens,
                &output,
                Some(draft_stats(stats, &request.settings)),
                started,
            );
            usage
                .additional_data
                .get_or_insert_with(Default::default)
                .insert(
                    "eredu_speculation".into(),
                    serde_json::to_value(eredu_core::speculative_decoding_telemetry(stats))
                        .map_err(error)?,
                );
            usage
        } else {
            let generation = PreparedChatGenerationRequest {
                input: PreparedChatInput::rendered_prompt(&prepared),
                settings,
                caller_stop_sequences: &stops,
                cancellation: cancellation.clone(),
                on_event: |event| emitter.push(event),
            };
            let output = if text_only {
                model.generate_prepared_text(generation)
            } else {
                model.generate_prepared_chat(generation)
            }
            .map_err(error)?;
            usage(request, input.text_tokens, &output, None, started)
        };
    if let Some(error) = emitter.failure {
        return Err(error);
    }
    Ok(usage)
}

pub fn usage<S>(
    request: &Request,
    input_tokens: u64,
    output: &GenerationOutput<S>,
    draft: Option<DraftStats>,
    started: Instant,
) -> ProviderUsage {
    ProviderUsage::new(
        request.model.model_name.clone(),
        Usage::new(
            Some(i32::try_from(input_tokens).unwrap_or(i32::MAX)),
            Some(i32::try_from(output.token_ids.len()).unwrap_or(i32::MAX)),
            None,
        ),
    )
    .with_stats(ProviderStats {
        time_to_first_token_ms: output
            .timing()
            .time_to_first_token()
            .map(|time| time.as_millis() as u64),
        model_load_ms: request.model_load_ms,
        elapsed_ms: Some(started.elapsed().as_millis() as u64),
        output_tokens: Some(output.token_ids.len()),
        draft,
    })
    .with_finish_reasons(vec![serde_json::to_value(output.finish_reason)
        .expect("finish reason serializes")
        .as_str()
        .unwrap()
        .to_owned()])
}

fn draft_stats(stats: &SpeculativeStats, settings: &ModelSettings) -> DraftStats {
    DraftStats {
        model: settings.draft_model.clone(),
        draft_tokens: stats.draft_tokens(),
        accepted_tokens: stats.accepted_tokens(),
        target_tokens: stats.target_tokens(),
        rounds: stats.rounds(),
        accept_rate: stats.accept_rate(),
    }
}

struct PendingCall {
    id: String,
    name: String,
    arguments: String,
}

pub struct Emitter<'a> {
    request: &'a Request,
    tx: &'a StreamSender,
    cancellation: GenerationCancellationToken,
    pending: HashMap<usize, PendingCall>,
    active: Option<usize>,
    emulator: Option<StreamingEmulatorParser>,
    finished: bool,
    pub failure: Option<ProviderError>,
}

impl<'a> Emitter<'a> {
    pub fn new(
        request: &'a Request,
        tx: &'a StreamSender,
        cancellation: GenerationCancellationToken,
        emulated: bool,
    ) -> Self {
        Self {
            request,
            tx,
            cancellation,
            pending: HashMap::new(),
            active: None,
            emulator: emulated.then(|| {
                StreamingEmulatorParser::new(
                    request
                        .tools
                        .iter()
                        .any(|tool| tool.name == CODE_EXECUTION_TOOL),
                )
            }),
            finished: false,
            failure: None,
        }
    }

    fn send(&mut self, mut message: Message) {
        message.id = Some(self.request.message_id.clone());
        if self.tx.blocking_send(Ok((Some(message), None))).is_err() {
            self.cancellation.cancel();
        }
    }

    pub fn push(&mut self, event: SemanticEvent) {
        if self.finished
            || self.failure.is_some()
            || self.tx.is_closed()
            || self.cancellation.is_cancelled()
        {
            self.cancellation.cancel();
            return;
        }
        match event {
            SemanticEvent::ReasoningDelta(text) => {
                self.send(Message::assistant().with_thinking(text, ""))
            }
            SemanticEvent::TextDelta(text) => {
                if let Some(parser) = &mut self.emulator {
                    let actions = parser.process_chunk(&text);
                    for action in actions {
                        let (message, tool) =
                            message_for_emulator_action(&action, &self.request.message_id);
                        self.send(message);
                        if tool {
                            self.cancellation.cancel();
                            break;
                        }
                    }
                } else {
                    self.send(Message::assistant().with_text(text));
                }
            }
            SemanticEvent::ToolCallStart { index, id, name } => {
                self.pending.insert(
                    index,
                    PendingCall {
                        id: format!("{}_{}", self.request.message_id, id),
                        name,
                        arguments: String::new(),
                    },
                );
                self.active = Some(index);
            }
            SemanticEvent::ToolArgumentsDelta {
                index,
                json_fragment,
            } => {
                if let Some(call) = self.pending.get_mut(&index) {
                    call.arguments.push_str(&json_fragment);
                }
            }
            SemanticEvent::ToolCallEnd => {
                if let Some(call) = self
                    .active
                    .take()
                    .and_then(|index| self.pending.remove(&index))
                {
                    match serde_json::from_str(&call.arguments) {
                        Ok(arguments) => self.send(Message::assistant().with_tool_request(
                            call.id,
                            Ok(CallToolRequestParams::new(call.name).with_arguments(arguments)),
                        )),
                        Err(err) => {
                            self.failure = Some(error(err));
                            self.cancellation.cancel();
                        }
                    }
                }
            }
            SemanticEvent::Finished { reason } => {
                self.pending.clear();
                if reason != eredu_core::FinishReason::Cancelled {
                    if let Some(parser) = &mut self.emulator {
                        let actions = parser.flush();
                        for action in actions {
                            let (message, _) =
                                message_for_emulator_action(&action, &self.request.message_id);
                            self.send(message);
                        }
                    }
                }
                self.finished = true;
            }
        }
    }
}
