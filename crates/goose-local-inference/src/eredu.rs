#[cfg(feature = "hf-hub")]
use std::collections::{HashMap, HashSet};

#[cfg(feature = "hf-hub")]
fn safetensors_shard(filename: &str) -> Option<(&str, u32, u32)> {
    let stem = filename.strip_suffix(".safetensors")?;
    let (indexed_name, total) = stem.rsplit_once("-of-")?;
    let (family, index) = indexed_name.rsplit_once('-')?;
    let index = index.parse().ok()?;
    let total = total.parse().ok()?;
    (index > 0 && index <= total).then_some((family, index, total))
}

#[cfg(feature = "hf-hub")]
pub(crate) fn snapshot_files_are_complete(
    filenames: &HashSet<&str>,
    index: Option<&serde_json::Value>,
) -> bool {
    let safetensors: Vec<_> = filenames
        .iter()
        .copied()
        .filter(|filename| filename.ends_with(".safetensors"))
        .collect();
    if safetensors.is_empty() {
        return false;
    }

    if let Some(index) = index {
        let Some(weight_map) = index.get("weight_map").and_then(|value| value.as_object()) else {
            return false;
        };
        let Some(expected): Option<HashSet<_>> =
            weight_map.values().map(|value| value.as_str()).collect()
        else {
            return false;
        };
        return !expected.is_empty()
            && expected.iter().all(|filename| {
                filename.ends_with(".safetensors") && filenames.contains(filename)
            });
    }

    let mut shard_groups = HashMap::new();
    for filename in safetensors {
        if let Some((family, index, total)) = safetensors_shard(filename) {
            let (expected_total, indices) = shard_groups
                .entry(family)
                .or_insert_with(|| (total, HashSet::new()));
            if *expected_total != total {
                return false;
            }
            indices.insert(index);
        }
    }

    shard_groups.values().all(|(total, indices)| {
        indices.len() == *total as usize && (1..=*total).all(|index| indices.contains(&index))
    })
}

pub(crate) const EREDU_BACKEND_ID: &str = "eredu";

pub(crate) fn unavailable_reason() -> Option<&'static str> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("The eredu backend in Goose requires Apple silicon macOS.")
    } else if !cfg!(feature = "eredu") {
        Some("Eredu support was not compiled in. Rebuild with the `eredu` feature.")
    } else {
        None
    }
}

#[cfg(all(feature = "eredu", target_os = "macos", target_arch = "aarch64"))]
mod imp {
    use std::any::Any;
    use std::path::Path;
    use std::sync::mpsc;
    use std::thread::JoinHandle;
    use std::time::Instant;

    use ::eredu::api::{
        default_local_device, inspect_artifact, local_device_plan, ArtifactSource,
        ChatCapabilityRejection, ChatContentFormat, ChatInputPreparationError, ChatInputRequest,
        ChatMessage, ChatPart, ChatPreparationOptions, ChatToolCall, ChatToolDefinition,
        CompanionSelection, GenerationForecast, GenerationForecastError, GenerationForecastOptions,
        GenerationRequest as EreduGenerationRequest, OutputMode, PlannedModel,
        PreparedChatGenerationSettings, PreparedTextModel, TextInspectionOptions, TextModelError,
        TextModelOptions,
    };
    use ::eredu::runtime::chat::{NativeToolSupport, SemanticSupport, ToolChoice};
    use base64::prelude::*;
    use eredu_backend_mlx::{backend::MlxBackend, native::MlxDrafter, MlxBackendFactory};
    use eredu_core::{
        DraftPlacementPlan, DraftingPlan, ExecutionPlan, GenerationCancellationToken,
        GenerationConfigOverrides, InspectionSeverity, RealizedDrafting, ResidencyPlan, RgbImage,
        SemanticEvent, TextSamplingStrategy,
    };
    use eredu_text::tokenizer::ModelChatTemplate;
    use goose_provider_types::conversation::message::{Message, MessageContent};
    use goose_provider_types::conversation::token_usage::{
        DraftStats, ProviderStats, ProviderUsage, Usage,
    };
    use goose_provider_types::errors::ProviderError;
    use goose_provider_types::formats::openai;
    use goose_provider_types::images::ImageFormat;
    use goose_provider_types::request_log::LoggerHandleExt;
    use rmcp::model::{CallToolRequestParams, Tool};
    use serde_json::{json, Value};

    use crate::backend::{BackendLoadedModel, LocalGenerationRequest, LocalInferenceBackend};
    use crate::model::{ChatTemplate, ModelSettings, SamplingConfig, ToolCallingMode};
    use crate::tool_emulation::{
        build_emulator_tool_description, load_tiny_model_prompt, message_for_emulator_action,
        StreamingEmulatorParser, CODE_EXECUTION_TOOL,
    };
    use crate::{extract_text_content, ResolvedModelPaths, StreamSender};

    use super::EREDU_BACKEND_ID;

    type Session = PlannedModel<MlxBackend<'static>, RealizedDrafting<MlxDrafter>>;

    pub(crate) struct EreduBackend;

    impl EreduBackend {
        pub(crate) fn new() -> Self {
            Self
        }
    }

    fn text_options(settings: &ModelSettings) -> Result<TextModelOptions, ProviderError> {
        let chat_template = match &settings.chat_template {
            ChatTemplate::Embedded => None,
            ChatTemplate::CustomInline { template } if !template.trim().is_empty() => {
                Some(ModelChatTemplate::Single(template.clone()))
            }
            ChatTemplate::CustomInline { .. } => return Err(error("Custom chat template is empty")),
            ChatTemplate::Builtin { .. } => return Err(error(
                "llama.cpp built-in template names are not supported by eredu. Select the embedded template or supply a custom Jinja template."
            )),
        };
        Ok(TextModelOptions { chat_template })
    }

    pub(crate) fn validate_model_directory(path: &Path) -> Result<(), ProviderError> {
        inspect_artifact(&ArtifactSource::new(path)).map_err(error)?;
        Ok(())
    }

    fn load_model(
        resolved: &ResolvedModelPaths,
        settings: &ModelSettings,
    ) -> Result<Session, ProviderError> {
        if settings.draft_model.is_some() && resolved.draft_model_path.is_none() {
            return Err(error("The selected draft model is not available locally. Download it before enabling speculative decoding."));
        }
        let device = local_device_plan(default_local_device()).map_err(error)?;
        let mut plan = ExecutionPlan::fully_resident(device.clone());
        if let Some(draft) = &resolved.draft_model_path {
            plan = plan.with_drafting(DraftingPlan::External {
                residency: ResidencyPlan::FullyResident,
                model: draft.to_string_lossy().into_owned(),
                placement: DraftPlacementPlan::Device { device },
                max_draft_tokens: 4,
                lookahead: false,
                adaptive_lookahead: false,
            });
        }
        let mut source = ArtifactSource::new(&resolved.model_path);
        if let Some(projector) = &resolved.mmproj_path {
            source = source.with_projector(CompanionSelection::Explicit(projector.clone()));
        }
        let prepared = PreparedTextModel::from_artifact(
            inspect_artifact(&source).map_err(error)?,
            text_options(settings)?,
        )
        .map_err(error)?;
        let factory = MlxBackendFactory::default();
        let mut selected = prepared
            .select_execution_plan(&factory, &plan)
            .map_err(error)?;
        let report = selected.inspect(TextInspectionOptions::default());
        if !report.is_loadable() {
            return Err(error(
                report
                    .issues
                    .iter()
                    .filter(|issue| issue.severity == InspectionSeverity::Error)
                    .map(|issue| issue.detail.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        selected.load().map_err(error)
    }

    struct GenerationRequest {
        system: String,
        messages: Vec<Message>,
        tools: Vec<Tool>,
        settings: ModelSettings,
        temperature: Option<f32>,
        max_tokens: Option<i32>,
        context_limit: usize,
        message_id: String,
        tx: StreamSender,
        cancellation: GenerationCancellationToken,
        reply: mpsc::SyncSender<Result<GenerationResult, ProviderError>>,
    }

    struct GenerationResult {
        input_tokens: usize,
        output_tokens: usize,
        stats: ProviderStats,
        memory_forecast: Value,
    }

    // MLX sessions own thread-local state. Only commands cross Goose's blocking-pool threads.
    struct EreduLoadedModel {
        sender: Option<mpsc::Sender<GenerationRequest>>,
        worker: Option<JoinHandle<()>>,
    }

    impl BackendLoadedModel for EreduLoadedModel {
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    impl Drop for EreduLoadedModel {
        fn drop(&mut self) {
            self.sender.take();
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    impl LocalInferenceBackend for EreduBackend {
        fn id(&self) -> &'static str {
            EREDU_BACKEND_ID
        }

        fn load_model(
            &self,
            _model_id: &str,
            resolved: &ResolvedModelPaths,
            settings: &ModelSettings,
        ) -> Result<Box<dyn BackendLoadedModel>, ProviderError> {
            let resolved = resolved.clone();
            let settings = settings.clone();
            let (sender, requests) = mpsc::channel::<GenerationRequest>();
            let (ready, loaded) = mpsc::sync_channel(1);
            let worker = std::thread::Builder::new()
                .name("goose-eredu".into())
                .spawn(move || {
                    let mut session = match load_model(&resolved, &settings) {
                        Ok(loaded) => {
                            let _ = ready.send(Ok(()));
                            loaded
                        }
                        Err(err) => {
                            let _ = ready.send(Err(err));
                            return;
                        }
                    };
                    for request in requests {
                        let result = generate(&mut session, &request);
                        let settled = session.model().synchronize().map_err(error);
                        let result = result.and_then(|output| settled.map(|()| output));
                        let _ = request.reply.send(result);
                    }
                })
                .map_err(error)?;
            let loaded_model = EreduLoadedModel {
                sender: Some(sender),
                worker: Some(worker),
            };
            loaded.recv().map_err(error)??;
            Ok(Box::new(loaded_model))
        }

        fn generate(
            &self,
            loaded: &mut dyn BackendLoadedModel,
            request: LocalGenerationRequest<'_>,
        ) -> Result<(), ProviderError> {
            let loaded = loaded
                .as_any_mut()
                .downcast_mut::<EreduLoadedModel>()
                .ok_or_else(|| error("Loaded model backend mismatch"))?;
            let cancellation = GenerationCancellationToken::new();
            let monitor_token = cancellation.clone();
            let tx = request.tx.clone();
            let monitor = tokio::spawn(async move {
                tx.closed().await;
                monitor_token.cancel();
            });
            let (reply, result) = mpsc::sync_channel(1);
            let sent = loaded
                .sender
                .as_ref()
                .expect("live eredu worker")
                .send(GenerationRequest {
                    system: request.system.to_owned(),
                    messages: request.messages.to_vec(),
                    tools: request.tools.to_vec(),
                    settings: request.settings.clone(),
                    temperature: request.temperature,
                    max_tokens: request.max_tokens,
                    context_limit: request.context_limit,
                    message_id: request.message_id.to_owned(),
                    tx: request.tx.clone(),
                    cancellation,
                    reply,
                });
            let result = sent
                .map_err(error)
                .and_then(|()| result.recv().map_err(error))
                .and_then(|result| result);
            monitor.abort();
            let mut result = result?;
            result.stats.model_load_ms = request.model_load_ms;
            let usage = Usage::new(
                Some(result.input_tokens as i32),
                Some(result.output_tokens as i32),
                Some((result.input_tokens + result.output_tokens) as i32),
            );
            let _ = request.log.write(
                &json!({
                    "path": "eredu",
                    "stats": result.stats,
                    "memory_forecast": result.memory_forecast,
                }),
                Some(&usage),
            );
            let usage = ProviderUsage::new(request.model_name, usage).with_stats(result.stats);
            let _ = request.tx.blocking_send(Ok((None, Some(usage))));
            Ok(())
        }

        fn available_memory_bytes(&self) -> u64 {
            0
        }
    }

    fn generation_settings(
        request: &GenerationRequest,
        prompt_tokens: usize,
    ) -> Result<PreparedChatGenerationSettings, ProviderError> {
        if request.context_limit > 0 && prompt_tokens >= request.context_limit {
            return Err(ProviderError::ContextLengthExceeded(format!(
                "Prompt ({prompt_tokens} tokens) exceeds context limit ({} tokens)",
                request.context_limit
            )));
        }
        let settings = &request.settings;
        let (temperature, top_k, top_p, min_p, seed, strategy) = match settings.sampling {
            SamplingConfig::Greedy => (0.0, 0, 1.0, 0.0, None, TextSamplingStrategy::Standard),
            SamplingConfig::Temperature {
                temperature,
                top_k,
                top_p,
                min_p,
                seed,
            } => (
                temperature,
                top_k,
                top_p,
                min_p,
                seed,
                TextSamplingStrategy::Standard,
            ),
            SamplingConfig::MirostatV2 { tau, eta, seed } => (
                1.0,
                0,
                1.0,
                0.0,
                seed,
                TextSamplingStrategy::MirostatV2 { tau, eta },
            ),
        };
        let temperature = request.temperature.unwrap_or(temperature);
        let headroom = if request.context_limit == 0 {
            usize::MAX
        } else {
            request.context_limit - prompt_tokens
        };
        let max_tokens = settings
            .max_output_tokens
            .or_else(|| request.max_tokens.and_then(|n| usize::try_from(n).ok()))
            .unwrap_or(if headroom == usize::MAX {
                4096
            } else {
                headroom
            })
            .min(headroom);
        Ok(PreparedChatGenerationSettings {
            overrides: GenerationConfigOverrides {
                do_sample: Some(temperature > 0.0),
                temperature: Some(temperature),
                top_k: Some(top_k),
                top_p: Some(top_p),
                min_p: Some(min_p),
                repetition_penalty: Some(settings.repeat_penalty),
                repeat_last_n: Some(settings.repeat_last_n),
                frequency_penalty: Some(settings.frequency_penalty),
                presence_penalty: Some(settings.presence_penalty),
                max_new_tokens: Some(max_tokens),
            },
            seed: u64::from(seed.unwrap_or(0)),
            strategy,
            ..Default::default()
        })
    }

    fn prepare(
        request: &GenerationRequest,
        emulated: bool,
    ) -> Result<ChatInputRequest, ProviderError> {
        let messages = if emulated {
            let system = format!(
                "{}\n{}\n{}",
                request.system,
                load_tiny_model_prompt(),
                build_emulator_tool_description(
                    &request.tools,
                    request.tools.iter().any(|t| t.name == CODE_EXECUTION_TOOL)
                )
            );
            let mut values = vec![json!({"role": "system", "content": system})];
            for message in request.messages.iter().filter(|m| m.is_agent_visible()) {
                let mut text_message = message.clone();
                text_message
                    .content
                    .retain(|content| !matches!(content, MessageContent::Image(_)));
                let text = extract_text_content(&text_message);
                let mut content = vec![json!({"type": "text", "text": text})];
                for item in &message.content {
                    if let MessageContent::Image(image) = item {
                        content.push(json!({"type": "image_url", "image_url": {"url": format!("data:{};base64,{}", image.mime_type, image.data)}}));
                    }
                }
                values.push(json!({"role": if message.role == rmcp::model::Role::User { "user" } else { "assistant" }, "content": content}));
            }
            values
        } else {
            let mut values = vec![json!({"role": "system", "content": request.system})];
            values.extend(openai::format_messages(
                &request.messages,
                &ImageFormat::OpenAi,
            ));
            values
        };
        let messages = messages
            .into_iter()
            .map(chat_message)
            .collect::<Result<_, _>>()?;
        let tools = if emulated {
            Vec::new()
        } else {
            request
                .tools
                .iter()
                .map(|tool| ChatToolDefinition {
                    name: tool.name.to_string(),
                    description: tool.description.as_ref().map(ToString::to_string),
                    parameters: json!(tool.input_schema),
                })
                .collect()
        };
        Ok(ChatInputRequest {
            messages,
            tools,
            options: ChatPreparationOptions {
                tool_choice: if emulated || request.tools.is_empty() {
                    ToolChoice::None
                } else {
                    ToolChoice::Auto
                },
                enable_thinking: Some(request.settings.enable_thinking),
                // Goose enables thinking by default, including for plain-text models.
                allow_unparsed_reasoning: true,
                add_generation_prompt: true,
                content_format: ChatContentFormat::String {
                    separator: "\n".into(),
                },
                ..Default::default()
            },
        })
    }

    fn chat_message(message: Value) -> Result<ChatMessage, ProviderError> {
        let content = match &message["content"] {
            Value::String(text) => vec![ChatPart::Text(text.clone())],
            Value::Array(parts) => parts
                .iter()
                .map(|part| {
                    if part["type"] == "text" {
                        return Ok(ChatPart::Text(
                            part["text"].as_str().unwrap_or_default().to_owned(),
                        ));
                    }
                    let url = part["image_url"]["url"]
                        .as_str()
                        .ok_or_else(|| error("Missing image data"))?;
                    let (_, data) = url
                        .split_once(',')
                        .filter(|(prefix, _)| {
                            prefix.starts_with("data:") && prefix.ends_with(";base64")
                        })
                        .ok_or_else(|| {
                            error("Attach images directly; eredu does not fetch remote image URLs")
                        })?;
                    let bytes = BASE64_STANDARD.decode(data).map_err(error)?;
                    let image = image::load_from_memory(&bytes).map_err(error)?.into_rgb8();
                    let (width, height) = image.dimensions();
                    Ok(ChatPart::Image(
                        RgbImage::new(image.into_raw(), width, height).map_err(error)?,
                    ))
                })
                .collect::<Result<_, ProviderError>>()?,
            _ => Vec::new(),
        };
        Ok(match message["role"].as_str() {
            Some("system") => ChatMessage::System(content),
            Some("user") => ChatMessage::User(content),
            Some("assistant") => ChatMessage::Assistant {
                content,
                reasoning_content: message["reasoning_content"].as_str().map(str::to_owned),
                tool_calls: message["tool_calls"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|call| {
                        ChatToolCall::from_json_arguments(
                            call["id"].as_str().map(str::to_owned),
                            call["function"]["name"].as_str().unwrap_or_default(),
                            call["function"]["arguments"].as_str().unwrap_or_default(),
                        )
                        .map_err(error)
                    })
                    .collect::<Result<_, _>>()?,
            },
            Some("tool") => ChatMessage::Tool {
                tool_call_id: message["tool_call_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                name: message["name"].as_str().map(str::to_owned),
                content,
            },
            role => return Err(error(format!("Unsupported chat role: {role:?}"))),
        })
    }

    fn generate(
        session: &mut Session,
        request: &GenerationRequest,
    ) -> Result<GenerationResult, ProviderError> {
        if request.tx.is_closed() {
            request.cancellation.cancel();
        }
        if request.cancellation.is_cancelled() {
            return Ok(GenerationResult {
                input_tokens: 0,
                output_tokens: 0,
                stats: ProviderStats::default(),
                memory_forecast: Value::Null,
            });
        }
        session.model_mut().reset().map_err(error)?;
        let mut emulated = !request.tools.is_empty()
            && request.settings.tool_calling == ToolCallingMode::ForceEmulated;
        let prepared = session
            .model_mut()
            .prepare_typed_chat(prepare(request, emulated)?);
        let prepared = match prepared {
            Ok(prepared)
                if emulated
                    || request.tools.is_empty()
                    || matches!(
                        prepared.chat().native_tool_support(),
                        NativeToolSupport::Supported
                    ) =>
            {
                prepared
            }
            Ok(_) if request.settings.tool_calling == ToolCallingMode::Auto => {
                emulated = true;
                session
                    .model_mut()
                    .prepare_typed_chat(prepare(request, true)?)
                    .map_err(error)?
            }
            Err(ChatInputPreparationError::Text(TextModelError::UnsupportedCapability(
                ChatCapabilityRejection::NativeTools(_),
            ))) if request.settings.tool_calling == ToolCallingMode::Auto => {
                emulated = true;
                session
                    .model_mut()
                    .prepare_typed_chat(prepare(request, true)?)
                    .map_err(error)?
            }
            Ok(prepared) => {
                return Err(error(format!(
                    "Native tools unavailable: {:?}",
                    prepared.chat().native_tool_support()
                )))
            }
            Err(err) => return Err(error(err)),
        };
        let started = Instant::now();
        let prepared = session
            .model()
            .materialize_chat_input(prepared)
            .map_err(error)?;
        let input_tokens = usize::try_from(prepared.count().model_positions).map_err(error)?;
        let settings = generation_settings(request, input_tokens)?;
        let (chat, payload) = prepared.into_parts();
        let output_mode = if matches!(chat.semantic_support(), SemanticSupport::Supported) {
            OutputMode::Semantic
        } else {
            OutputMode::Text
        };
        let mut emitter = Emitter::new(request, emulated);
        let generation = EreduGenerationRequest {
            input: payload.into_input(session.model(), &chat).map_err(error)?,
            output_mode,
            settings,
            caller_stop_sequences: &[],
            cancellation: request.cancellation.clone(),
            control_handle: None,
            on_event: |event| emitter.event(event),
        };
        let memory_forecast = forecast_diagnostic(
            session.forecast_generation(&generation, &GenerationForecastOptions::default()),
        );
        let output = session.generate(generation).map_err(error)?;
        let output_tokens = output.token_ids.len();
        let timing = *output.timing();
        let draft = output.stats().drafting.as_ref().map(|stats| DraftStats {
            model: request.settings.draft_model.clone(),
            draft_tokens: stats.draft_tokens(),
            accepted_tokens: stats.accepted_tokens(),
            target_tokens: stats.target_tokens(),
            rounds: stats.rounds(),
            accept_rate: stats.accept_rate(),
        });
        emitter.finish();
        Ok(GenerationResult {
            input_tokens,
            output_tokens,
            memory_forecast,
            stats: ProviderStats {
                time_to_first_token_ms: timing.time_to_first_token().map(|d| d.as_millis() as u64),
                elapsed_ms: Some(started.elapsed().as_millis() as u64),
                output_tokens: Some(output_tokens),
                draft,
                ..Default::default()
            },
        })
    }

    fn forecast_diagnostic(forecast: Result<GenerationForecast, GenerationForecastError>) -> Value {
        match forecast {
            Ok(forecast) => json!({ "forecast": forecast }),
            Err(error) => json!({ "error": error.to_string() }),
        }
    }

    struct Emitter<'a> {
        request: &'a GenerationRequest,
        emulator: Option<StreamingEmulatorParser>,
        emulated_tool_emitted: bool,
    }

    impl<'a> Emitter<'a> {
        fn new(request: &'a GenerationRequest, emulated: bool) -> Self {
            Self {
                request,
                emulator: emulated.then(|| {
                    StreamingEmulatorParser::new(
                        request.tools.iter().any(|t| t.name == CODE_EXECUTION_TOOL),
                    )
                }),
                emulated_tool_emitted: false,
            }
        }
        fn send(&mut self, mut message: Message) {
            message.id = Some(self.request.message_id.clone());
            if self
                .request
                .tx
                .blocking_send(Ok((Some(message), None)))
                .is_err()
            {
                self.request.cancellation.cancel();
            }
        }
        fn content(&mut self, text: &str) {
            if let Some(parser) = &mut self.emulator {
                let actions = parser.process_chunk(text);
                for action in actions {
                    let (message, is_tool) =
                        message_for_emulator_action(&action, &self.request.message_id);
                    self.send(message);
                    if is_tool {
                        self.emulated_tool_emitted = true;
                        self.request.cancellation.cancel();
                        break;
                    }
                }
            } else if !text.is_empty() {
                self.send(Message::assistant().with_text(text));
            }
        }
        fn event(&mut self, event: SemanticEvent) {
            match event {
                SemanticEvent::TextDelta(text) => self.content(&text),
                SemanticEvent::ReasoningDelta(text) if self.request.settings.enable_thinking => {
                    self.send(Message::assistant().with_thinking(text, ""))
                }
                SemanticEvent::ToolCallCompleted {
                    id,
                    name,
                    arguments,
                    ..
                } => {
                    self.send(Message::assistant().with_tool_request(
                        id,
                        Ok(CallToolRequestParams::new(name).with_arguments(arguments)),
                    ));
                }
                _ => {}
            }
        }
        fn finish(&mut self) {
            if self.request.tx.is_closed() || self.emulated_tool_emitted {
                return;
            }
            if let Some(parser) = &mut self.emulator {
                let actions = parser.flush();
                for action in actions {
                    let (message, is_tool) =
                        message_for_emulator_action(&action, &self.request.message_id);
                    self.send(message);
                    if is_tool {
                        break;
                    }
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn request() -> (
            GenerationRequest,
            tokio::sync::mpsc::Receiver<
                Result<(Option<Message>, Option<ProviderUsage>), ProviderError>,
            >,
        ) {
            let (tx, rx) = tokio::sync::mpsc::channel(16);
            let (reply, _) = mpsc::sync_channel(1);
            (
                GenerationRequest {
                    system: String::new(),
                    messages: Vec::new(),
                    tools: Vec::new(),
                    settings: ModelSettings::default(),
                    temperature: None,
                    max_tokens: None,
                    context_limit: 1024,
                    message_id: "turn".into(),
                    tx,
                    cancellation: GenerationCancellationToken::new(),
                    reply,
                },
                rx,
            )
        }

        #[test]
        fn native_calls_are_published_only_when_completed() {
            let (request, mut rx) = request();
            let mut emitter = Emitter::new(&request, false);
            emitter.event(SemanticEvent::ToolCallStart {
                index: 0,
                id: "call".into(),
                name: "extension.original".into(),
            });
            emitter.event(SemanticEvent::ToolArgumentsDelta {
                index: 0,
                json_fragment: r#"{"command":"pwd"}"#.into(),
            });
            assert!(rx.try_recv().is_err());
            emitter.event(SemanticEvent::ToolCallCompleted {
                index: 0,
                id: "call".into(),
                name: "extension.original".into(),
                arguments: serde_json::from_str(r#"{"command":"pwd"}"#).unwrap(),
            });
            let (message, _) = rx.try_recv().unwrap().unwrap();
            let message = message.unwrap();
            let MessageContent::ToolRequest(call) = &message.content[0] else {
                panic!("expected a tool request")
            };
            assert_eq!(call.tool_call.as_ref().unwrap().name, "extension.original");
            assert_eq!(
                call.tool_call.as_ref().unwrap().arguments.as_ref().unwrap()["command"],
                "pwd"
            );
            emitter.finish();
            assert!(rx.try_recv().is_err());
        }

        #[test]
        fn cancelled_partial_native_call_is_not_executable() {
            let (request, mut rx) = request();
            let mut emitter = Emitter::new(&request, false);
            emitter.event(SemanticEvent::ToolCallStart {
                index: 0,
                id: "call".into(),
                name: "shell".into(),
            });
            emitter.event(SemanticEvent::ToolArgumentsDelta {
                index: 0,
                json_fragment: "{\"command\":".into(),
            });
            request.cancellation.cancel();
            emitter.finish();
            assert!(rx.try_recv().is_err());
        }

        #[test]
        fn sampling_preserves_mirostat_penalties_and_context_headroom() {
            let (mut request, _rx) = request();
            request.settings.sampling = SamplingConfig::MirostatV2 {
                tau: 3.0,
                eta: 0.2,
                seed: Some(7),
            };
            request.settings.repeat_penalty = 1.2;
            request.settings.frequency_penalty = 0.4;
            request.settings.max_output_tokens = Some(512);
            let sampling = generation_settings(&request, 1000).unwrap();
            assert_eq!(
                sampling.strategy,
                TextSamplingStrategy::MirostatV2 { tau: 3.0, eta: 0.2 }
            );
            assert_eq!(sampling.seed, 7);
            assert_eq!(sampling.overrides.repetition_penalty, Some(1.2));
            assert_eq!(sampling.overrides.frequency_penalty, Some(0.4));
            assert_eq!(sampling.overrides.max_new_tokens, Some(24));
            assert!(matches!(
                generation_settings(&request, 1024),
                Err(ProviderError::ContextLengthExceeded(_))
            ));
        }
    }

    fn error(error: impl std::fmt::Display) -> ProviderError {
        ProviderError::ExecutionError(format!("Eredu: {error}"))
    }
}

#[cfg(not(all(feature = "eredu", target_os = "macos", target_arch = "aarch64")))]
mod imp {
    use super::*;
    use crate::backend::{BackendLoadedModel, LocalGenerationRequest, LocalInferenceBackend};
    use crate::model::ModelSettings;
    use crate::ResolvedModelPaths;
    use goose_provider_types::errors::ProviderError;
    use std::path::Path;

    pub(crate) struct EreduBackend;
    impl EreduBackend {
        pub(crate) fn new() -> Self {
            Self
        }
    }
    fn unavailable() -> ProviderError {
        ProviderError::ExecutionError(
            unavailable_reason()
                .expect("unavailable eredu build")
                .into(),
        )
    }
    pub(crate) fn validate_model_directory(_path: &Path) -> Result<(), ProviderError> {
        Err(unavailable())
    }
    impl LocalInferenceBackend for EreduBackend {
        fn id(&self) -> &'static str {
            EREDU_BACKEND_ID
        }
        fn load_model(
            &self,
            _: &str,
            _: &ResolvedModelPaths,
            _: &ModelSettings,
        ) -> Result<Box<dyn BackendLoadedModel>, ProviderError> {
            Err(unavailable())
        }
        fn generate(
            &self,
            _: &mut dyn BackendLoadedModel,
            _: LocalGenerationRequest<'_>,
        ) -> Result<(), ProviderError> {
            Err(unavailable())
        }
        fn available_memory_bytes(&self) -> u64 {
            0
        }
    }
}

pub(crate) use imp::{validate_model_directory, EreduBackend};
