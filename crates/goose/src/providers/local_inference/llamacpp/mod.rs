mod inference_emulated_tools;
mod inference_engine;
mod inference_native_tools;

use std::any::Any;
use std::path::PathBuf;

use anyhow::Result;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::{list_llama_ggml_backend_devices, LlamaBackendDeviceType, LogOptions};
use rmcp::model::Role;

use self::inference_emulated_tools::{
    build_emulator_tool_description, generate_with_emulated_tools, load_tiny_model_prompt,
};
use self::inference_engine::{GenerationContext, LoadedModel};
use self::inference_native_tools::generate_with_native_tools;
use crate::providers::errors::ProviderError;
use crate::providers::formats::openai::format_tools;
use crate::providers::local_inference::backend::{
    BackendLoadedModel, LocalGenerationRequest, LocalInferenceBackend,
};
use crate::providers::local_inference::multimodal::ExtractedImage;
use crate::providers::local_inference::tool_parsing::compact_tools_json;
use crate::providers::local_inference::{
    build_openai_messages_json, extract_text_content, ResolvedModelPaths,
};

pub(super) const LLAMACPP_BACKEND_ID: &str = "llamacpp";

const CODE_EXECUTION_TOOL: &str = "code_execution__execute_typescript";

pub(super) struct LlamaCppBackend {
    backend: LlamaBackend,
}

impl LlamaCppBackend {
    pub(super) fn new() -> Result<Self> {
        let backend = match LlamaBackend::init() {
            Ok(backend) => backend,
            Err(llama_cpp_2::LlamaCppError::BackendAlreadyInitialized) => {
                unreachable!(
                    "LlamaBackend already initialized but Weak was dead; \
                     the runtime mutex prevents concurrent re-init"
                )
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to initialize local inference runtime");
                return Err(anyhow::anyhow!("Failed to init llama backend: {}", e));
            }
        };

        llama_cpp_2::send_logs_to_tracing(LogOptions::default());
        log_inference_backend_devices();

        Ok(Self { backend })
    }

    pub(super) fn llama_backend(&self) -> &LlamaBackend {
        &self.backend
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

impl LocalInferenceBackend for LlamaCppBackend {
    fn id(&self) -> &'static str {
        LLAMACPP_BACKEND_ID
    }

    fn load_model(
        &self,
        model_id: &str,
        resolved: &ResolvedModelPaths,
        settings: &crate::providers::local_inference::local_model_registry::ModelSettings,
    ) -> Result<Box<dyn BackendLoadedModel>, ProviderError> {
        let model_path = &resolved.model_path;

        if !model_path.exists() {
            return Err(ProviderError::ExecutionError(format!(
                "Model not downloaded: {}. Please download it from Settings > Local Inference.",
                model_id
            )));
        }

        tracing::info!(
            backend = self.id(),
            "Loading {} from: {}",
            model_id,
            model_path.display()
        );

        let mut params = LlamaModelParams::default();
        if let Some(n_gpu_layers) = settings.n_gpu_layers {
            params = params.with_n_gpu_layers(n_gpu_layers);
        }
        if settings.use_mlock {
            params = params.with_use_mlock(true);
        }
        let model = LlamaModel::load_from_file(&self.backend, model_path, &params)
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

        tracing::info!(
            backend = self.id(),
            model_id = model_id,
            "Model loaded successfully"
        );

        Ok(Box::new(LoadedModel {
            model,
            template,
            mtmd_ctx,
        }))
    }

    fn generate(
        &self,
        loaded: &mut dyn BackendLoadedModel,
        request: LocalGenerationRequest<'_>,
    ) -> Result<(), ProviderError> {
        let loaded = loaded
            .as_any_mut()
            .downcast_mut::<LoadedModel>()
            .ok_or_else(|| {
                ProviderError::ExecutionError("Loaded model backend mismatch".to_string())
            })?;

        let native_tool_calling = request.settings.native_tool_calling;
        let use_emulator = !native_tool_calling && !request.tools.is_empty();
        let system_prompt = if use_emulator {
            load_tiny_model_prompt()
        } else {
            request.system.to_string()
        };

        let has_vision = request.resolved_model.mmproj_path.is_some();
        let marker = llama_cpp_2::mtmd::mtmd_default_marker();
        let (images, vision_messages): (Vec<ExtractedImage>, Option<Vec<_>>) = if has_vision {
            let (imgs, msgs) =
                super::multimodal::extract_images_from_messages(request.messages, marker);
            (imgs, Some(msgs))
        } else {
            (Vec::new(), None)
        };
        let effective_messages = vision_messages.as_deref().unwrap_or(request.messages);

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

        let code_mode_enabled = request.tools.iter().any(|t| t.name == CODE_EXECUTION_TOOL);

        if use_emulator && !request.tools.is_empty() {
            let tool_desc = build_emulator_tool_description(request.tools, code_mode_enabled);
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

        let (full_tools_json, compact_tools) = if !use_emulator && !request.tools.is_empty() {
            let full = format_tools(request.tools)
                .ok()
                .and_then(|spec| serde_json::to_string(&spec).ok());
            let compact = compact_tools_json(request.tools);
            (full, compact)
        } else {
            (None, None)
        };

        let oai_messages_json = if request.settings.use_jinja || native_tool_calling {
            Some(build_openai_messages_json(
                &system_prompt,
                effective_messages,
            ))
        } else {
            None
        };

        if !images.is_empty() && loaded.mtmd_ctx.is_none() {
            loaded.mtmd_ctx = Self::init_mtmd_context(
                &loaded.model,
                &request.resolved_model.mmproj_path,
                request.settings,
            );
        }

        let mut gen_ctx = GenerationContext {
            loaded,
            backend: self,
            chat_messages: &chat_messages,
            settings: request.settings,
            context_limit: request.context_limit,
            model_name: request.model_name,
            message_id: request.message_id,
            tx: request.tx,
            log: request.log,
            images: &images,
        };

        if use_emulator {
            generate_with_emulated_tools(&mut gen_ctx, code_mode_enabled)
        } else {
            generate_with_native_tools(
                &mut gen_ctx,
                &oai_messages_json,
                full_tools_json.as_deref(),
                compact_tools.as_deref(),
            )
        }
    }

    fn available_memory_bytes(&self) -> u64 {
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
}

impl BackendLoadedModel for LoadedModel {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
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
