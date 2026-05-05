pub mod hf_models;
mod inference_emulated_tools;
mod inference_engine;
mod inference_native_tools;
pub mod local_model_registry;
pub(crate) mod multimodal;
mod tool_parsing;

use inference_emulated_tools::{
    build_emulator_tool_description, generate_with_emulated_tools, load_tiny_model_prompt,
};
use inference_engine::GenerationContext;
use inference_engine::LoadedModel;
use inference_native_tools::generate_with_native_tools;
use tool_parsing::compact_tools_json;

use crate::config::ExtensionConfig;
use crate::conversation::message::{Message, MessageContent};
use crate::model::ModelConfig;
use crate::providers::base::{
    MessageStream, Provider, ProviderDef, ProviderMetadata, ProviderUsage, Usage,
};
use crate::providers::errors::ProviderError;
use crate::providers::formats::openai::format_tools;
use crate::providers::utils::RequestLog;
use anyhow::Result;
use async_stream::try_stream;
use async_trait::async_trait;
use futures::future::BoxFuture;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::{list_llama_ggml_backend_devices, LlamaBackendDeviceType, LogOptions};
use multimodal::ExtractedImage;
use rmcp::model::{Role, Tool};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, Weak};
use tokio::sync::Mutex;
use uuid::Uuid;

const SHELL_TOOL: &str = "developer__shell";
const CODE_EXECUTION_TOOL: &str = "code_execution__execute_typescript";

type ModelSlot = Arc<Mutex<Option<LoadedModel>>>;

/// Owns the llama backend and all cached models. Field order matters:
/// `models` is declared before `backend` so Rust drops all loaded models
/// (and their Metal/GPU resources) before the backend calls
/// `llama_backend_free()`, avoiding the ggml-metal assertion on shutdown.
pub struct InferenceRuntime {
    models: StdMutex<HashMap<String, ModelSlot>>,
    backend: LlamaBackend,
}

/// Global weak reference used to share a single `InferenceRuntime` across
/// all providers and server routes. Only a `Weak` is stored — strong `Arc`s
/// live in providers and `AppState`. When all strong refs drop (normal
/// shutdown), the runtime is deallocated and the backend freed. The `Weak`
/// left behind is inert during `__cxa_finalize`, so no ggml statics race.
static RUNTIME: StdMutex<Weak<InferenceRuntime>> = StdMutex::new(Weak::new());

impl InferenceRuntime {
    pub fn get_or_init() -> Result<Arc<Self>> {
        let mut guard = RUNTIME.lock().expect("runtime lock poisoned");
        if let Some(runtime) = guard.upgrade() {
            return Ok(runtime);
        }
        // Safety invariant: the Weak::upgrade() check and LlamaBackend::init()
        // both execute inside this same mutex guard, so there is no window where
        // another thread could drop the Arc and re-enter concurrently.
        // BackendAlreadyInitialized therefore means LlamaBackend::drop() did not
        // reset the C library's init flag — a llama-cpp-rs bug, not a race.
        let backend = match LlamaBackend::init() {
            Ok(b) => b,
            Err(llama_cpp_2::LlamaCppError::BackendAlreadyInitialized) => {
                unreachable!(
                    "LlamaBackend already initialized but Weak was dead; \
                     the mutex guard prevents concurrent re-init"
                )
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to initialize local inference runtime");
                return Err(anyhow::anyhow!("Failed to init llama backend: {}", e));
            }
        };
        llama_cpp_2::send_logs_to_tracing(LogOptions::default());
        log_inference_backend_devices();
        let runtime = Arc::new(Self {
            models: StdMutex::new(HashMap::new()),
            backend,
        });
        *guard = Arc::downgrade(&runtime);
        Ok(runtime)
    }

    pub fn backend(&self) -> &LlamaBackend {
        &self.backend
    }

    fn get_or_create_model_slot(&self, model_id: &str) -> ModelSlot {
        let mut map = self.models.lock().expect("model cache lock poisoned");
        map.entry(model_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    }

    fn other_model_slots(&self, keep_model_id: &str) -> Vec<ModelSlot> {
        let map = self.models.lock().expect("model cache lock poisoned");
        map.iter()
            .filter(|(id, _)| id.as_str() != keep_model_id)
            .map(|(_, slot)| slot.clone())
            .collect()
    }
}

const PROVIDER_NAME: &str = "local";
const DEFAULT_MODEL: &str = "bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M";

pub const LOCAL_LLM_MODEL_CONFIG_KEY: &str = "LOCAL_LLM_MODEL";

pub struct ResolvedModelPaths {
    pub model_path: PathBuf,
    pub context_limit: usize,
    pub settings: crate::providers::local_inference::local_model_registry::ModelSettings,
    pub mmproj_path: Option<PathBuf>,
}

/// Resolve model path, context limit, settings, and mmproj path for a model ID from the registry.
pub fn resolve_model_path(model_id: &str) -> Option<ResolvedModelPaths> {
    use crate::providers::local_inference::local_model_registry::{
        default_settings_for_model, get_registry,
    };

    if let Ok(registry) = get_registry().lock() {
        if let Some(entry) = registry.get_model(model_id) {
            let ctx = entry.settings.context_size.unwrap_or(0) as usize;
            let mut settings = entry.settings.clone();
            // Capability flags are inherent to the model family, not user-configurable.
            // Re-derive them so that registry entries persisted before a model was
            // recognized (or with a different quantization) still get the right behavior.
            let defaults = default_settings_for_model(model_id);
            settings.native_tool_calling = defaults.native_tool_calling;
            settings.vision_capable = defaults.vision_capable;
            settings.mmproj_size_bytes = entry.mmproj_size_bytes;
            let mmproj_path = entry.mmproj_path.as_ref().filter(|p| p.exists()).cloned();
            return Some(ResolvedModelPaths {
                model_path: entry.local_path.clone(),
                context_limit: ctx,
                settings,
                mmproj_path,
            });
        }
    }

    None
}

fn is_accelerator_device(device_type: LlamaBackendDeviceType) -> bool {
    matches!(
        device_type,
        LlamaBackendDeviceType::Gpu
            | LlamaBackendDeviceType::IntegratedGpu
            | LlamaBackendDeviceType::Accelerator
    )
}

fn is_non_cpu_device(device_type: LlamaBackendDeviceType) -> bool {
    !matches!(device_type, LlamaBackendDeviceType::Cpu)
}

fn log_inference_backend_devices() {
    let devices = list_llama_ggml_backend_devices();
    let non_cpu_devices: Vec<_> = devices
        .iter()
        .filter(|device| is_non_cpu_device(device.device_type))
        .collect();

    if non_cpu_devices.is_empty() {
        tracing::info!(
            device_count = devices.len(),
            "No non-CPU llama.cpp backend devices detected for local inference"
        );
        return;
    }

    for device in non_cpu_devices {
        tracing::info!(
            index = device.index,
            backend = %device.backend,
            name = %device.name,
            description = %device.description,
            device_type = ?device.device_type,
            memory_total_bytes = device.memory_total as u64,
            memory_free_bytes = device.memory_free as u64,
            "Non-CPU llama.cpp backend device detected for local inference"
        );
    }
}

pub fn available_inference_memory_bytes(runtime: &InferenceRuntime) -> u64 {
    let _ = &runtime.backend;
    let devices = list_llama_ggml_backend_devices();

    let accel_memory = devices
        .iter()
        .filter(|d| is_accelerator_device(d.device_type))
        .map(|d| d.memory_free as u64)
        .max()
        .unwrap_or(0);

    if accel_memory > 0 {
        accel_memory
    } else {
        devices
            .iter()
            .filter(|d| d.device_type == LlamaBackendDeviceType::Cpu)
            .map(|d| d.memory_free as u64)
            .max()
            .unwrap_or(0)
    }
}

pub fn recommend_local_model(runtime: &InferenceRuntime) -> String {
    use local_model_registry::{get_registry, is_featured_model, FEATURED_MODELS};

    let available_memory = available_inference_memory_bytes(runtime);

    if let Ok(registry) = get_registry().lock() {
        let mut models: Vec<_> = registry
            .list_models()
            .iter()
            .filter(|m| is_featured_model(&m.id) && m.size_bytes > 0)
            .collect();
        models.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

        // Return largest that fits in available memory
        for model in &models {
            if available_memory >= model.size_bytes {
                return model.id.clone();
            }
        }

        // If nothing fits, return smallest
        if let Some(smallest) = models.last() {
            return smallest.id.clone();
        }
    }

    // Fallback to first featured model
    FEATURED_MODELS[0].spec.to_string()
}

fn build_openai_messages_json(system: &str, messages: &[Message]) -> String {
    use crate::providers::formats::openai::format_messages;
    use crate::providers::utils::ImageFormat;

    let mut arr: Vec<Value> = vec![json!({"role": "system", "content": system})];
    arr.extend(format_messages(messages, &ImageFormat::OpenAi));
    strip_image_parts_from_messages(&mut arr);
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
}

/// Remove `image_url` content parts from OpenAI-format messages JSON, replacing
/// each with a text note. This prevents an FFI crash in llama.cpp which does not
/// accept `image_url` content-part types.
fn strip_image_parts_from_messages(messages: &mut [Value]) {
    let mut stripped = false;
    for msg in messages.iter_mut() {
        if let Some(content) = msg.get_mut("content").and_then(|c| c.as_array_mut()) {
            for part in content.iter_mut() {
                if part.get("type").and_then(|t| t.as_str()) == Some("image_url") {
                    *part = json!({
                        "type": "text",
                        "text": "[Image attached — image input is not supported with the currently selected model]"
                    });
                    stripped = true;
                }
            }
        }
    }
    if stripped {
        tracing::warn!("Stripped image content parts from messages — vision encoder not available for this model");
    }
}

/// Convert a message into plain text for the emulator path's chat history.
///
/// This is the emulator-path counterpart of [`format_messages`] used by the native
/// path. It reconstructs the text-based tool syntax that the emulator prompt teaches
/// the model:
///
/// - `ToolRequest` with a `"command"` argument → `$ command`
/// - `ToolRequest` with a `"code"` argument → `` ```execute_typescript\n…\n``` ``
/// - `ToolResponse` → `Command output:\n…`
///
/// Only `developer__shell` and `code_execution__execute_typescript` style tool calls are
/// recognized (by argument shape, not tool name). Tool calls from other extensions
/// (e.g. custom MCP tools made by a native-tool-calling model earlier in the
/// conversation) are silently dropped, since the emulator path has no syntax to
/// represent them.
fn extract_text_content(msg: &Message) -> String {
    let mut parts = Vec::new();

    for content in &msg.content {
        match content {
            MessageContent::Text(text) => {
                parts.push(text.text.clone());
            }
            MessageContent::ToolRequest(req) => {
                if let Ok(call) = &req.tool_call {
                    if let Some(cmd) = call
                        .arguments
                        .as_ref()
                        .and_then(|a| a.get("command"))
                        .and_then(|v| v.as_str())
                    {
                        parts.push(format!("$ {}", cmd));
                    } else if let Some(code) = call
                        .arguments
                        .as_ref()
                        .and_then(|a| a.get("code"))
                        .and_then(|v| v.as_str())
                    {
                        parts.push(format!("```execute_typescript\n{}\n```", code));
                    }
                }
            }
            MessageContent::ToolResponse(response) => match &response.tool_result {
                Ok(result) => {
                    let mut output_parts = Vec::new();
                    for content_item in &result.content {
                        if let Some(text_content) = content_item.as_text() {
                            output_parts.push(text_content.text.to_string());
                        }
                    }
                    if !output_parts.is_empty() {
                        parts.push(format!("Command output:\n{}", output_parts.join("\n")));
                    }
                }
                Err(e) => {
                    parts.push(format!("Command error: {}", e));
                }
            },
            MessageContent::Image(_) => {
                parts.push(
                    "[Image attached — image input is not supported with the currently selected model]"
                        .to_string(),
                );
            }
            _ => {}
        }
    }

    parts.join("\n")
}

/// Build a `ProviderUsage` and write the request log entry.
fn finalize_usage(
    log: &mut RequestLog,
    model_name: String,
    path_label: &str,
    prompt_token_count: usize,
    output_token_count: i32,
    extra_log_fields: Option<(&str, &str)>,
) -> ProviderUsage {
    let input_tokens = prompt_token_count as i32;
    let total_tokens = input_tokens + output_token_count;
    let usage = Usage::new(
        Some(input_tokens),
        Some(output_token_count),
        Some(total_tokens),
    );
    let mut log_json = serde_json::json!({
        "path": path_label,
        "prompt_tokens": input_tokens,
        "output_tokens": output_token_count,
    });
    if let Some((key, value)) = extra_log_fields {
        log_json[key] = serde_json::json!(value);
    }
    let _ = log.write(&log_json, Some(&usage));
    ProviderUsage::new(model_name, usage)
}

type StreamSender =
    tokio::sync::mpsc::Sender<Result<(Option<Message>, Option<ProviderUsage>), ProviderError>>;

pub struct LocalInferenceProvider {
    runtime: Arc<InferenceRuntime>,
    model: ModelSlot,
    model_config: ModelConfig,
    name: String,
}

impl LocalInferenceProvider {
    pub async fn from_env(model: ModelConfig, _extensions: Vec<ExtensionConfig>) -> Result<Self> {
        let runtime = InferenceRuntime::get_or_init()?;
        let model_slot = runtime.get_or_create_model_slot(&model.model_name);
        Ok(Self {
            runtime,
            model: model_slot,
            model_config: model,
            name: PROVIDER_NAME.to_string(),
        })
    }

    fn load_model_sync(
        runtime: &InferenceRuntime,
        model_id: &str,
        settings: &crate::providers::local_inference::local_model_registry::ModelSettings,
    ) -> Result<LoadedModel, ProviderError> {
        let resolved = resolve_model_path(model_id)
            .ok_or_else(|| ProviderError::ExecutionError(format!("Unknown model: {}", model_id)))?;
        let model_path = resolved.model_path;

        if !model_path.exists() {
            return Err(ProviderError::ExecutionError(format!(
                "Model not downloaded: {}. Please download it from Settings > Local Inference.",
                model_id
            )));
        }

        tracing::info!("Loading {} from: {}", model_id, model_path.display());

        let backend = runtime.backend();

        let mut params = LlamaModelParams::default();
        if let Some(n_gpu_layers) = settings.n_gpu_layers {
            params = params.with_n_gpu_layers(n_gpu_layers);
        }
        if settings.use_mlock {
            params = params.with_use_mlock(true);
        }
        let model = LlamaModel::load_from_file(backend, &model_path, &params)
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;

        let template = match model.chat_template(None) {
            Ok(t) => t,
            Err(_) => {
                tracing::warn!("Model has no embedded chat template, falling back to chatml");
                LlamaChatTemplate::new("chatml").map_err(|e| {
                    ProviderError::ExecutionError(format!(
                        "Failed to create fallback chat template: {}",
                        e
                    ))
                })?
            }
        };

        let mtmd_ctx = Self::init_mtmd_context(&model, &resolved.mmproj_path, settings);

        tracing::info!(model_id = model_id, "Model loaded successfully");

        Ok(LoadedModel {
            model,
            template,
            mtmd_ctx,
        })
    }

    fn init_mtmd_context(
        model: &LlamaModel,
        mmproj_path: &Option<PathBuf>,
        settings: &crate::providers::local_inference::local_model_registry::ModelSettings,
    ) -> Option<llama_cpp_2::mtmd::MtmdContext> {
        use llama_cpp_2::mtmd::{MtmdContext, MtmdContextParams};

        let mmproj_path = mmproj_path.as_ref().filter(|p| p.exists())?;

        let params = MtmdContextParams {
            use_gpu: true,
            n_threads: settings
                .n_threads
                .unwrap_or_else(|| MtmdContextParams::default().n_threads),
            ..MtmdContextParams::default()
        };

        match MtmdContext::init_from_file(mmproj_path.to_str().unwrap_or_default(), model, &params)
        {
            Ok(ctx) => {
                tracing::info!(
                    vision = ctx.support_vision(),
                    audio = ctx.support_audio(),
                    "Multimodal context initialized"
                );
                Some(ctx)
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to init multimodal context");
                None
            }
        }
    }
}

impl ProviderDef for LocalInferenceProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata
    where
        Self: Sized,
    {
        use crate::providers::local_inference::local_model_registry::{
            get_registry, FEATURED_MODELS,
        };

        let mut known_models: Vec<&str> = FEATURED_MODELS.iter().map(|m| m.spec).collect();

        // Add any registry models not already in the featured list
        let mut dynamic_models = Vec::new();
        if let Ok(registry) = get_registry().lock() {
            for entry in registry.list_models() {
                if !known_models.contains(&entry.id.as_str()) {
                    dynamic_models.push(entry.id.clone());
                }
            }
        }
        let dynamic_refs: Vec<&str> = dynamic_models.iter().map(|s| s.as_str()).collect();
        known_models.extend(dynamic_refs);

        ProviderMetadata::new(
            PROVIDER_NAME,
            "Local Inference",
            "Local inference using quantized GGUF models (llama.cpp)",
            DEFAULT_MODEL,
            known_models,
            "https://github.com/utilityai/llama-cpp-rs",
            vec![],
        )
    }

    fn from_env(
        model: ModelConfig,
        extensions: Vec<ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>>
    where
        Self: Sized,
    {
        Box::pin(Self::from_env(model, extensions))
    }
}

#[async_trait]
impl Provider for LocalInferenceProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model_config.clone()
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        use crate::providers::local_inference::local_model_registry::get_registry;

        let mut all_models: Vec<String> = Vec::new();

        if let Ok(registry) = get_registry().lock() {
            for entry in registry.list_models() {
                all_models.push(entry.id.clone());
            }
        }

        Ok(all_models)
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        _session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let resolved = resolve_model_path(&model_config.model_name).ok_or_else(|| {
            ProviderError::ExecutionError(format!("Model not found: {}", model_config.model_name))
        })?;
        let model_context_limit = resolved.context_limit;
        let model_settings = resolved.settings;

        // Ensure model is loaded — unload any other models first to free memory.
        {
            let mut model_lock = self.model.lock().await;
            if model_lock.is_none() {
                for slot in self.runtime.other_model_slots(&model_config.model_name) {
                    let mut other = slot.lock().await;
                    if other.is_some() {
                        tracing::info!("Unloading previous model to free memory");
                        *other = None;
                    }
                }

                let model_id = model_config.model_name.clone();
                let settings_for_load = model_settings.clone();
                let runtime_for_load = self.runtime.clone();
                let loaded = tokio::task::spawn_blocking(move || {
                    Self::load_model_sync(&runtime_for_load, &model_id, &settings_for_load)
                })
                .await
                .map_err(|e| ProviderError::ExecutionError(e.to_string()))??;
                *model_lock = Some(loaded);
            }
        }

        // Allow request_params to override thinking
        let mut model_settings = model_settings;
        if let Some(false) =
            model_config.get_config_param::<bool>("enable_thinking", "GOOSE_LOCAL_ENABLE_THINKING")
        {
            model_settings.enable_thinking = false;
        }

        // Use the model's native_tool_calling setting to decide the path.
        // Featured models have this set explicitly; user-added models default to false.
        let native_tool_calling = model_settings.native_tool_calling;
        let use_emulator = !native_tool_calling && !tools.is_empty();
        let system_prompt = if use_emulator {
            load_tiny_model_prompt()
        } else {
            system.to_string()
        };

        // Extract images for vision-capable models, replacing them with markers.
        // For non-vision models, leave messages unchanged (existing strip logic handles them).
        let has_vision = resolved.mmproj_path.is_some();
        let marker = llama_cpp_2::mtmd::mtmd_default_marker();
        let (images, vision_messages): (Vec<ExtractedImage>, Option<Vec<Message>>) = if has_vision {
            let (imgs, msgs) = multimodal::extract_images_from_messages(messages, marker);
            (imgs, Some(msgs))
        } else {
            (Vec::new(), None)
        };
        let effective_messages: &[Message] = vision_messages.as_deref().unwrap_or(messages);

        // Build chat messages for the template
        let mut chat_messages =
            vec![
                LlamaChatMessage::new("system".to_string(), system_prompt.clone()).map_err(
                    |e| {
                        ProviderError::ExecutionError(format!(
                            "Failed to create system message: {}",
                            e
                        ))
                    },
                )?,
            ];

        let code_mode_enabled = tools.iter().any(|t| t.name == CODE_EXECUTION_TOOL);

        if use_emulator && !tools.is_empty() {
            let tool_desc = build_emulator_tool_description(tools, code_mode_enabled);
            chat_messages = vec![LlamaChatMessage::new(
                "system".to_string(),
                format!("{}{}", system_prompt, tool_desc),
            )
            .map_err(|e| {
                ProviderError::ExecutionError(format!("Failed to create system message: {}", e))
            })?];
        }

        for msg in effective_messages {
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            let content = extract_text_content(msg);
            if !content.trim().is_empty() {
                chat_messages.push(LlamaChatMessage::new(role.to_string(), content).map_err(
                    |e| ProviderError::ExecutionError(format!("Failed to create message: {}", e)),
                )?);
            }
        }

        let (full_tools_json, compact_tools) = if !use_emulator && !tools.is_empty() {
            let full = format_tools(tools)
                .ok()
                .and_then(|spec| serde_json::to_string(&spec).ok());
            let compact = compact_tools_json(tools);
            (full, compact)
        } else {
            (None, None)
        };

        let oai_messages_json = if model_settings.use_jinja || native_tool_calling {
            Some(build_openai_messages_json(
                &system_prompt,
                effective_messages,
            ))
        } else {
            None
        };

        let model_arc = self.model.clone();
        let runtime = self.runtime.clone();
        let model_name = model_config.model_name.clone();
        let context_limit = model_context_limit;
        let settings = model_settings;
        let mmproj_path = resolved.mmproj_path.clone();

        let log_payload = serde_json::json!({
            "system": &system_prompt,
            "messages": messages.iter().map(|m| {
                serde_json::json!({
                    "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
                    "content": extract_text_content(m),
                })
            }).collect::<Vec<_>>(),
            "tools": tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
            "settings": {
                "use_jinja": settings.use_jinja,
                "native_tool_calling": settings.native_tool_calling,
                "context_size": settings.context_size,
                "sampling": settings.sampling,
            },
        });

        let mut log = RequestLog::start(&self.model_config, &log_payload)
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;

        let (tx, mut rx) = tokio::sync::mpsc::channel::<
            Result<(Option<Message>, Option<ProviderUsage>), ProviderError>,
        >(32);

        tokio::task::spawn_blocking(move || {
            // Macro to log errors before sending them through the channel
            macro_rules! send_err {
                ($err:expr) => {{
                    let err = $err;
                    let msg = match &err {
                        ProviderError::ExecutionError(s) => s.as_str(),
                        ProviderError::ContextLengthExceeded(s) => s.as_str(),
                        _ => "unknown error",
                    };
                    let _ = log.error(msg);
                    let _ = tx.blocking_send(Err(err));
                    return;
                }};
            }

            let mut model_guard = model_arc.blocking_lock();
            let loaded = match model_guard.as_mut() {
                Some(l) => l,
                None => {
                    send_err!(ProviderError::ExecutionError(
                        "Model not loaded".to_string()
                    ));
                }
            };

            // Lazily initialize the multimodal context if the vision encoder
            // was downloaded after the model was loaded.
            if !images.is_empty() && loaded.mtmd_ctx.is_none() {
                loaded.mtmd_ctx = LocalInferenceProvider::init_mtmd_context(
                    &loaded.model,
                    &mmproj_path,
                    &settings,
                );
            }

            let message_id = Uuid::new_v4().to_string();

            let mut gen_ctx = GenerationContext {
                loaded,
                runtime: &runtime,
                chat_messages: &chat_messages,
                settings: &settings,
                context_limit,
                model_name,
                message_id: &message_id,
                tx: &tx,
                log: &mut log,
                images: &images,
            };

            let result = if use_emulator {
                generate_with_emulated_tools(&mut gen_ctx, code_mode_enabled)
            } else {
                generate_with_native_tools(
                    &mut gen_ctx,
                    &oai_messages_json,
                    full_tools_json.as_deref(),
                    compact_tools.as_deref(),
                )
            };

            if let Err(err) = result {
                let msg = match &err {
                    ProviderError::ExecutionError(s) => s.as_str(),
                    ProviderError::ContextLengthExceeded(s) => s.as_str(),
                    _ => "unknown error",
                };
                let _ = log.error(msg);
                let _ = tx.blocking_send(Err(err));
            }
        });

        Ok(Box::pin(try_stream! {
            while let Some(result) = rx.recv().await {
                let item = result?;
                yield item;
            }

        }))
    }
}
