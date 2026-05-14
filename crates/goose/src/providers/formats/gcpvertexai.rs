use super::{anthropic, google};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::base::Usage;
use anyhow::{Context, Result};
use rmcp::model::Tool;
use serde_json::Value;

use std::fmt;

pub type StreamingMessageStream = std::pin::Pin<
    Box<
        dyn futures::Stream<
                Item = anyhow::Result<(
                    Option<Message>,
                    Option<crate::providers::base::ProviderUsage>,
                )>,
            > + Send
            + 'static,
    >,
>;

/// Sensible default values of Google Cloud Platform (GCP) locations for model deployment.
///
/// Each variant corresponds to a specific GCP region where models can be hosted.
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum GcpLocation {
    /// Represents the us-central1 region in Iowa
    Iowa,
    /// Represents the us-east5 region in Ohio
    Ohio,
    /// Represents the global endpoint (required for Gemini 3 models)
    Global,
}

impl fmt::Display for GcpLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Iowa => write!(f, "us-central1"),
            Self::Ohio => write!(f, "us-east5"),
            Self::Global => write!(f, "global"),
        }
    }
}

/// Represents errors that can occur during model operations.
///
/// This enum encompasses various error conditions that might arise when working
/// with GCP Vertex AI models, including unsupported models, invalid requests,
/// and unsupported locations.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// Error when an unsupported Vertex AI model is specified
    #[error("Unsupported Vertex AI model: {0}")]
    UnsupportedModel(String),
    /// Error when the request structure is invalid
    #[error("Invalid request structure: {0}")]
    InvalidRequest(String),
    /// Error when an unsupported GCP location is specified
    #[error("Unsupported GCP location: {0}")]
    UnsupportedLocation(String),
}

/// Default model for GCP Vertex AI.
pub const DEFAULT_MODEL: &str = "gemini-2.5-flash";

pub const KNOWN_MODELS: &[&str] = &[
    "claude-opus-4-6",
    "claude-sonnet-4-6",
    "claude-opus-4-5@20251101",
    "claude-sonnet-4-5@20250929",
    "claude-opus-4-1@20250805",
    "claude-haiku-4-5@20251001",
    "claude-opus-4@20250514",
    "claude-sonnet-4@20250514",
    "claude-3-5-haiku@20241022",
    "claude-3-haiku@20240307",
    "gemini-3.1-flash-lite",
    "gemini-3.1-pro-preview",
    "gemini-3-pro-preview",
    "gemini-3-flash-preview",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
    "gemini-2.0-flash",
    "gemini-2.0-flash-lite",
];

/// Represents available GCP Vertex AI models for goose.
///
/// This enum encompasses different model families that are supported
/// in the GCP Vertex AI platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GcpVertexAIModel {
    /// Claude model family
    Claude(String),
    /// Gemini model family
    Gemini(String),
    /// MaaS (Model as a Service) models from Model Garden
    /// Contains (publisher, full_model_name)
    MaaS(String, String),
}

impl fmt::Display for GcpVertexAIModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Claude(name) => write!(f, "{name}"),
            Self::Gemini(name) => write!(f, "{name}"),
            Self::MaaS(_, name) => write!(f, "{name}"),
        }
    }
}

impl GcpVertexAIModel {
    /// Returns the default GCP location for the model.
    ///
    /// Location routing by model family:
    /// - Claude models: Ohio (us-east5)
    /// - Gemini 3.x models: Global
    /// - Other Gemini models: Iowa (us-central1)
    /// - MaaS models: Iowa (us-central1)
    pub fn known_location(&self) -> GcpLocation {
        match self {
            Self::Claude(_) => GcpLocation::Ohio,
            Self::Gemini(name) if name.starts_with("gemini-3") => GcpLocation::Global,
            Self::Gemini(_) => GcpLocation::Iowa,
            Self::MaaS(_, _) => GcpLocation::Iowa,
        }
    }
}

impl TryFrom<&str> for GcpVertexAIModel {
    type Error = ModelError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if s.starts_with("claude-") {
            Ok(Self::Claude(s.to_string()))
        } else if s.starts_with("gemini-") {
            Ok(Self::Gemini(s.to_string()))
        } else if s.ends_with("-maas") {
            let publisher = s
                .split('-')
                .next()
                .ok_or_else(|| ModelError::UnsupportedModel(s.to_string()))?
                .to_string();
            Ok(Self::MaaS(publisher, s.to_string()))
        } else {
            Err(ModelError::UnsupportedModel(s.to_string()))
        }
    }
}

/// Holds context information for a model request since the Vertex AI platform
/// supports multiple model families.
///
/// This structure maintains information about the model being used
/// and provides utility methods for handling model-specific operations.
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// The GCP Vertex AI model being used
    pub model: GcpVertexAIModel,
}

impl RequestContext {
    /// Creates a new RequestContext from a model ID string.
    ///
    /// # Arguments
    /// * `model_id` - The string identifier of the model
    ///
    /// # Returns
    /// * `Result<Self>` - A new RequestContext if the model ID is valid
    pub fn new(model_id: &str) -> Result<Self> {
        Ok(Self {
            model: GcpVertexAIModel::try_from(model_id)
                .with_context(|| format!("Failed to parse model ID: {model_id}"))?,
        })
    }

    /// Returns the provider associated with the model.
    pub fn provider(&self) -> ModelProvider {
        match &self.model {
            GcpVertexAIModel::Claude(_) => ModelProvider::Anthropic,
            GcpVertexAIModel::Gemini(_) => ModelProvider::Google,
            GcpVertexAIModel::MaaS(publisher, _) => ModelProvider::MaaS(publisher.clone()),
        }
    }
}

/// Represents available model providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelProvider {
    /// Anthropic provider (Claude models)
    Anthropic,
    /// Google provider (Gemini models)
    Google,
    /// MaaS provider (Model as a Service from Model Garden)
    MaaS(String),
}

impl ModelProvider {
    /// Returns the string representation of the provider.
    pub fn as_str(&self) -> String {
        match self {
            Self::Anthropic => "anthropic".to_string(),
            Self::Google => "google".to_string(),
            Self::MaaS(publisher) => publisher.clone(),
        }
    }
}

/// Creates an Anthropic-specific Vertex AI request payload.
///
/// # Arguments
/// * `model_config` - Configuration for the model
/// * `system` - System prompt
/// * `messages` - Array of messages
/// * `tools` - Array of available tools
///
/// # Returns
/// * `Result<Value>` - JSON request payload for Anthropic API
fn create_anthropic_request(
    model_config: &ModelConfig,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
) -> Result<Value> {
    let mut request = anthropic::create_request(model_config, system, messages, tools)?;

    let obj = request
        .as_object_mut()
        .ok_or_else(|| ModelError::InvalidRequest("Request is not a JSON object".to_string()))?;

    // Note: We don't need to specify the model in the request body
    // The model is determined by the endpoint URL in GCP Vertex AI
    obj.remove("model");
    obj.insert(
        "anthropic_version".to_string(),
        Value::String("vertex-2023-10-16".to_string()),
    );

    Ok(request)
}

/// Creates a Gemini-specific Vertex AI request payload.
///
/// # Arguments
/// * `model_config` - Configuration for the model
/// * `system` - System prompt
/// * `messages` - Array of messages
/// * `tools` - Array of available tools
///
/// # Returns
/// * `Result<Value>` - JSON request payload for Google API
fn create_google_request(
    model_config: &ModelConfig,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
) -> Result<Value> {
    google::create_request(model_config, system, messages, tools)
}

/// Creates a provider-specific request payload and context.
///
/// # Arguments
/// * `model_config` - Configuration for the model
/// * `system` - System prompt
/// * `messages` - Array of messages
/// * `tools` - Array of available tools
///
/// # Returns
/// * `Result<(Value, RequestContext)>` - Tuple of request payload and context
pub fn create_request(
    model_config: &ModelConfig,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
) -> Result<(Value, RequestContext)> {
    let context = RequestContext::new(&model_config.model_name)?;

    let request = match &context.model {
        GcpVertexAIModel::Claude(_) => {
            create_anthropic_request(model_config, system, messages, tools)?
        }
        GcpVertexAIModel::Gemini(_) => {
            create_google_request(model_config, system, messages, tools)?
        }
        GcpVertexAIModel::MaaS(_, _) => {
            // TODO: Branch on publisher for format selection once we know which
            // MaaS providers use which formats (e.g., OpenAI vs Google format)
            // For now, default to Google format since most use generateContent endpoint
            create_google_request(model_config, system, messages, tools)?
        }
    };

    Ok((request, context))
}

/// Converts a provider response to a Message.
///
/// # Arguments
/// * `response` - The raw response from the provider
/// * `request_context` - Context information about the request
///
/// # Returns
/// * `Result<Message>` - Converted message
pub fn response_to_message(response: Value, request_context: RequestContext) -> Result<Message> {
    match request_context.provider() {
        ModelProvider::Anthropic => anthropic::response_to_message(&response),
        ModelProvider::Google => google::response_to_message(response),
        ModelProvider::MaaS(_) => google::response_to_message(response),
    }
}

/// Extracts token usage information from the response data.
///
/// # Arguments
/// * `data` - The response data containing usage information
/// * `request_context` - Context information about the request
///
/// # Returns
/// * `Result<Usage>` - Usage statistics
pub fn get_usage(data: &Value, request_context: &RequestContext) -> Result<Usage> {
    match request_context.provider() {
        ModelProvider::Anthropic => anthropic::get_usage(data),
        ModelProvider::Google => google::get_usage(data),
        ModelProvider::MaaS(_) => google::get_usage(data),
    }
}

pub fn response_to_streaming_message<S>(
    stream: S,
    request_context: &RequestContext,
) -> StreamingMessageStream
where
    S: futures::Stream<Item = anyhow::Result<String>> + Unpin + Send + 'static,
{
    match request_context.provider() {
        ModelProvider::Anthropic => Box::pin(anthropic::response_to_streaming_message(stream)),
        ModelProvider::Google | ModelProvider::MaaS(_) => {
            Box::pin(google::response_to_streaming_message(stream))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_model_parsing() -> Result<()> {
        let claude = GcpVertexAIModel::try_from("claude-sonnet-4@20250514")?;
        assert!(matches!(claude, GcpVertexAIModel::Claude(_)));
        assert_eq!(claude.to_string(), "claude-sonnet-4@20250514");

        let gemini = GcpVertexAIModel::try_from("gemini-2.5-flash")?;
        assert!(matches!(gemini, GcpVertexAIModel::Gemini(_)));
        assert_eq!(gemini.to_string(), "gemini-2.5-flash");

        let maas = GcpVertexAIModel::try_from("qwen-maas")?;
        assert!(matches!(maas, GcpVertexAIModel::MaaS(_, _)));

        assert!(GcpVertexAIModel::try_from("unsupported-model").is_err());
        Ok(())
    }

    #[test]
    fn test_default_locations() -> Result<()> {
        let claude_model = GcpVertexAIModel::try_from("claude-sonnet-4@20250514")?;
        assert_eq!(claude_model.known_location(), GcpLocation::Ohio);

        let gemini_model = GcpVertexAIModel::try_from("gemini-2.5-flash")?;
        assert_eq!(gemini_model.known_location(), GcpLocation::Iowa);

        let gemini_3_model = GcpVertexAIModel::try_from("gemini-3.1-flash-lite")?;
        assert_eq!(gemini_3_model.known_location(), GcpLocation::Global);

        Ok(())
    }

    #[test]
    fn test_unknown_model_parsing() -> Result<()> {
        let model = GcpVertexAIModel::try_from("claude-future-version")?;
        assert!(matches!(model, GcpVertexAIModel::Claude(_)));
        assert_eq!(model.to_string(), "claude-future-version");

        let model = GcpVertexAIModel::try_from("gemini-4.0-ultra")?;
        assert!(matches!(model, GcpVertexAIModel::Gemini(_)));
        assert_eq!(model.to_string(), "gemini-4.0-ultra");

        Ok(())
    }
}
