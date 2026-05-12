use rmcp::model::Tool;
use std::any::Any;

use crate::conversation::message::Message;
use crate::providers::errors::ProviderError;
use crate::providers::local_inference::local_model_registry::ModelSettings;
use crate::providers::utils::RequestLog;

use super::{ResolvedModelPaths, StreamSender};

pub(super) trait BackendLoadedModel: Send {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub(super) struct LocalGenerationRequest<'a> {
    pub model_name: String,
    pub system: &'a str,
    pub messages: &'a [Message],
    pub tools: &'a [Tool],
    pub settings: &'a ModelSettings,
    pub context_limit: usize,
    pub resolved_model: &'a ResolvedModelPaths,
    pub message_id: &'a str,
    pub tx: &'a StreamSender,
    pub log: &'a mut RequestLog,
}

pub(super) trait LocalInferenceBackend: Send + Sync {
    fn id(&self) -> &'static str;

    fn load_model(
        &self,
        model_id: &str,
        resolved: &ResolvedModelPaths,
        settings: &ModelSettings,
    ) -> Result<Box<dyn BackendLoadedModel>, ProviderError>;

    fn generate(
        &self,
        loaded: &mut dyn BackendLoadedModel,
        request: LocalGenerationRequest<'_>,
    ) -> Result<(), ProviderError>;

    fn available_memory_bytes(&self) -> u64;
}
