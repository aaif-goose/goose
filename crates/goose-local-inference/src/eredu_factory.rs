use crate::{
    backend::{BackendLoadedModel, LocalGenerationRequest, LocalInferenceBackend},
    eredu_adapter::{error, generation::Request, WorkerHandle},
    model::ModelSettings,
    ResolvedModelPaths,
};
use goose_provider_types::{errors::ProviderError, request_log::LoggerHandleExt};
use std::any::Any;

use crate::selection::EREDU_BACKEND_ID;
pub(crate) struct EreduBackend;

impl BackendLoadedModel for WorkerHandle {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn reusable_after_error(&self) -> bool {
        false
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
        if settings.n_batch.is_some()
            || settings.n_gpu_layers.is_some()
            || settings.n_threads.is_some()
            || settings.flash_attention.is_some()
            || settings.use_mlock
        {
            return Err(error("Batch size, GPU layer count, thread count, flash-attention and mlock controls belong to llama.cpp. Clear those explicit settings to use Eredu's execution planner."));
        }
        if settings.draft_model.is_some() && resolved.draft_model_path.is_none() {
            return Err(error("The selected draft model is not downloaded"));
        }
        #[cfg(all(feature = "mlx", target_os = "macos"))]
        {
            WorkerHandle::spawn(
                eredu_backend_mlx::MlxBackendFactory::default,
                resolved.model_path.clone(),
                settings.clone(),
                resolved.draft_model_path.clone(),
            )
            .map(|worker| Box::new(worker) as Box<dyn BackendLoadedModel>)
        }
        #[cfg(not(all(feature = "mlx", target_os = "macos")))]
        {
            let _ = (resolved, settings);
            Err(error(
                "The Eredu backend requires macOS and a build with the mlx feature",
            ))
        }
    }

    fn generate(
        &self,
        loaded: &mut dyn BackendLoadedModel,
        request: LocalGenerationRequest<'_>,
    ) -> Result<(), ProviderError> {
        let worker = loaded
            .as_any_mut()
            .downcast_mut::<WorkerHandle>()
            .ok_or_else(|| error("loaded model backend mismatch"))?;
        let cancellation = eredu_core::GenerationCancellationToken::new();
        let closed = request.tx.clone();
        let disconnect = cancellation.clone();
        let monitor = tokio::runtime::Handle::current().spawn(async move {
            closed.closed().await;
            disconnect.cancel();
        });
        let result = worker.generate(
            Request {
                model: request.model_config.clone(),
                settings: request.settings.clone(),
                system: request.system.to_owned(),
                messages: request.messages.to_vec(),
                tools: request.tools.to_vec(),
                message_id: request.message_id.to_owned(),
                model_load_ms: request.model_load_ms,
            },
            request.tx.clone(),
            cancellation,
        );
        monitor.abort();
        let mut usage = result?;
        usage
            .additional_data
            .get_or_insert_with(Default::default)
            .insert(
                "eredu_execution_plan".into(),
                serde_json::to_value(&worker.report).map_err(error)?,
            );
        let _ = request.log.write(&serde_json::json!({"path": "eredu", "execution_plan": worker.report, "inspection": worker.inspection, "stats": usage.stats, "finish_reasons": usage.finish_reasons}), Some(&usage.usage));
        let _ = request.tx.blocking_send(Ok((None, Some(usage))));
        Ok(())
    }

    fn available_memory_bytes(&self) -> u64 {
        0
    }
}
