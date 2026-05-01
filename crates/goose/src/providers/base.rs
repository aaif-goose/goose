use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::Stream;
use serde::{Deserialize, Serialize};

use super::canonical::{map_to_canonical_model, CanonicalModelRegistry};
use super::errors::ProviderError;
use super::inventory::{default_inventory_identity, InventoryIdentityInput};
use super::retry::RetryConfig;
use crate::config::base::ConfigValue;
use crate::config::{Config, ExtensionConfig, GooseMode};
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use crate::model::ModelConfig;
use crate::permission::PermissionConfirmation;
use crate::utils::safe_truncate;
use rmcp::model::Tool;
use utoipa::ToSchema;

use once_cell::sync::Lazy;
use regex::Regex;
use std::ops::{Add, AddAssign};
use std::pin::Pin;
use std::sync::LazyLock;
use std::sync::Mutex;

fn strip_xml_tags(text: &str) -> String {
    static BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)<([a-zA-Z][a-zA-Z0-9_]*)[^>]*>.*?</[a-zA-Z][a-zA-Z0-9_]*>").unwrap()
    });
    static TAG_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"</?[a-zA-Z][a-zA-Z0-9_]*[^>]*>").unwrap());
    let pass1 = BLOCK_RE.replace_all(text, "");
    TAG_RE.replace_all(&pass1, "").into_owned()
}

fn extract_short_title(text: &str) -> String {
    let word_count = text.split_whitespace().count();
    if word_count <= 8 {
        return text.to_string();
    }

    {
        let mut results = Vec::new();
        let mut quote_char: Option<char> = None;
        let mut current = String::new();
        let mut prev_char: Option<char> = None;

        for ch in text.chars() {
            match quote_char {
                None => {
                    if matches!(ch, '"' | '\'' | '`') {
                        let after_alnum = prev_char.map(|p| p.is_alphanumeric()).unwrap_or(false);
                        if !after_alnum {
                            quote_char = Some(ch);
                            current.clear();
                        }
                    }
                }
                Some(q) => {
                    if ch == q {
                        let trimmed = current.trim().to_string();
                        let wc = trimmed.split_whitespace().count();
                        if (2..=8).contains(&wc) {
                            results.push(trimmed);
                        }
                        quote_char = None;
                        current.clear();
                    } else {
                        current.push(ch);
                    }
                }
            }
            prev_char = Some(ch);
        }

        if let Some(title) = results.last() {
            return title.clone();
        }
    }

    if let Some(last) = text.lines().rev().find(|l| !l.trim().is_empty()) {
        return last.trim().to_string();
    }

    text.to_string()
}

/// A global store for the current model being used, we use this as when a provider returns, it tells us the real model, not an alias
pub static CURRENT_MODEL: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

/// Set the current model in the global store
pub fn set_current_model(model: &str) {
    if let Ok(mut current_model) = CURRENT_MODEL.lock() {
        *current_model = Some(model.to_string());
    }
}

/// Get the current model from the global store, the real model, not an alias
pub fn get_current_model() -> Option<String> {
    CURRENT_MODEL.lock().ok().and_then(|model| model.clone())
}

pub static MSG_COUNT_FOR_SESSION_NAME_GENERATION: usize = 3;

/// Information about a model's capabilities
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct ModelInfo {
    /// The name of the model
    pub name: String,
    /// The maximum context length this model supports
    pub context_limit: usize,
    /// Cost per token for input in USD (optional)
    pub input_token_cost: Option<f64>,
    /// Cost per token for output in USD (optional)
    pub output_token_cost: Option<f64>,
    /// Currency for the costs (default: "$")
    pub currency: Option<String>,
    /// Whether this model supports cache control
    pub supports_cache_control: Option<bool>,
}

impl ModelInfo {
    /// Create a new ModelInfo with just name and context limit
    pub fn new(name: impl Into<String>, context_limit: usize) -> Self {
        Self {
            name: name.into(),
            context_limit,
            input_token_cost: None,
            output_token_cost: None,
            currency: None,
            supports_cache_control: None,
        }
    }

    /// Create a new ModelInfo with cost information (per token)
    pub fn with_cost(
        name: impl Into<String>,
        context_limit: usize,
        input_cost: f64,
        output_cost: f64,
    ) -> Self {
        Self {
            name: name.into(),
            context_limit,
            input_token_cost: Some(input_cost),
            output_token_cost: Some(output_cost),
            currency: Some("$".to_string()),
            supports_cache_control: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum ProviderType {
    Preferred,
    Builtin,
    Declarative,
    Custom,
}

/// Metadata about a provider's configuration requirements and capabilities
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderMetadata {
    /// The unique identifier for this provider
    pub name: String,
    /// Display name for the provider in UIs
    pub display_name: String,
    /// Description of the provider's capabilities
    pub description: String,
    /// The default/recommended model for this provider
    pub default_model: String,
    /// A list of currently known models with their capabilities
    pub known_models: Vec<ModelInfo>,
    /// Link to the docs where models can be found
    pub model_doc_link: String,
    /// Required configuration keys
    pub config_keys: Vec<ConfigKey>,
    /// step-by-step instructions for set up providers eg: api key
    #[serde(default)]
    pub setup_steps: Vec<String>,
    /// Hint shown in the model picker when this provider manages its own model selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_selection_hint: Option<String>,
}

impl ProviderMetadata {
    pub fn new(
        name: &str,
        display_name: &str,
        description: &str,
        default_model: &str,
        model_names: Vec<&str>,
        model_doc_link: &str,
        config_keys: Vec<ConfigKey>,
    ) -> Self {
        Self {
            name: name.to_string(),
            display_name: display_name.to_string(),
            description: description.to_string(),
            default_model: default_model.to_string(),
            known_models: model_names
                .iter()
                .map(|&model_name| ModelInfo {
                    name: model_name.to_string(),
                    context_limit: ModelConfig::new_or_fail(model_name)
                        .with_canonical_limits(name)
                        .context_limit(),
                    input_token_cost: None,
                    output_token_cost: None,
                    currency: None,
                    supports_cache_control: None,
                })
                .collect(),
            model_doc_link: model_doc_link.to_string(),
            config_keys,
            setup_steps: vec![],
            model_selection_hint: None,
        }
    }

    pub fn with_models(
        name: &str,
        display_name: &str,
        description: &str,
        default_model: &str,
        models: Vec<ModelInfo>,
        model_doc_link: &str,
        config_keys: Vec<ConfigKey>,
    ) -> Self {
        Self {
            name: name.to_string(),
            display_name: display_name.to_string(),
            description: description.to_string(),
            default_model: default_model.to_string(),
            known_models: models,
            model_doc_link: model_doc_link.to_string(),
            config_keys,
            setup_steps: vec![],
            model_selection_hint: None,
        }
    }

    pub fn empty() -> Self {
        Self {
            name: "".to_string(),
            display_name: "".to_string(),
            description: "".to_string(),
            default_model: "".to_string(),
            known_models: vec![],
            model_doc_link: "".to_string(),
            config_keys: vec![],
            setup_steps: vec![],
            model_selection_hint: None,
        }
    }

    pub fn with_setup_steps(mut self, steps: Vec<&str>) -> Self {
        self.setup_steps = steps.into_iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_model_selection_hint(mut self, hint: &str) -> Self {
        self.model_selection_hint = Some(hint.to_string());
        self
    }
}

/// Configuration key metadata for provider setup
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfigKey {
    /// The name of the configuration key (e.g., "API_KEY")
    pub name: String,
    /// Whether this key is required for the provider to function
    pub required: bool,
    /// Whether this key should be stored securely (e.g., in keychain)
    pub secret: bool,
    /// Optional default value for the key
    pub default: Option<String>,
    /// Whether this key should be configured using an OAuth flow
    /// When true, the provider's configure_oauth() method will be called instead of prompting for manual input
    pub oauth_flow: bool,
    /// Whether this OAuth flow uses the device code grant (RFC 8628)
    /// When true, the user must enter a verification code in the browser
    #[serde(default)]
    pub device_code_flow: bool,
    /// Whether this key should be shown prominently during provider setup
    /// (onboarding, settings modal, CLI configure)
    #[serde(default)]
    pub primary: bool,
}

impl ConfigKey {
    /// Create a new ConfigKey
    pub fn new(
        name: &str,
        required: bool,
        secret: bool,
        default: Option<&str>,
        primary: bool,
    ) -> Self {
        Self {
            name: name.to_string(),
            required,
            secret,
            default: default.map(|s| s.to_string()),
            oauth_flow: false,
            device_code_flow: false,
            primary,
        }
    }

    pub fn from_value_type<T: ConfigValue>(required: bool, secret: bool, primary: bool) -> Self {
        Self {
            name: T::KEY.to_string(),
            required,
            secret,
            default: Some(T::DEFAULT.to_string()),
            oauth_flow: false,
            device_code_flow: false,
            primary,
        }
    }

    /// Create a new ConfigKey that uses an OAuth flow for configuration
    ///
    /// This is used for providers that support OAuth authentication instead of manual API key entry.
    /// When oauth_flow is true, the configuration system will call the provider's configure_oauth() method.
    pub fn new_oauth(
        name: &str,
        required: bool,
        secret: bool,
        default: Option<&str>,
        primary: bool,
    ) -> Self {
        Self {
            name: name.to_string(),
            required,
            secret,
            default: default.map(|s| s.to_string()),
            oauth_flow: true,
            device_code_flow: false,
            primary,
        }
    }

    /// Create a new ConfigKey that uses OAuth device code flow (RFC 8628) for configuration
    ///
    /// Similar to new_oauth, but indicates the provider uses the device code grant where the user
    /// must enter a verification code in the browser.
    pub fn new_oauth_device_code(
        name: &str,
        required: bool,
        secret: bool,
        default: Option<&str>,
        primary: bool,
    ) -> Self {
        Self {
            name: name.to_string(),
            required,
            secret,
            default: default.map(|s| s.to_string()),
            oauth_flow: true,
            device_code_flow: true,
            primary,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub model: String,
    pub usage: Usage,
}

impl ProviderUsage {
    pub fn new(model: String, usage: Usage) -> Self {
        Self { model, usage }
    }

    /// Ensures this ProviderUsage has token counts, estimating them if necessary
    pub async fn ensure_tokens(
        &mut self,
        system_prompt: &str,
        request_messages: &[Message],
        response: &Message,
        tools: &[Tool],
    ) -> Result<(), ProviderError> {
        crate::providers::usage_estimator::ensure_usage_tokens(
            self,
            system_prompt,
            request_messages,
            response,
            tools,
        )
        .await
        .map_err(|e| ProviderError::ExecutionError(format!("Failed to ensure usage tokens: {}", e)))
    }

    /// Combine this ProviderUsage with another, adding their token counts
    /// Uses the model from this ProviderUsage
    pub fn combine_with(&self, other: &ProviderUsage) -> ProviderUsage {
        ProviderUsage {
            model: self.model.clone(),
            usage: self.usage + other.usage,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Copy)]
pub struct Usage {
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub total_tokens: Option<i32>,
    pub cache_read_input_tokens: Option<i32>,
    pub cache_write_input_tokens: Option<i32>,
}

fn sum_optionals<T>(a: Option<T>, b: Option<T>) -> Option<T>
where
    T: Add<Output = T> + Default,
{
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        (Some(x), None) => Some(x + T::default()),
        (None, Some(y)) => Some(T::default() + y),
        (None, None) => None,
    }
}

impl Add for Usage {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(
            sum_optionals(self.input_tokens, other.input_tokens),
            sum_optionals(self.output_tokens, other.output_tokens),
            sum_optionals(self.total_tokens, other.total_tokens),
        )
        .with_cache_tokens(
            sum_optionals(self.cache_read_input_tokens, other.cache_read_input_tokens),
            sum_optionals(
                self.cache_write_input_tokens,
                other.cache_write_input_tokens,
            ),
        )
    }
}

impl AddAssign for Usage {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Usage {
    pub fn new(
        input_tokens: Option<i32>,
        output_tokens: Option<i32>,
        total_tokens: Option<i32>,
    ) -> Self {
        let calculated_total = if total_tokens.is_none() {
            match (input_tokens, output_tokens) {
                (Some(input), Some(output)) => Some(input + output),
                (Some(input), None) => Some(input),
                (None, Some(output)) => Some(output),
                (None, None) => None,
            }
        } else {
            total_tokens
        };

        Self {
            input_tokens,
            output_tokens,
            total_tokens: calculated_total,
            cache_read_input_tokens: None,
            cache_write_input_tokens: None,
        }
    }

    pub fn with_cache_tokens(
        mut self,
        cache_read_input_tokens: Option<i32>,
        cache_write_input_tokens: Option<i32>,
    ) -> Self {
        self.cache_read_input_tokens = cache_read_input_tokens;
        self.cache_write_input_tokens = cache_write_input_tokens;
        self
    }
}

pub trait ProviderDef: Send + Sync {
    type Provider: Provider + 'static;

    fn metadata() -> ProviderMetadata
    where
        Self: Sized;

    fn from_env(
        model: ModelConfig,
        extensions: Vec<ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>>
    where
        Self: Sized;

    fn supports_inventory_refresh() -> bool
    where
        Self: Sized,
    {
        false
    }

    fn inventory_identity() -> Result<InventoryIdentityInput>
    where
        Self: Sized,
    {
        let metadata = Self::metadata();
        Ok(default_inventory_identity(
            &metadata.name,
            &metadata.name,
            &metadata.config_keys,
            Config::global(),
        ))
    }

    fn inventory_configured() -> bool
    where
        Self: Sized,
    {
        let metadata = Self::metadata();
        super::inventory::default_inventory_configured(&metadata.config_keys, Config::global())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionRouting {
    ActionRequired,
    Noop,
}

/// Base trait for AI providers (OpenAI, Anthropic, etc)
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get the name of this provider instance
    fn get_name(&self) -> &str;

    /// Primary streaming method that all providers must implement.
    ///
    /// Note: Do not add `#[instrument]` here — the call sites (`complete` and
    /// `stream_response_from_provider`) create the telemetry span so that
    /// `session.id` is set once rather than in every provider.
    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError>;

    /// Complete with a specific model config.
    #[tracing::instrument(
        skip(self, model_config, session_id, system, messages, tools),
        fields(session.id = %session_id, gen_ai.request.model = %model_config.model_name)
    )]
    async fn complete(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        let stream = self
            .stream(model_config, session_id, system, messages, tools)
            .await?;
        collect_stream(stream).await
    }

    /// Try fast model first, fall back to regular model on failure.
    async fn complete_fast(
        &self,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        let model_config = self.get_model_config();
        let fast_config = model_config.use_fast_model();

        let result = self
            .complete(&fast_config, session_id, system, messages, tools)
            .await;

        match result {
            Ok(response) => Ok(response),
            Err(e) => {
                if fast_config.model_name != model_config.model_name {
                    tracing::warn!(
                        "Fast model {} failed with error: {}. Falling back to regular model {}",
                        fast_config.model_name,
                        e,
                        model_config.model_name
                    );
                    self.complete(&model_config, session_id, system, messages, tools)
                        .await
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Get the model config from the provider
    fn get_model_config(&self) -> ModelConfig;

    fn retry_config(&self) -> RetryConfig {
        RetryConfig::default()
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![])
    }

    fn skip_canonical_filtering(&self) -> bool {
        false
    }

    /// Fetch inventory models filtered by canonical registry and usability.
    async fn fetch_recommended_models(&self) -> Result<Vec<String>, ProviderError> {
        let all_models = self.fetch_supported_models().await?;

        if self.skip_canonical_filtering() {
            return Ok(all_models);
        }

        let registry = CanonicalModelRegistry::bundled().map_err(|e| {
            ProviderError::ExecutionError(format!("Failed to load canonical registry: {}", e))
        })?;

        let provider_name = self.get_name();

        // Get all text-capable models with their release dates
        let mut models_with_dates: Vec<(String, Option<String>)> = all_models
            .iter()
            .filter_map(|model| {
                let canonical_id = map_to_canonical_model(provider_name, model, registry)?;

                let (provider, model_name) = canonical_id.split_once('/')?;
                let canonical_model = registry.get(provider, model_name)?;

                if !canonical_model
                    .modalities
                    .input
                    .contains(&crate::providers::canonical::Modality::Text)
                {
                    return None;
                }

                if !canonical_model.tool_call && !self.get_model_config().toolshim {
                    return None;
                }

                let release_date = canonical_model.release_date.clone();

                Some((model.clone(), release_date))
            })
            .collect();

        // Sort by release date (most recent first), then alphabetically for models without dates
        models_with_dates.sort_by(|a, b| match (&a.1, &b.1) {
            (Some(date_a), Some(date_b)) => date_b.cmp(date_a),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.0.cmp(&b.0),
        });

        let inventory_models: Vec<String> = models_with_dates
            .into_iter()
            .map(|(name, _)| name)
            .collect();

        if inventory_models.is_empty() {
            Ok(all_models)
        } else {
            Ok(inventory_models)
        }
    }

    async fn map_to_canonical_model(
        &self,
        provider_model: &str,
    ) -> Result<Option<String>, ProviderError> {
        let registry = CanonicalModelRegistry::bundled().map_err(|e| {
            ProviderError::ExecutionError(format!("Failed to load canonical registry: {}", e))
        })?;

        Ok(map_to_canonical_model(
            self.get_name(),
            provider_model,
            registry,
        ))
    }

    fn supports_embeddings(&self) -> bool {
        false
    }

    /// Whether the provider manages its own conversation context (e.g. CLI
    /// wrappers like Claude Code or Gemini CLI). When true, goose-side
    /// context management such as tool-pair summarization is skipped because
    /// the provider's internal state is the source of truth.
    fn manages_own_context(&self) -> bool {
        false
    }

    async fn supports_cache_control(&self) -> bool {
        false
    }

    /// Create embeddings if supported. Default implementation returns an error.
    async fn create_embeddings(
        &self,
        _session_id: &str,
        _texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, ProviderError> {
        Err(ProviderError::ExecutionError(
            "This provider does not support embeddings".to_string(),
        ))
    }

    /// Returns the first 3 user messages as strings for session naming,
    /// filtering out assistant-only content (e.g. preprompt blocks).
    fn get_initial_user_messages(&self, messages: &Conversation) -> Vec<String> {
        messages
            .iter()
            .filter(|m| m.role == rmcp::model::Role::User)
            .take(MSG_COUNT_FOR_SESSION_NAME_GENERATION)
            .map(|m| {
                m.content
                    .iter()
                    .filter_map(|c| c.filter_for_audience(rmcp::model::Role::User))
                    .filter_map(|c| c.as_text().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect()
    }

    /// Extracts preprompt context (assistant-audience blocks) from the first user message.
    /// These are content blocks visible to the assistant but not the user.
    fn get_preprompt_context(&self, messages: &Conversation) -> String {
        messages
            .iter()
            .filter(|m| m.role == rmcp::model::Role::User)
            .take(1)
            .flat_map(|m| m.content.iter())
            .filter_map(|c| {
                // If this block is NOT visible to the user, it's preprompt/assistant-only content
                if c.filter_for_audience(rmcp::model::Role::User).is_none() {
                    c.as_text().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Generate a session name/description based on the conversation history
    /// Creates a prompt asking for a concise description in 4 words or less.
    async fn generate_session_name(
        &self,
        session_id: &str,
        messages: &Conversation,
    ) -> Result<String, ProviderError> {
        let context = self.get_initial_user_messages(messages);
        let preprompt_context = self.get_preprompt_context(messages);
        let system = crate::prompt_template::render_template(
            "session_name.md",
            &std::collections::HashMap::<String, String>::new(),
        )
        .map_err(|e| ProviderError::ContextLengthExceeded(e.to_string()))?;

        use super::cli_common::{
            SESSION_NAME_BEGIN_MARKER, SESSION_NAME_END_MARKER, SESSION_NAME_SUFFIX,
        };

        let preprompt_section = if preprompt_context.is_empty() {
            String::new()
        } else {
            format!(
                "---BEGIN BACKGROUND CONTEXT (for understanding only, do NOT base the title on this)---\n{}\n---END BACKGROUND CONTEXT---\n\n",
                preprompt_context
            )
        };

        let user_text = format!(
            "{}{}\n{}\n{}\n\n{}",
            preprompt_section,
            SESSION_NAME_BEGIN_MARKER,
            context.join("\n"),
            SESSION_NAME_END_MARKER,
            SESSION_NAME_SUFFIX,
        );
        let message = Message::user().with_text(&user_text);
        let result = self
            .complete_fast(session_id, &system, &[message], &[])
            .await?;

        let raw: String = result
            .0
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .collect();
        let description = strip_xml_tags(&raw)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        Ok(safe_truncate(&extract_short_title(&description), 100))
    }

    /// Configure OAuth authentication for this provider
    ///
    /// This method is called when a provider has configuration keys marked with oauth_flow = true.
    /// Providers that support OAuth should override this method to implement their specific OAuth flow.
    ///
    /// # Returns
    /// * `Ok(())` if OAuth configuration succeeds and credentials are saved
    /// * `Err(ProviderError)` if OAuth fails or is not supported by this provider
    ///
    /// # Default Implementation
    /// The default implementation returns an error indicating OAuth is not supported.
    async fn configure_oauth(&self) -> Result<(), ProviderError> {
        Err(ProviderError::ExecutionError(
            "OAuth configuration not supported by this provider".to_string(),
        ))
    }

    async fn refresh_credentials(&self) -> Result<(), ProviderError> {
        Err(ProviderError::NotImplemented(
            "credential refresh not supported by this provider".to_string(),
        ))
    }

    async fn update_mode(&self, _session_id: &str, _mode: GooseMode) -> Result<(), ProviderError> {
        Ok(())
    }

    fn permission_routing(&self) -> PermissionRouting {
        PermissionRouting::Noop
    }

    async fn handle_permission_confirmation(
        &self,
        _request_id: &str,
        _confirmation: &PermissionConfirmation,
    ) -> bool {
        false
    }
}

/// A message stream yields partial text content but complete tool calls, all within the Message object
/// So a message with text will contain potentially just a word of a longer response, but tool calls
/// messages will only be yielded once concatenated.
pub type MessageStream = Pin<
    Box<dyn Stream<Item = Result<(Option<Message>, Option<ProviderUsage>), ProviderError>> + Send>,
>;

pub fn stream_from_single_message(message: Message, usage: ProviderUsage) -> MessageStream {
    let stream = futures::stream::once(async move { Ok((Some(message), Some(usage))) });
    Box::pin(stream)
}

/// Collect all chunks from a MessageStream into a single Message and ProviderUsage
pub async fn collect_stream(
    mut stream: MessageStream,
) -> Result<(Message, ProviderUsage), ProviderError> {
    use futures::StreamExt;

    let mut final_message: Option<Message> = None;
    let mut final_usage: Option<ProviderUsage> = None;

    while let Some(result) = stream.next().await {
        let (msg_opt, usage_opt) = result?;

        if let Some(msg) = msg_opt {
            final_message = Some(match final_message {
                Some(mut prev) => {
                    for new_content in msg.content {
                        match (&mut prev.content.last_mut(), &new_content) {
                            // Coalesce consecutive text blocks
                            (
                                Some(MessageContent::Text(last_text)),
                                MessageContent::Text(new_text),
                            ) => {
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
    }

    match final_message {
        Some(msg) => {
            let usage = final_usage
                .unwrap_or_else(|| ProviderUsage::new("unknown".to_string(), Usage::default()));
            Ok((msg, usage))
        }
        None => Err(ProviderError::ExecutionError(
            "Stream yielded no message".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use test_case::test_case;

    use serde_json::json;

    #[test]
    fn test_strip_xml_tags() {
        assert_eq!(strip_xml_tags("<think>reasoning</think>answer"), "answer");
        assert_eq!(strip_xml_tags("before<t>mid</t>after"), "beforeafter");
        assert_eq!(strip_xml_tags("<a>x</a><b>y</b>z"), "z");
        assert_eq!(strip_xml_tags("no tags here"), "no tags here");
        assert_eq!(strip_xml_tags("a < b > c"), "a < b > c");
        assert_eq!(strip_xml_tags("<think>über</think>ok"), "ok");
        assert_eq!(strip_xml_tags("<think>日本語</think>hello"), "hello");
        assert_eq!(strip_xml_tags(""), "");
        assert_eq!(strip_xml_tags("<>stuff</>"), "<>stuff</>");
        // attributes
        assert_eq!(
            strip_xml_tags(r#"<think class="deep">reasoning</think>answer"#),
            "answer"
        );
        // self-closing tags
        assert_eq!(strip_xml_tags("<br/>self closing"), "self closing");
        // orphan closing tags
        assert_eq!(strip_xml_tags("orphan </think> tag"), "orphan  tag");
        // multiline content
        assert_eq!(
            strip_xml_tags("<think>\nline1\nline2\n</think>result"),
            "result"
        );
    }

    #[test]
    fn test_extract_short_title() {
        assert_eq!(extract_short_title("List files"), "List files");
        assert_eq!(
            extract_short_title(
                r#"blah blah blah blah blah blah blah blah blah "List files in folder""#
            ),
            "List files in folder"
        );
        assert_eq!(
            extract_short_title(
                "blah blah blah blah blah blah blah blah blah `View current files`"
            ),
            "View current files"
        );
        assert_eq!(
            extract_short_title(
                r#"stuff stuff stuff stuff stuff stuff stuff stuff "Abc title" "Zzz title""#
            ),
            "Zzz title"
        );
        assert_eq!(
            extract_short_title(
                "long long long long long long long long long\nList files in folder"
            ),
            "List files in folder"
        );
        assert_eq!(
            extract_short_title(
                r#"lots of words here and there and more and more "single" final line here"#
            ),
            "lots of words here and there and more and more \"single\" final line here"
        );
        assert_eq!(extract_short_title("Hello world"), "Hello world");
        assert_eq!(
            extract_short_title(
                r#"1. Analyze the request. 2. The user's message says list files. 3. "List current folder files" fits perfectly. Result: List current folder files"#
            ),
            "List current folder files"
        );
        assert_eq!(
            extract_short_title(
                r#"the user's phrasing is about listing files and the user's intent is clear. "List folder files" is best"#
            ),
            "List folder files"
        );
        assert_eq!(
            extract_short_title(
                "lots of reasoning here about what to call it\nList current folder files"
            ),
            "List current folder files"
        );
    }

    #[test]
    fn test_usage_creation() {
        let usage = Usage::new(Some(10), Some(20), Some(30));
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.output_tokens, Some(20));
        assert_eq!(usage.total_tokens, Some(30));
    }

    fn content_from_str(s: String) -> MessageContent {
        if let Some(img_data) = s.strip_prefix("*img:") {
            MessageContent::image(format!("http://example.com/{}", img_data), "image/png")
        } else if let Some(tool_name) = s.strip_prefix("*tool:") {
            let tool_call = Ok(
                rmcp::model::CallToolRequestParams::new(tool_name.to_string())
                    .with_arguments(serde_json::Map::new()),
            );
            MessageContent::tool_request(format!("tool_{}", tool_name), tool_call)
        } else {
            MessageContent::text(s)
        }
    }

    fn create_test_stream(
        items: Vec<String>,
    ) -> impl Stream<Item = Result<(Option<Message>, Option<ProviderUsage>), ProviderError>> {
        use futures::stream;
        stream::iter(items.into_iter().map(|item| {
            let content = content_from_str(item);
            let message = Message::new(
                rmcp::model::Role::Assistant,
                chrono::Utc::now().timestamp(),
                vec![content],
            );
            Ok((Some(message), None))
        }))
    }

    fn content_to_strings(msg: &Message) -> Vec<String> {
        msg.content
            .iter()
            .map(|c| match c {
                MessageContent::Text(t) => t.text.clone(),
                MessageContent::Image(_) => "*img".to_string(),
                MessageContent::ToolRequest(tr) => {
                    if let Ok(call) = &tr.tool_call {
                        format!("*tool:{}", call.name)
                    } else {
                        "*tool:error".to_string()
                    }
                }
                _ => "*other".to_string(),
            })
            .collect()
    }

    #[test_case(
        vec!["Hello", " ", "world"],
        vec!["Hello world"]
        ; "consecutive text coalesces"
    )]
    #[test_case(
        vec!["Hello", "*img:pic1", "world"],
        vec!["Hello", "*img", "world"]
        ; "non-text breaks coalescing"
    )]
    #[test_case(
        vec!["A", "B", "*img:pic1", "C", "D", "*tool:read", "E", "F"],
        vec!["AB", "*img", "CD", "*tool:read", "EF"]
        ; "multiple text groups"
    )]
    #[test_case(
        vec!["Text1", "*img:pic", "Text2"],
        vec!["Text1", "*img", "Text2"]
        ; "mixed content in chunk"
    )]
    #[tokio::test]
    async fn test_collect_stream_coalescing(input_items: Vec<&str>, expected: Vec<&str>) {
        let items: Vec<String> = input_items.into_iter().map(|s| s.to_string()).collect();
        let stream = create_test_stream(items);
        let (msg, _) = collect_stream(Box::pin(stream)).await.unwrap();
        assert_eq!(content_to_strings(&msg), expected);
    }

    #[tokio::test]
    async fn test_collect_stream_defaults_usage() {
        let stream = create_test_stream(vec!["Hello".to_string()]);
        let (msg, usage) = collect_stream(Box::pin(stream)).await.unwrap();
        assert_eq!(content_to_strings(&msg), vec!["Hello"]);
        assert_eq!(usage.model, "unknown");
    }

    #[test]
    fn test_usage_serialization() -> Result<()> {
        let usage = Usage::new(Some(10), Some(20), Some(30));
        let serialized = serde_json::to_string(&usage)?;
        let deserialized: Usage = serde_json::from_str(&serialized)?;

        assert_eq!(usage.input_tokens, deserialized.input_tokens);
        assert_eq!(usage.output_tokens, deserialized.output_tokens);
        assert_eq!(usage.total_tokens, deserialized.total_tokens);

        // Test JSON structure
        let json_value: serde_json::Value = serde_json::from_str(&serialized)?;
        assert_eq!(json_value["input_tokens"], json!(10));
        assert_eq!(json_value["output_tokens"], json!(20));
        assert_eq!(json_value["total_tokens"], json!(30));

        Ok(())
    }

    #[test]
    fn test_set_and_get_current_model() {
        // Set the model
        set_current_model("gpt-4o");

        // Get the model and verify
        let model = get_current_model();
        assert_eq!(model, Some("gpt-4o".to_string()));

        // Change the model
        set_current_model("claude-sonnet-4-20250514");

        // Get the updated model and verify
        let model = get_current_model();
        assert_eq!(model, Some("claude-sonnet-4-20250514".to_string()));
    }

    #[test]
    fn test_provider_metadata_context_limits() {
        // Test that ProviderMetadata::new correctly sets context limits
        let test_models = vec!["gpt-4o", "claude-sonnet-4-20250514", "unknown-model"];
        let metadata = ProviderMetadata::new(
            "test",
            "Test Provider",
            "Test Description",
            "gpt-4o",
            test_models,
            "https://example.com",
            vec![],
        );

        let model_info: HashMap<String, usize> = metadata
            .known_models
            .into_iter()
            .map(|m| (m.name, m.context_limit))
            .collect();

        // gpt-4o should have 128k limit
        assert_eq!(*model_info.get("gpt-4o").unwrap(), 128_000);

        // claude-sonnet-4-20250514 should have 200k limit
        assert_eq!(
            *model_info.get("claude-sonnet-4-20250514").unwrap(),
            200_000
        );

        // unknown model should have default limit (128k)
        assert_eq!(*model_info.get("unknown-model").unwrap(), 128_000);
    }

    #[test]
    fn test_model_info_creation() {
        // Test direct ModelInfo creation
        let info = ModelInfo {
            name: "test-model".to_string(),
            context_limit: 1000,
            input_token_cost: None,
            output_token_cost: None,
            currency: None,
            supports_cache_control: None,
        };
        assert_eq!(info.context_limit, 1000);

        // Test equality
        let info2 = ModelInfo {
            name: "test-model".to_string(),
            context_limit: 1000,
            input_token_cost: None,
            output_token_cost: None,
            currency: None,
            supports_cache_control: None,
        };
        assert_eq!(info, info2);

        // Test inequality
        let info3 = ModelInfo {
            name: "test-model".to_string(),
            context_limit: 2000,
            input_token_cost: None,
            output_token_cost: None,
            currency: None,
            supports_cache_control: None,
        };
        assert_ne!(info, info3);
    }

    #[test]
    fn test_model_info_with_cost() {
        let info = ModelInfo::with_cost("gpt-4o", 128000, 0.0000025, 0.00001);
        assert_eq!(info.name, "gpt-4o");
        assert_eq!(info.context_limit, 128000);
        assert_eq!(info.input_token_cost, Some(0.0000025));
        assert_eq!(info.output_token_cost, Some(0.00001));
        assert_eq!(info.currency, Some("$".to_string()));
    }

    #[test]
    fn test_usage_addition_includes_cached_tokens() {
        let usage_a =
            Usage::new(Some(100), Some(20), Some(120)).with_cache_tokens(Some(10), Some(5));
        let usage_b = Usage::new(Some(50), Some(8), Some(58)).with_cache_tokens(Some(4), Some(1));

        let combined = usage_a + usage_b;

        assert_eq!(combined.input_tokens, Some(150));
        assert_eq!(combined.output_tokens, Some(28));
        assert_eq!(combined.total_tokens, Some(178));
        assert_eq!(combined.cache_read_input_tokens, Some(14));
        assert_eq!(combined.cache_write_input_tokens, Some(6));
    }
}
