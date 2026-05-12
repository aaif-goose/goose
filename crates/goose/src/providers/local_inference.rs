mod backend;
pub mod hf_models;
mod llamacpp;
pub mod local_model_registry;
pub(crate) mod multimodal;
mod tool_parsing;

use crate::config::ExtensionConfig;
use crate::conversation::message::{Message, MessageContent};
use crate::model::ModelConfig;
use crate::providers::base::{
    MessageStream, Provider, ProviderDef, ProviderMetadata, ProviderUsage, Usage,
};
use crate::providers::errors::ProviderError;
use crate::providers::utils::RequestLog;
use anyhow::Result;
use async_stream::try_stream;
use async_trait::async_trait;
use backend::{BackendLoadedModel, LocalInferenceBackend};
use futures::future::BoxFuture;
use llamacpp::{LlamaCppBackend, LLAMACPP_BACKEND_ID};
use rmcp::model::Tool;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, Weak};
use tokio::sync::Mutex;
use uuid::Uuid;

type ModelSlot = Arc<Mutex<Option<Box<dyn BackendLoadedModel>>>>;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ModelCacheKey {
    backend_id: &'static str,
    model_id: String,
}

impl ModelCacheKey {
    fn new(backend_id: &'static str, model_id: impl Into<String>) -> Self {
        Self {
            backend_id,
            model_id: model_id.into(),
        }
    }
}

pub struct InferenceRuntime {
    models: StdMutex<HashMap<ModelCacheKey, ModelSlot>>,
    backends: HashMap<&'static str, Arc<dyn LocalInferenceBackend>>,
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
        let llamacpp_backend: Arc<dyn LocalInferenceBackend> = Arc::new(LlamaCppBackend::new()?);
        let mut backends = HashMap::new();
        backends.insert(LLAMACPP_BACKEND_ID, llamacpp_backend);
        let runtime = Arc::new(Self {
            models: StdMutex::new(HashMap::new()),
            backends,
        });
        *guard = Arc::downgrade(&runtime);
        Ok(runtime)
    }

    fn default_backend(&self) -> &dyn LocalInferenceBackend {
        self.backends
            .get(LLAMACPP_BACKEND_ID)
            .expect("default local inference backend registered")
            .as_ref()
    }

    fn backend_for_model(
        &self,
        _resolved: &ResolvedModelPaths,
    ) -> Result<Arc<dyn LocalInferenceBackend>, ProviderError> {
        self.backends
            .get(LLAMACPP_BACKEND_ID)
            .cloned()
            .ok_or_else(|| {
                ProviderError::ExecutionError("Local inference backend unavailable".to_string())
            })
    }

    fn get_or_create_model_slot(&self, key: ModelCacheKey) -> ModelSlot {
        let mut map = self.models.lock().expect("model cache lock poisoned");
        map.entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    }

    fn other_model_slots(&self, keep_key: &ModelCacheKey) -> Vec<ModelSlot> {
        let map = self.models.lock().expect("model cache lock poisoned");
        map.iter()
            .filter(|(key, _)| *key != keep_key)
            .map(|(_, slot)| slot.clone())
            .collect()
    }
}

const PROVIDER_NAME: &str = "local";
const DEFAULT_MODEL: &str = "bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M";

pub const LOCAL_LLM_MODEL_CONFIG_KEY: &str = "LOCAL_LLM_MODEL";

#[derive(Clone)]
pub(super) struct ResolvedModelPaths {
    pub model_path: PathBuf,
    pub context_limit: usize,
    pub settings: crate::providers::local_inference::local_model_registry::ModelSettings,
    pub mmproj_path: Option<PathBuf>,
}

/// Resolve model path, context limit, settings, and mmproj path for a model ID from the registry.
fn resolve_model_path(model_id: &str) -> Option<ResolvedModelPaths> {
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

pub fn available_inference_memory_bytes(runtime: &InferenceRuntime) -> u64 {
    runtime.default_backend().available_memory_bytes()
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
    model_config: ModelConfig,
    name: String,
}

impl LocalInferenceProvider {
    pub async fn from_env(model: ModelConfig, _extensions: Vec<ExtensionConfig>) -> Result<Self> {
        let runtime = InferenceRuntime::get_or_init()?;
        Ok(Self {
            runtime,
            model_config: model,
            name: PROVIDER_NAME.to_string(),
        })
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
        let backend = self.runtime.backend_for_model(&resolved)?;
        let model_context_limit = resolved.context_limit;
        let model_settings = resolved.settings.clone();
        let cache_key = ModelCacheKey::new(backend.id(), model_config.model_name.clone());
        let model_slot = self.runtime.get_or_create_model_slot(cache_key.clone());

        // Ensure model is loaded — unload any other models first to free memory.
        {
            let mut model_lock = model_slot.lock().await;
            if model_lock.is_none() {
                for slot in self.runtime.other_model_slots(&cache_key) {
                    let mut other = slot.lock().await;
                    if other.is_some() {
                        tracing::info!("Unloading previous model to free memory");
                        *other = None;
                    }
                }

                let model_id = model_config.model_name.clone();
                let resolved_for_load = resolved.clone();
                let settings_for_load = model_settings.clone();
                let backend_for_load = backend.clone();
                let loaded = tokio::task::spawn_blocking(move || {
                    backend_for_load.load_model(&model_id, &resolved_for_load, &settings_for_load)
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

        let model_arc = model_slot.clone();
        let backend = backend.clone();
        let model_name = model_config.model_name.clone();
        let context_limit = model_context_limit;
        let settings = model_settings;
        let resolved_model = resolved.clone();
        let system = system.to_string();
        let messages = messages.to_vec();
        let tools = tools.to_vec();
        let log_payload = serde_json::json!({
            "system": &system,
            "messages": messages.iter().map(|m| {
                serde_json::json!({
                    "role": match m.role { rmcp::model::Role::User => "user", rmcp::model::Role::Assistant => "assistant" },
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

            let message_id = Uuid::new_v4().to_string();

            let request = backend::LocalGenerationRequest {
                model_name,
                system: &system,
                messages: &messages,
                tools: &tools,
                settings: &settings,
                context_limit,
                resolved_model: &resolved_model,
                message_id: &message_id,
                tx: &tx,
                log: &mut log,
            };

            let result = backend.generate(loaded.as_mut(), request);

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
