#[cfg(feature = "mlx")]
mod imp {
    use std::any::Any;
    use std::path::{Path, PathBuf};

    use mlx_lm::cache::ConcatKeyValueCache;
    use mlx_lm::gemma4_mtp::generate_gemma4_mtp;
    use mlx_lm::models::{gemma4_assistant::load_gemma4_assistant_model, LoadedModel, Model};
    use mlx_lm_utils::tokenizer::{Chat, Conversation, Role};
    use mlx_rs::transforms::eval;
    use serde_json::json;

    use crate::conversation::message::Message;
    use crate::providers::base::{DraftStats, ProviderStats, ProviderUsage, Usage};
    use crate::providers::errors::ProviderError;
    use crate::providers::local_inference::backend::{
        BackendLoadedModel, LocalGenerationRequest, LocalInferenceBackend,
    };
    use crate::providers::local_inference::local_model_registry::ModelSettings;
    use crate::providers::local_inference::native_tool_parsing::message_from_native_tool_text;
    use crate::providers::local_inference::tool_emulation::{
        build_emulator_tool_description, load_tiny_model_prompt, message_for_emulator_action,
        StreamingEmulatorParser, CODE_EXECUTION_TOOL,
    };
    use crate::providers::local_inference::{extract_text_content, ResolvedModelPaths};
    use crate::providers::utils::filter_extensions_from_system_prompt;

    pub(in crate::providers::local_inference) const MLX_BACKEND_ID: &str = "mlx";

    pub(in crate::providers::local_inference) struct MlxBackend;

    impl MlxBackend {
        pub(in crate::providers::local_inference) fn new() -> Self {
            Self
        }
    }

    impl LocalInferenceBackend for MlxBackend {
        fn id(&self) -> &'static str {
            MLX_BACKEND_ID
        }

        fn load_model(
            &self,
            model_id: &str,
            resolved: &ResolvedModelPaths,
            _settings: &ModelSettings,
        ) -> Result<Box<dyn BackendLoadedModel>, ProviderError> {
            if !resolved.model_path.exists() {
                return Err(ProviderError::ExecutionError(format!(
                    "Model not downloaded: {}. Please download it from Settings > Local Inference.",
                    model_id
                )));
            }

            let model_dir = model_dir_from_path(&resolved.model_path)?;
            let model = LoadedModel::load(&model_dir).map_err(mlx_error)?;
            tracing::info!(
                backend = self.id(),
                model_id,
                model_type = model.model_type(),
                "MLX model loaded successfully"
            );
            Ok(Box::new(MlxLoadedModel { model, model_dir }))
        }

        fn generate(
            &self,
            loaded: &mut dyn BackendLoadedModel,
            request: LocalGenerationRequest<'_>,
        ) -> Result<(), ProviderError> {
            let loaded = loaded
                .as_any_mut()
                .downcast_mut::<MlxLoadedModel>()
                .ok_or_else(|| {
                    ProviderError::ExecutionError("Loaded model backend mismatch".to_string())
                })?;

            let tool_mode = if request.tools.is_empty() {
                ToolMode::None
            } else if request.settings.native_tool_calling {
                ToolMode::Native
            } else {
                ToolMode::Emulated {
                    code_mode_enabled: request.tools.iter().any(|t| t.name == CODE_EXECUTION_TOOL),
                }
            };
            let prompt = build_prompt(
                &mut loaded.model,
                &request.model_name,
                request.system,
                request.messages,
                request.tools,
                tool_mode,
            )?;
            let prompt_tokens = loaded.model.encode(&prompt, false).map_err(mlx_error)?;
            if prompt_tokens.len() >= request.context_limit && request.context_limit > 0 {
                return Err(ProviderError::ContextLengthExceeded(format!(
                    "Prompt ({} tokens) exceeds context limit ({} tokens). Try reducing conversation length.",
                    prompt_tokens.len(), request.context_limit
                )));
            }

            let prompt_array = loaded
                .model
                .encode_to_array(&prompt, false)
                .map_err(mlx_error)?;
            let max_tokens = request
                .settings
                .max_output_tokens
                .or_else(|| {
                    request
                        .max_tokens
                        .and_then(|tokens| usize::try_from(tokens).ok())
                })
                .unwrap_or(512);
            let temp = request
                .temperature
                .unwrap_or_else(|| temperature(request.settings));
            let eos_token_ids = loaded.model.eos_token_ids().to_vec();
            let generation_started = std::time::Instant::now();
            let (generated_ids, draft_stats) =
                if let Some(draft_model_path) = &request.draft_model_path {
                    if matches!(loaded.model.model_mut(), Model::Gemma4(_)) {
                        let mut assistant =
                            load_gemma4_assistant_model(draft_model_path).map_err(|error| {
                                mlx_error(format!("failed to load MLX draft model: {error}"))
                            })?;
                        let target = match loaded.model.model_mut() {
                            Model::Gemma4(target) => target,
                            _ => unreachable!(),
                        };
                        let (ids, stats) = generate_gemma4_mtp(
                            target,
                            &mut assistant,
                            &prompt_array,
                            &eos_token_ids,
                            max_tokens,
                            temp,
                        )
                        .map_err(mlx_error)?;
                        (
                            ids,
                            Some(DraftStats {
                                model: Some(draft_model_path.display().to_string()),
                                draft_tokens: stats.draft_tokens,
                                accepted_tokens: stats.accepted_tokens,
                                target_tokens: stats.target_tokens,
                                rounds: stats.rounds,
                                accept_rate: stats.accept_rate(),
                            }),
                        )
                    } else {
                        generate_single_model(
                            &mut loaded.model,
                            &prompt_array,
                            &eos_token_ids,
                            max_tokens,
                            temp,
                        )?
                    }
                } else {
                    generate_single_model(
                        &mut loaded.model,
                        &prompt_array,
                        &eos_token_ids,
                        max_tokens,
                        temp,
                    )?
                };

            let generated_text = loaded
                .model
                .decode(&generated_ids, true)
                .map_err(mlx_error)?;
            emit_generated_response(&generated_text, request.message_id, tool_mode, request.tx)?;

            let output_tokens = generated_ids.len() as i32;
            let input_tokens = prompt_tokens.len() as i32;
            let usage = Usage::new(
                Some(input_tokens),
                Some(output_tokens),
                Some(input_tokens + output_tokens),
            );
            let log_json = serde_json::json!({
                "path": "mlx",
                "model_dir": loaded.model_dir,
                "prompt_tokens": input_tokens,
                "output_tokens": output_tokens,
                "generated_text": generated_text,
                "draft": draft_stats,
            });
            let _ = request.log.write(&log_json, Some(&usage));
            let stats = ProviderStats {
                time_to_first_token_ms: None,
                elapsed_ms: Some(generation_started.elapsed().as_millis() as u64),
                output_tokens: Some(generated_ids.len()),
                draft: draft_stats,
            };
            let provider_usage = ProviderUsage::new(request.model_name, usage).with_stats(stats);
            let _ = request.tx.blocking_send(Ok((None, Some(provider_usage))));
            Ok(())
        }

        fn available_memory_bytes(&self) -> u64 {
            0
        }
    }

    #[derive(Clone, Copy)]
    enum ToolMode {
        None,
        Native,
        Emulated { code_mode_enabled: bool },
    }

    struct MlxLoadedModel {
        model: LoadedModel,
        model_dir: PathBuf,
    }

    impl BackendLoadedModel for MlxLoadedModel {
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    fn model_dir_from_path(path: &Path) -> Result<PathBuf, ProviderError> {
        if path.is_dir() {
            Ok(path.to_path_buf())
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| mlx_error("MLX model path has no parent directory"))
        }
    }

    fn build_prompt(
        model: &mut LoadedModel,
        model_name: &str,
        system: &str,
        messages: &[Message],
        tools: &[rmcp::model::Tool],
        tool_mode: ToolMode,
    ) -> Result<String, ProviderError> {
        match tool_mode {
            ToolMode::Native => {
                let conversations = openai_messages(system, messages);
                let tool_specs = crate::providers::formats::openai::format_tools(tools)
                    .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;
                if let Some(prompt) = model
                    .apply_chat_template_json([conversations], Some(&tool_specs), true)
                    .map_err(mlx_error)?
                {
                    return Ok(prompt);
                }

                Ok(render_prompt(system, messages))
            }
            ToolMode::Emulated { code_mode_enabled } => {
                let system_prompt = format!(
                    "{}{}",
                    load_tiny_model_prompt(),
                    build_emulator_tool_description(tools, code_mode_enabled)
                );
                if is_gemma4(model) {
                    let conversations = gemma4_messages_with_system(&system_prompt, messages);
                    if let Some(prompt) = model
                        .apply_chat_template_json([conversations], None, true)
                        .map_err(mlx_error)?
                    {
                        return Ok(prompt);
                    }
                }

                let conversations = chat_conversations(&system_prompt, messages);
                if let Some(prompt) = model
                    .apply_chat_template([Chat::Owned(conversations)], None, true)
                    .map_err(mlx_error)?
                {
                    return Ok(prompt);
                }

                Ok(render_prompt(&system_prompt, messages))
            }
            ToolMode::None => {
                if is_gemma4(model) {
                    let conversations = gemma4_messages(model_name, system, messages);
                    if let Some(prompt) = model
                        .apply_chat_template_json([conversations], None, true)
                        .map_err(mlx_error)?
                    {
                        return Ok(prompt);
                    }
                }

                let conversations = chat_conversations(system, messages);
                if let Some(prompt) = model
                    .apply_chat_template([Chat::Owned(conversations)], None, true)
                    .map_err(mlx_error)?
                {
                    return Ok(prompt);
                }

                Ok(render_prompt(system, messages))
            }
        }
    }

    fn generate_single_model(
        model: &mut LoadedModel,
        prompt_array: &mlx_rs::Array,
        eos_token_ids: &[u32],
        max_tokens: usize,
        temp: f32,
    ) -> Result<(Vec<u32>, Option<DraftStats>), ProviderError> {
        let mut cache: Vec<Option<ConcatKeyValueCache>> = Vec::new();
        let mut generated_ids = Vec::new();
        {
            let generator = model
                .generate(&mut cache, temp, prompt_array)
                .take(max_tokens);
            for token in generator {
                let token = token.map_err(mlx_error)?;
                eval([&token]).map_err(mlx_error)?;
                let token_id = token.item::<u32>();
                if eos_token_ids.contains(&token_id) {
                    break;
                }
                generated_ids.push(token_id);
            }
        }
        Ok((generated_ids, None))
    }

    fn is_gemma4(model: &LoadedModel) -> bool {
        matches!(model.model_type(), "gemma4" | "gemma4_text")
    }

    fn gemma4_messages(
        model_name: &str,
        system: &str,
        messages: &[Message],
    ) -> Vec<serde_json::Value> {
        let system = gemma4_system_prompt(model_name, system);
        gemma4_messages_with_optional_system(system.as_deref(), messages)
    }

    fn gemma4_messages_with_system(system: &str, messages: &[Message]) -> Vec<serde_json::Value> {
        gemma4_messages_with_optional_system(Some(system), messages)
    }

    fn gemma4_messages_with_optional_system(
        system: Option<&str>,
        messages: &[Message],
    ) -> Vec<serde_json::Value> {
        let mut values = Vec::new();
        if let Some(system) = system.map(str::trim).filter(|system| !system.is_empty()) {
            values.push(json!({
                "role": "system",
                "content": system,
            }));
        }

        for message in messages.iter().filter(|message| message.is_agent_visible()) {
            let text = extract_text_content(message);
            if text.trim().is_empty() {
                continue;
            }

            match message.role {
                rmcp::model::Role::User => values.push(json!({
                    "role": "user",
                    "content": [{"type": "text", "text": text.trim(), "content": text.trim()}],
                })),
                rmcp::model::Role::Assistant => values.push(json!({
                    "role": "assistant",
                    "content": text.trim(),
                })),
            }
        }

        values
    }

    fn gemma4_system_prompt(model_name: &str, system: &str) -> Option<String> {
        if should_use_tiny_system_prompt(model_name) {
            return Some(load_tiny_model_prompt());
        }

        let filtered = filter_extensions_from_system_prompt(system);
        let system = filtered.trim();
        if system.is_empty() {
            None
        } else {
            Some(system.to_string())
        }
    }

    fn should_use_tiny_system_prompt(model_name: &str) -> bool {
        estimate_model_size_billions(model_name).is_some_and(|size| size <= 4.0)
    }

    fn estimate_model_size_billions(model_name: &str) -> Option<f32> {
        let normalized = model_name.to_ascii_lowercase().replace('-', "_");
        for part in normalized.split('_') {
            if let Some(value) = part.strip_suffix('b') {
                if let Ok(size) = value.parse::<f32>() {
                    return Some(size);
                }
            }
            if let Some(value) = part
                .strip_prefix('e')
                .and_then(|value| value.strip_suffix('b'))
            {
                if let Ok(size) = value.parse::<f32>() {
                    return Some(size);
                }
            }
        }
        None
    }

    fn openai_messages(system: &str, messages: &[Message]) -> Vec<serde_json::Value> {
        let mut values = vec![serde_json::json!({
            "role": "system",
            "content": system,
        })];
        values.extend(crate::providers::formats::openai::format_messages(
            messages,
            &crate::providers::utils::ImageFormat::OpenAi,
        ));
        values
    }

    fn chat_conversations(system: &str, messages: &[Message]) -> Vec<Conversation<Role, String>> {
        let mut conversations = Vec::new();
        if !system.trim().is_empty() {
            conversations.push(Conversation {
                role: Role::System,
                content: system.trim().to_string(),
            });
        }
        for message in messages.iter().filter(|message| message.is_agent_visible()) {
            let role = match message.role {
                rmcp::model::Role::User => Role::User,
                rmcp::model::Role::Assistant => Role::Assistant,
            };
            let text = extract_text_content(message);
            if !text.trim().is_empty() {
                conversations.push(Conversation {
                    role,
                    content: text.trim().to_string(),
                });
            }
        }
        conversations
    }

    fn emit_generated_response(
        generated_text: &str,
        message_id: &str,
        tool_mode: ToolMode,
        tx: &tokio::sync::mpsc::Sender<
            Result<(Option<Message>, Option<ProviderUsage>), ProviderError>,
        >,
    ) -> Result<(), ProviderError> {
        if generated_text.is_empty() {
            return Ok(());
        }

        match tool_mode {
            ToolMode::None => {
                let mut msg = Message::assistant().with_text(generated_text);
                msg.id = Some(message_id.to_string());
                tx.blocking_send(Ok((Some(msg), None))).map_err(|_| {
                    ProviderError::ExecutionError("Failed to stream MLX response".to_string())
                })?;
            }
            ToolMode::Native => {
                if let Some(message) = message_from_native_tool_text(generated_text, message_id)? {
                    tx.blocking_send(Ok((Some(message), None))).map_err(|_| {
                        ProviderError::ExecutionError("Failed to stream MLX response".to_string())
                    })?;
                } else {
                    let mut msg = Message::assistant().with_text(generated_text);
                    msg.id = Some(message_id.to_string());
                    tx.blocking_send(Ok((Some(msg), None))).map_err(|_| {
                        ProviderError::ExecutionError("Failed to stream MLX response".to_string())
                    })?;
                }
            }
            ToolMode::Emulated { code_mode_enabled } => {
                let mut parser = StreamingEmulatorParser::new(code_mode_enabled);
                let mut actions = parser.process_chunk(generated_text);
                actions.extend(parser.flush());

                for action in actions {
                    let (message, _) = message_for_emulator_action(&action, message_id);
                    tx.blocking_send(Ok((Some(message), None))).map_err(|_| {
                        ProviderError::ExecutionError("Failed to stream MLX response".to_string())
                    })?;
                }
            }
        }
        Ok(())
    }

    fn temperature(settings: &ModelSettings) -> f32 {
        match &settings.sampling {
            crate::providers::local_inference::local_model_registry::SamplingConfig::Greedy => 0.0,
            crate::providers::local_inference::local_model_registry::SamplingConfig::Temperature {
                temperature,
                ..
            } => *temperature,
            crate::providers::local_inference::local_model_registry::SamplingConfig::MirostatV2 {
                ..
            } => 0.0,
        }
    }

    fn render_prompt(system: &str, messages: &[Message]) -> String {
        let mut prompt = String::new();
        if !system.trim().is_empty() {
            prompt.push_str("System: ");
            prompt.push_str(system.trim());
            prompt.push('\n');
        }
        for message in messages.iter().filter(|message| message.is_agent_visible()) {
            let role = match message.role {
                rmcp::model::Role::User => "User",
                rmcp::model::Role::Assistant => "Assistant",
            };
            let text = extract_text_content(message);
            if !text.trim().is_empty() {
                prompt.push_str(role);
                prompt.push_str(": ");
                prompt.push_str(text.trim());
                prompt.push('\n');
            }
        }
        prompt.push_str("Assistant: ");
        prompt
    }

    fn mlx_error(error: impl std::fmt::Display) -> ProviderError {
        ProviderError::ExecutionError(format!("MLX backend error: {}", error))
    }
}

#[cfg(not(feature = "mlx"))]
mod imp {
    use crate::providers::errors::ProviderError;
    use crate::providers::local_inference::backend::{
        BackendLoadedModel, LocalGenerationRequest, LocalInferenceBackend,
    };
    use crate::providers::local_inference::local_model_registry::ModelSettings;
    use crate::providers::local_inference::ResolvedModelPaths;

    pub(in crate::providers::local_inference) const MLX_BACKEND_ID: &str = "mlx";

    pub(in crate::providers::local_inference) struct MlxBackend;

    impl MlxBackend {
        pub(in crate::providers::local_inference) fn new() -> Self {
            Self
        }
    }

    impl LocalInferenceBackend for MlxBackend {
        fn id(&self) -> &'static str {
            MLX_BACKEND_ID
        }

        fn load_model(
            &self,
            _model_id: &str,
            _resolved: &ResolvedModelPaths,
            _settings: &ModelSettings,
        ) -> Result<Box<dyn BackendLoadedModel>, ProviderError> {
            Err(ProviderError::ExecutionError(
                "MLX backend support was not compiled in. Rebuild with the `mlx` feature."
                    .to_string(),
            ))
        }

        fn generate(
            &self,
            _loaded: &mut dyn BackendLoadedModel,
            _request: LocalGenerationRequest<'_>,
        ) -> Result<(), ProviderError> {
            Err(ProviderError::ExecutionError(
                "MLX backend support was not compiled in. Rebuild with the `mlx` feature."
                    .to_string(),
            ))
        }

        fn available_memory_bytes(&self) -> u64 {
            0
        }
    }
}

pub(in crate::providers::local_inference) use imp::{MlxBackend, MLX_BACKEND_ID};
