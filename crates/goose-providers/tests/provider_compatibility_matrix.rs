//! Live smoke tests and request recordings for HTTP conversation providers.
//! Effort is passed through ModelConfig, not application environment wiring.
//!
//! Run explicitly; credentials in the environment enable billable requests:
//! `cargo test -p goose-providers --features rustls-tls --test provider_compatibility_matrix -- --ignored --nocapture`
//!
//! Selection uses exact, comma-separated values (unset means all):
//! - GOOSE_PROVIDER_MATRIX_PROVIDERS=openai,anthropic
//! - GOOSE_PROVIDER_MATRIX_MODELS=gpt-6-luna
//! - GOOSE_PROVIDER_MATRIX_SCENARIOS=text,tool_call,tool_continuation
//! - GOOSE_PROVIDER_MATRIX_LIST=1 lists selected rows without sending/recording.
//! - GOOSE_PROVIDER_MATRIX_RECORD_ONLY=1 never sends, even with credentials.
//! - REPLAY_VERIFIED_LIVE=1 bypasses verified snapshots and attempts live requests.
//!   Missing credentials still record only; record-only mode always wins.
//!
//! Unknown filters and empty selections fail rather than silently passing.
//! Successful live rows produce committed verified request snapshots under
//! tests/snapshots/provider_compatibility/<provider>/<hex-model>/<scenario>.json.
//! Verified rows skip request generation and live execution on later runs, even
//! without credentials. Delete a row's snapshot to invalidate it individually.
//! No payload/code comparison is performed: changes require explicit invalidation.
//! GOOSE_PROVIDER_MATRIX_SNAPSHOT_DIR overrides the snapshot directory for testing.
//! Record-only mode and failed replays never overwrite a previous verified result.
//!
//! Scenarios: text, thinking_off, thinking_low, thinking_medium, tool_call,
//! tool_continuation, multi_turn, structured_output, vision, non_streaming.
//! Tool continuation sends two requests and replays the real assistant message;
//! recording-only mode uses explicitly labeled synthetic history. Tool selection
//! is required where forwarding is exposed, otherwise prompt-directed.
//! Unsupported scenarios are reported, not counted as passes. A missing catalog
//! tool capability is unknown, so tool scenarios still attempt the request.
//!
//! PROVIDER_MODELS lists explicit default pairs; multiple models per provider
//! may be listed. Override with GOOSE_PROVIDER_MATRIX_<PROVIDER>_MODEL (uppercase,
//! replacing '-' with '_'). Local servers need explicit host/model configuration.
//! Artifacts: target/provider-compatibility-matrix/<provider>/<hex-model>/<scenario>.json,
//! plus summary.json for the selected run. Model IDs are hex encoded to avoid
//! path collisions. GOOSE_PROVIDER_MATRIX_OUTPUT_DIR overrides the directory.
//! Records include each request, wire controls, parsed response/usage, latency,
//! and instruction compliance. Simple greeting obedience is diagnostic; tool,
//! history, schema, and vision fixture checks are assertions.
//!
//! This excludes decision, voice/session, and on-device APIs. Discovery and
//! retries are blocked; each turn makes at most one inference attempt.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    convert::Infallible,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, ensure, Result};
use futures::StreamExt;
use goose_providers::{
    anthropic::{AnthropicFormatOptions, AnthropicProviderBuilder, ANTHROPIC_API_VERSION},
    api_client::{ApiClient, AuthMethod, RequestBuilderDecorator},
    azure_foundry::{endpoint_kind, AzureFoundryProvider, EndpointKind},
    base::{collect_stream, Provider},
    canonical::{maybe_get_canonical_model, CanonicalModel, Modality},
    conversation::{message::Message, token_usage::ProviderUsage},
    databricks::DatabricksProvider,
    databricks_auth::DatabricksAuth,
    databricks_v2::DatabricksV2Provider,
    declarative::{self, DeclarativeProviderConfig, KeyResolver, ProviderEngine},
    errors::ProviderError,
    google::GoogleProvider,
    model::ModelConfig,
    ollama::OllamaProviderBuilder,
    openai::{self, OpenAiProviderBuilder},
    openai_compatible::OpenAiCompatibleProvider,
    openrouter::OpenRouterProvider,
    request_log::{install_logger, RequestLogHandle, RequestLogger},
    retry::RetryConfig,
    snowflake::SnowflakeProvider,
    thinking::ThinkingEffort,
};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Role, Tool};
use serde_json::{json, Value};

const NOT_SENT: &str = "provider matrix: request recorded but not sent";
const DUMMY_KEY: &str = "provider-matrix-recording-only";
const PROMPT: &str = "Reply with the single word hello.";
const SYSTEM: &str = "You are a helpful assistant.";

// Provider names match native factory names or bundled definition file stems.
// Local model IDs are recording placeholders unless explicitly overridden.
const PROVIDER_MODELS: &[(&str, &str)] = &[
    ("aimlapi", "openai/gpt-5-5"),
    ("alibaba", "qwen3.7-max"),
    ("anthropic", "claude-haiku-5-5"),
    ("atomic_chat", "qwen3"),
    ("azure_foundry", "gpt-6-luna"),
    ("celeris", "celeris-1"),
    ("cerebras", "gpt-oss-120b"),
    ("databricks", "databricks-gpt-6-luna"),
    ("databricks_v2", "databricks-gpt-6-luna"),
    ("deepseek", "deepseek-reasoner"),
    ("empiriolabs", "qwen3-7-plus"),
    ("eurouter", "gpt-5-mini"),
    ("fireworks", "accounts/fireworks/models/gpt-oss-20b"),
    ("friendli", "zai-org/GLM-5.2"),
    ("futurmix", "claude-sonnet-4-20250514"),
    ("google", "gemini-3.6-flash"),
    ("groq", "openai/gpt-oss-20b"),
    ("iflytek", "4.0Ultra"),
    ("iflytek_astron", "xsparkx2flash"),
    ("inception", "mercury-coder"),
    ("llama_swap", "qwen3"),
    ("lmstudio", "qwen3"),
    ("lynkr", "qwen3"),
    ("meta", "muse-spark-1.1"),
    ("minimax", "MiniMax-M2.7"),
    ("mistral", "magistral-medium-2509"),
    ("moonshot", "kimi-k2-thinking-turbo"),
    ("nearai", "zai-org/GLM-5.2"),
    ("novita", "moonshotai/kimi-k2.5"),
    ("nvidia", "z-ai/glm-4.7"),
    ("ollama", "qwen3"),
    ("ollama_cloud", "gpt-oss:20b"),
    ("omlx", "qwen3"),
    ("opencode_go", "kimi-k2.6"),
    ("opencode_zen", "kimi-k3"),
    ("openai", "gpt-6-luna"),
    ("openai_compatible", "gpt-6-luna"),
    ("openrouter", "anthropic/claude-sonnet-4.6"),
    ("opper", "anthropic/claude-sonnet-5"),
    ("orcarouter", "anthropic/claude-sonnet-4.6"),
    ("ovhcloud", "gpt-oss-20b"),
    ("perplexity", "sonar-reasoning"),
    ("pleumrouter", "deepseek-v4-pro"),
    ("routstr", "claude-opus-4.7"),
    ("sakana", "fugu"),
    ("saladcloud", "qwen3.6-35b-a3b"),
    ("saygm", "gpt-6-luna"),
    ("scaleway", "openai/gpt-oss-120b"),
    ("snowflake", "claude-sonnet-4-5"),
    ("tanzu", "openai/gpt-oss-120b"),
    ("tensorix", "z-ai/glm-5"),
    ("together", "openai/gpt-oss-120b"),
    ("trustedrouter", "openai/gpt-6-luna"),
    ("venice", "llama-3.3-70b"),
    ("vercel_ai_gateway", "openai/gpt-5-mini"),
    ("zai", "glm-5.3-flash"),
    ("zai_coding_plan", "glm-5.3-flash"),
    ("zhipu", "glm-4.5"),
];

type LogError = Box<dyn std::error::Error + Send + Sync>;
type CurrentRecording = Arc<Mutex<Option<Arc<Mutex<Recording>>>>>;

struct Recording {
    path: PathBuf,
    document: Value,
    inference_attempts: usize,
}

impl Recording {
    fn turn_mut(&mut self) -> Result<&mut Value> {
        self.document["turns"]
            .as_array_mut()
            .and_then(|turns| turns.last_mut())
            .ok_or_else(|| anyhow!("no active request turn"))
    }

    fn save(&self) -> Result<()> {
        fs::write(&self.path, serde_json::to_vec_pretty(&self.document)?)?;
        Ok(())
    }
}

// Providers log the original failure before retrying. Capture that diagnostic
// because the single-attempt guard otherwise replaces it with "retry blocked".
struct MatrixSubscriber(CurrentRecording);

struct RetryMessage(String);

impl tracing::field::Visit for RetryMessage {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

impl tracing::Subscriber for MatrixSubscriber {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.target() == "goose_provider_types::retry"
            && *metadata.level() == tracing::Level::WARN
    }

    fn new_span(&self, _attributes: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        tracing::span::Id::from_u64(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut message = RetryMessage(String::new());
        event.record(&mut message);
        if !message.0.contains("retrying") {
            return;
        }
        if let Some(recording) = self.0.lock().unwrap().as_ref() {
            let mut recording = recording.lock().unwrap();
            if recording.document.get("first_provider_error").is_none() {
                recording.document["first_provider_error"] = json!(redact_text(&message.0));
            }
        }
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}

struct MatrixLogger(CurrentRecording);
struct MatrixLogHandle(Arc<Mutex<Recording>>);

impl RequestLogger for MatrixLogger {
    fn start(&self) -> std::result::Result<Box<dyn RequestLogHandle>, LogError> {
        let recording = self
            .0
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| anyhow!("no active matrix row"))?
            .clone();
        Ok(Box::new(MatrixLogHandle(recording)))
    }
}

impl RequestLogHandle for MatrixLogHandle {
    fn write(&mut self, line: &str) -> std::result::Result<(), LogError> {
        let entry: Value = serde_json::from_str(line)?;
        if let Some(error) = entry["error"].as_str() {
            let mut recording = self.0.lock().unwrap();
            let turn = recording.turn_mut()?;
            if turn.get("provider_error").is_none() {
                turn["provider_error"] = json!(redact_text(error));
            }
            recording.save()?;
        }
        if let Some(payload) = entry.get("input") {
            let mut recording = self.0.lock().unwrap();
            // A provider fallback must not replace the payload of the first attempt.
            let turn = recording.turn_mut()?;
            if turn.get("payload").is_some() {
                return Err(
                    anyhow!("provider attempted to generate a second inference request").into(),
                );
            }
            turn["model_config"] = entry["model_config"].clone();
            turn["payload"] = payload.clone();
            turn["wire_controls"] = wire_controls(payload);
            recording.save()?;
        }
        Ok(())
    }
}

fn env_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn env_or(name: &str, fallback: &str) -> String {
    env_value(name).unwrap_or_else(|| fallback.to_string())
}

fn model_override_key(provider: &str) -> String {
    format!(
        "GOOSE_PROVIDER_MATRIX_{}_MODEL",
        provider.to_uppercase().replace('-', "_")
    )
}

struct MatrixKeyResolver {
    requires_auth: bool,
}

impl KeyResolver for MatrixKeyResolver {
    type Error = Infallible;

    fn resolve_key(&self, key: &str) -> std::result::Result<String, Self::Error> {
        Ok(env_or(key, if self.requires_auth { DUMMY_KEY } else { "" }))
    }
}

struct Case {
    name: String,
    model: String,
    definition: Option<DeclarativeProviderConfig>,
    missing: Vec<String>,
}

impl Case {
    fn native(name: &str, model: &str, required: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            model: env_or(&model_override_key(name), model),
            definition: None,
            missing: required
                .iter()
                .filter(|key| env_value(key).is_none())
                .map(|key| key.to_string())
                .collect(),
        }
    }

    fn declarative(name: &str, default_model: &str, definition: &str) -> Result<Self> {
        let mut config = declarative::deserialize_provider_config(definition)?;
        let override_key = model_override_key(name);
        let mut missing = Vec::new();
        let model = env_or(&override_key, default_model);
        if !config.api_key_env.is_empty() && env_value(&config.api_key_env).is_none() {
            missing.push(config.api_key_env.clone());
        }
        if let Some(vars) = &config.env_vars {
            for var in vars {
                let placeholder = format!("${{{}}}", var.name);
                if !config.base_url.contains(&placeholder) {
                    if var.name.ends_with("_STREAMING") {
                        config.supports_streaming = env_value(&var.name)
                            .or_else(|| var.default.clone())
                            .map(|value| value.eq_ignore_ascii_case("true"));
                    }
                    continue;
                }
                let value = env_value(&var.name).or_else(|| var.default.clone());
                if value.is_none() {
                    missing.push(var.name.clone());
                }
                if !config.requires_auth && env_value(&var.name).is_none() {
                    missing.push(var.name.clone());
                }
                config.base_url = config.base_url.replace(
                    &placeholder,
                    value
                        .as_deref()
                        .unwrap_or("https://provider-matrix.invalid"),
                );
            }
        }
        if !config.requires_auth && env_value(&override_key).is_none() {
            missing.push(override_key);
        }
        // An optional key must not prevent an explicitly configured local server
        // from being exercised. No required key is synthesized for live requests.
        if !config.requires_auth {
            missing.retain(|key| key != &config.api_key_env);
        }
        missing.sort();
        missing.dedup();
        Ok(Self {
            name: name.to_string(),
            model,
            definition: Some(config),
            missing,
        })
    }

    fn build(
        &self,
        decorator: RequestBuilderDecorator,
        streaming: bool,
    ) -> Result<Box<dyn Provider>> {
        if let Some(config) = &self.definition {
            let mut config = config.clone();
            if !streaming {
                config.supports_streaming = Some(false);
            }
            let keys = MatrixKeyResolver {
                requires_auth: config.requires_auth,
            };
            return match config.engine {
                ProviderEngine::OpenAI => Ok(Box::new(
                    openai::from_declarative_config(config.clone(), None, keys)?
                        .map_api_client(|client| client.with_request_builder(decorator))
                        .build(),
                )),
                ProviderEngine::Anthropic => Ok(Box::new(
                    goose_providers::anthropic::from_declarative_config(
                        config.clone(),
                        None,
                        keys,
                    )?
                    .map_api_client(|client| client.with_request_builder(decorator))
                    .build(),
                )),
                ProviderEngine::Ollama => Ok(Box::new(
                    goose_providers::ollama::from_declarative_config(config.clone(), None, keys)?
                        .map_api_client(|client| client.with_request_builder(decorator))
                        .build(),
                )),
            };
        }
        let client = |host: String, auth| -> Result<ApiClient> {
            Ok(ApiClient::new_with_tls(host, auth, None)?.with_request_builder(decorator.clone()))
        };
        let bearer = |key| AuthMethod::BearerToken(env_or(key, DUMMY_KEY));
        let provider: Box<dyn Provider> = match self.name.as_str() {
            "openai" => {
                let (host, query, has_v1) = if let Some(host) = env_value("OPENAI_HOST") {
                    (host, Vec::new(), true)
                } else if let Some(base_url) = env_value("OPENAI_BASE_URL") {
                    openai::parse_openai_base_url(&base_url)?
                } else {
                    ("https://api.openai.com".into(), Vec::new(), true)
                };
                let native = url::Url::parse(&host)?.host_str() == Some("api.openai.com");
                let mut api_client = client(host, bearer("OPENAI_API_KEY"))?.with_query(query);
                for (key, header) in [
                    ("OPENAI_ORGANIZATION", "OpenAI-Organization"),
                    ("OPENAI_PROJECT", "OpenAI-Project"),
                ] {
                    if let Some(value) = env_value(key) {
                        api_client = api_client.with_header(header, &value)?;
                    }
                }
                Box::new(
                    OpenAiProviderBuilder::new(api_client)
                        .supports_streaming(streaming)
                        .native_openai(native)
                        .preserve_thinking_context(!native)
                        .base_path(env_or(
                            "OPENAI_BASE_PATH",
                            if has_v1 {
                                "v1/chat/completions"
                            } else {
                                "chat/completions"
                            },
                        ))
                        .build(),
                )
            }
            "anthropic" => Box::new(
                AnthropicProviderBuilder::new(
                    client(
                        env_or("ANTHROPIC_HOST", "https://api.anthropic.com"),
                        AuthMethod::ApiKey {
                            header_name: "x-api-key".into(),
                            key: env_or("ANTHROPIC_API_KEY", DUMMY_KEY),
                        },
                    )?
                    .with_header("anthropic-version", ANTHROPIC_API_VERSION)?,
                )
                .format_options(AnthropicFormatOptions::native())
                .build(),
            ),
            "google" => Box::new(GoogleProvider::new(
                env_or("GOOGLE_HOST", "https://generativelanguage.googleapis.com"),
                env_or("GOOGLE_API_KEY", DUMMY_KEY),
                None,
                Some(decorator.clone()),
                None,
            )?),
            "openrouter" => Box::new(OpenRouterProvider::new(
                client(
                    env_or("OPENROUTER_HOST", "https://openrouter.ai"),
                    bearer("OPENROUTER_API_KEY"),
                )?,
                None,
                None,
            )),
            "ollama" => Box::new(
                OllamaProviderBuilder::new(client(
                    openai::ensure_url_scheme(&env_or("OLLAMA_HOST", "http://localhost:11434")),
                    AuthMethod::NoAuth,
                )?)
                .build(),
            ),
            "openai_compatible" => Box::new(
                OpenAiCompatibleProvider::new(
                    self.name.clone(),
                    client(
                        env_or(
                            "OPENAI_COMPATIBLE_BASE_URL",
                            "https://provider-matrix.invalid/v1",
                        ),
                        bearer("OPENAI_COMPATIBLE_API_KEY"),
                    )?,
                    String::new(),
                )
                .with_supports_streaming(streaming),
            ),
            "databricks" => Box::new(DatabricksProvider::new(
                env_or("DATABRICKS_HOST", "https://provider-matrix.invalid"),
                DatabricksAuth::token(env_or("DATABRICKS_TOKEN", DUMMY_KEY)),
                RetryConfig::new(0, 0, 1.0, 0),
                None,
                None,
                None,
                Some(decorator.clone()),
                None,
                None,
                None,
            )?),
            "databricks_v2" => {
                let mut provider = DatabricksV2Provider::new(
                    env_or("DATABRICKS_HOST", "https://provider-matrix.invalid"),
                    DatabricksAuth::token(env_or("DATABRICKS_TOKEN", DUMMY_KEY)),
                    RetryConfig::new(0, 0, 1.0, 0),
                    None,
                    None,
                    None,
                    Some(decorator.clone()),
                    None,
                )?;
                if let Some(path) = env_value("DATABRICKS_V2_GATEWAY_PATH") {
                    provider = provider.with_gateway_path(&path)?;
                }
                Box::new(provider)
            }
            "snowflake" => Box::new(SnowflakeProvider::new(
                env_or("SNOWFLAKE_HOST", "provider-matrix.snowflakecomputing.com"),
                env_or("SNOWFLAKE_TOKEN", DUMMY_KEY),
                None,
                Some(decorator.clone()),
            )?),
            "azure_foundry" => {
                let endpoint = env_or(
                    "AZURE_FOUNDRY_ENDPOINT",
                    "https://provider-matrix.services.ai.azure.com",
                );
                let maas = endpoint_kind(&endpoint) == EndpointKind::Maas;
                let auth = |anthropic: bool, chat: bool| {
                    if let Some(key) = env_value("AZURE_FOUNDRY_API_KEY") {
                        if maas && chat {
                            AuthMethod::BearerToken(key)
                        } else {
                            AuthMethod::ApiKey {
                                header_name: if anthropic { "x-api-key" } else { "api-key" }.into(),
                                key,
                            }
                        }
                    } else {
                        bearer("AZURE_FOUNDRY_AD_TOKEN")
                    }
                };
                Box::new(AzureFoundryProvider::create(
                    endpoint,
                    env_value("AZURE_FOUNDRY_API_VERSION"),
                    Some(self.model.clone()),
                    auth(false, true),
                    auth(false, false),
                    auth(true, false),
                    auth(false, false),
                    None,
                    Some(decorator.clone()),
                )?)
            }
            other => bail!("native provider is missing a matrix factory: {other}"),
        };
        Ok(provider)
    }
}

fn cases() -> Result<Vec<Case>> {
    let definitions: BTreeMap<_, _> = declarative::fixed_provider_config_entries()
        .into_iter()
        .map(|(filename, definition)| (filename.trim_end_matches(".json"), definition))
        .collect();
    let mut cases = Vec::new();
    let mut covered = HashSet::new();
    for &(name, model) in PROVIDER_MODELS {
        covered.insert(name);
        let mut case = if let Some(definition) = definitions.get(name) {
            Case::declarative(name, model, definition)?
        } else {
            let required: &[&str] = match name {
                "openai" => &["OPENAI_API_KEY"],
                "anthropic" => &["ANTHROPIC_API_KEY"],
                "google" => &["GOOGLE_API_KEY"],
                "openrouter" => &["OPENROUTER_API_KEY"],
                "ollama" => &["OLLAMA_HOST", "GOOSE_PROVIDER_MATRIX_OLLAMA_MODEL"],
                "openai_compatible" => &["OPENAI_COMPATIBLE_API_KEY", "OPENAI_COMPATIBLE_BASE_URL"],
                "databricks" | "databricks_v2" => &["DATABRICKS_TOKEN", "DATABRICKS_HOST"],
                "snowflake" => &["SNOWFLAKE_TOKEN", "SNOWFLAKE_HOST"],
                "azure_foundry" => &["AZURE_FOUNDRY_ENDPOINT"],
                other => bail!("unknown provider in PROVIDER_MODELS: {other}"),
            };
            Case::native(name, model, required)
        };
        if name == "azure_foundry" {
            if env_value("AZURE_FOUNDRY_API_KEY").is_none()
                && env_value("AZURE_FOUNDRY_AD_TOKEN").is_none()
            {
                case.missing
                    .push("AZURE_FOUNDRY_API_KEY or AZURE_FOUNDRY_AD_TOKEN".into());
            }
            if let Some(model) = env_value("AZURE_FOUNDRY_MODEL") {
                case.model = env_or(&model_override_key(name), &model);
            } else if env_value(&model_override_key(name)).is_none() {
                case.missing.push(
                    "AZURE_FOUNDRY_MODEL or GOOSE_PROVIDER_MATRIX_AZURE_FOUNDRY_MODEL".into(),
                );
            }
        }
        cases.push(case);
    }
    ensure!(
        definitions.keys().all(|name| covered.contains(name)),
        "bundled providers missing from PROVIDER_MODELS: {:?}",
        definitions
            .keys()
            .filter(|name| !covered.contains(*name))
            .collect::<Vec<_>>()
    );
    cases.sort_by(|a, b| (&a.name, &a.model).cmp(&(&b.name, &b.model)));
    ensure!(
        cases
            .windows(2)
            .all(|pair| (&pair[0].name, &pair[0].model) != (&pair[1].name, &pair[1].model)),
        "duplicate matrix provider/model pair"
    );
    Ok(cases)
}

fn transport(recording: Arc<Mutex<Recording>>, live: bool) -> RequestBuilderDecorator {
    Arc::new(move |builder| {
        let request = builder
            .try_clone()
            .ok_or_else(|| anyhow!("request cannot be cloned"))?
            .build()?;
        let mut recording = recording.lock().unwrap();
        // ApiClient decorates before attaching the JSON body. The provider's
        // start_log supplies that exact body; discovery has no such log entry.
        if request.method() != reqwest::Method::POST
            || recording.turn_mut()?.get("payload").is_none()
        {
            return Err(ProviderError::ExecutionError(NOT_SENT.into()).into());
        }
        if recording.inference_attempts != 0 {
            return Err(ProviderError::ExecutionError(
                "matrix allows only one inference attempt; retry blocked".into(),
            )
            .into());
        }
        recording.inference_attempts += 1;
        let mut endpoint = request.url().clone();
        endpoint.set_query(None);
        endpoint.set_fragment(None);
        let _ = endpoint.set_username("");
        let _ = endpoint.set_password(None);
        let turn = recording.turn_mut()?;
        turn["method"] = json!(request.method().as_str());
        turn["endpoint"] = json!(redact_text(endpoint.as_str()));
        recording.document["live_attempted"] = json!(live);
        recording.save()?;
        if live {
            Ok(builder)
        } else {
            Err(ProviderError::ExecutionError(NOT_SENT.into()).into())
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    Text,
    ThinkingOff,
    ThinkingLow,
    ThinkingMedium,
    ToolCall,
    ToolContinuation,
    MultiTurn,
    StructuredOutput,
    Vision,
    NonStreaming,
}

impl Scenario {
    const ALL: [Self; 10] = [
        Self::Text,
        Self::ThinkingOff,
        Self::ThinkingLow,
        Self::ThinkingMedium,
        Self::ToolCall,
        Self::ToolContinuation,
        Self::MultiTurn,
        Self::StructuredOutput,
        Self::Vision,
        Self::NonStreaming,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::ThinkingOff => "thinking_off",
            Self::ThinkingLow => "thinking_low",
            Self::ThinkingMedium => "thinking_medium",
            Self::ToolCall => "tool_call",
            Self::ToolContinuation => "tool_continuation",
            Self::MultiTurn => "multi_turn",
            Self::StructuredOutput => "structured_output",
            Self::Vision => "vision",
            Self::NonStreaming => "non_streaming",
        }
    }

    fn effort(self) -> Option<ThinkingEffort> {
        match self {
            Self::ThinkingOff => Some(ThinkingEffort::Off),
            Self::ThinkingLow => Some(ThinkingEffort::Low),
            Self::ThinkingMedium => Some(ThinkingEffort::Medium),
            _ => None,
        }
    }

    fn uses_tools(self) -> bool {
        matches!(self, Self::ToolCall | Self::ToolContinuation)
    }
}

const TOOL_PROMPT: &str = "Call matrix_echo exactly once with {\"value\":\"matrix-input\"}. After receiving its result, reply with exactly the result text and do not call another tool.";
const TOOL_RESULT: &str = "matrix-result-731";
const HISTORY_WORD: &str = "cobalt";
const RED_IMAGE: &str = "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAF0lEQVR4nGP4z8BAEiJN9aiGUQ1DSgMAkPn/Afnh+ngAAAAASUVORK5CYII=";

fn echo_tool() -> Tool {
    Tool::new("matrix_echo", "Return a deterministic compatibility-matrix result.", Arc::new(
        json!({"type":"object","properties":{"value":{"type":"string","enum":["matrix-input"]}},
            "required":["value"],"additionalProperties":false}).as_object().unwrap().clone(),
    ))
}

fn canonical(case: &Case) -> Option<CanonicalModel> {
    let provider = case
        .definition
        .as_ref()
        .map_or(case.name.as_str(), |config| config.name.as_str());
    maybe_get_canonical_model(provider, &case.model)
}

fn supports_non_streaming(case: &Case) -> bool {
    if let Some(config) = &case.definition {
        return config.engine == ProviderEngine::OpenAI;
    }
    matches!(case.name.as_str(), "openai" | "openai_compatible")
}

fn can_force_tools(case: &Case) -> bool {
    // These routes forward tool_choice, unlike the Anthropic/Google formatters.
    // Ollama's OpenAI-compatible endpoint does not promise to honor forcing.
    if matches!(
        case.name.as_str(),
        "ollama" | "ollama_cloud" | "snowflake" | "google"
    ) {
        return false;
    }
    if let Some(config) = &case.definition {
        return config.engine == ProviderEngine::OpenAI;
    }
    matches!(
        case.name.as_str(),
        "openai" | "openai_compatible" | "openrouter"
    ) || (matches!(
        case.name.as_str(),
        "databricks" | "databricks_v2" | "azure_foundry"
    ) && goose_providers::formats::openai::is_openai_responses_model(&case.model))
}

fn unsupported(case: &Case, scenario: Scenario) -> Option<String> {
    let model = canonical(case);
    if scenario.uses_tools() {
        if model.as_ref().is_some_and(|model| !model.tool_call) {
            return Some("catalog declares that the model does not support tool calls".into());
        }
        if scenario == Scenario::ToolContinuation && case.name == "snowflake" {
            return Some(
                "Snowflake formatter uses text fallback rather than native tool-call correlation"
                    .into(),
            );
        }
    }
    if scenario == Scenario::Vision {
        if case.name == "snowflake" {
            return Some("Snowflake formatter does not encode image input".into());
        }
        return match model {
            Some(model) if model.modalities.input.contains(&Modality::Image) => None,
            Some(_) => Some("catalog declares no image input support".into()),
            None => Some("image input capability is unknown".into()),
        };
    }
    if scenario == Scenario::NonStreaming && !supports_non_streaming(case) {
        return Some("non-streaming mode is not exposed by this provider factory".into());
    }
    if scenario == Scenario::StructuredOutput {
        // No uniform schema capability exists in the catalog. Only advertise
        // routes whose request format and schema support are known here.
        if !matches!(case.name.as_str(), "openai" | "openai_compatible")
            || !model.is_some_and(|model| {
                [
                    "openai/gpt-4o",
                    "openai/gpt-4.1",
                    "openai/gpt-5",
                    "openai/gpt-6",
                ]
                .iter()
                .any(|prefix| model.id.starts_with(prefix))
            })
        {
            return Some(
                "strict JSON-schema output is not declared for this provider/model".into(),
            );
        }
    }
    None
}

fn model_config(case: &Case, scenario: Scenario) -> ModelConfig {
    let provider = case
        .definition
        .as_ref()
        .map_or(case.name.as_str(), |config| config.name.as_str());
    let mut model = ModelConfig::new(&case.model).with_canonical_limits(provider);
    if let Some(effort) = scenario.effort() {
        model = model.with_thinking_effort(effort);
    }
    if case.definition.as_ref().is_some_and(|config| {
        config
            .models
            .iter()
            .any(|declared| declared.name == case.model && declared.reasoning)
    }) {
        model.reasoning = Some(true);
    }
    model.max_tokens = Some(model.max_tokens.unwrap_or(12288).min(12288));
    if scenario == Scenario::StructuredOutput {
        model = model.with_merged_request_params(HashMap::from([("response_format".into(), json!({
            "type":"json_schema","json_schema":{"name":"matrix_greeting","strict":true,
                "schema":{"type":"object","properties":{"greeting":{"type":"string","enum":["hello"]}},
                    "required":["greeting"],"additionalProperties":false}}
        }))]));
    }
    model
}

fn wire_controls(payload: &Value) -> Value {
    let mut controls = serde_json::Map::new();
    for (name, pointer) in [
        ("thinking", "/thinking"),
        ("reasoning", "/reasoning"),
        ("reasoning_effort", "/reasoning_effort"),
        ("output_config", "/output_config"),
        ("think", "/think"),
        ("google_thinking", "/generationConfig/thinkingConfig"),
        ("tool_choice", "/tool_choice"),
        ("response_format", "/response_format"),
        ("text", "/text"),
    ] {
        if let Some(value) = payload.pointer(pointer) {
            controls.insert(name.into(), value.clone());
        }
    }
    Value::Object(controls)
}

fn validate_thinking_controls(payload: &Value, model: &ModelConfig) -> Result<()> {
    let Some(effort) = model.thinking_effort() else {
        return Ok(());
    };
    if payload.get("input").is_some() && model.is_openai_reasoning_model() {
        let expected = goose_providers::formats::openai::openai_reasoning_effort_for_thinking(
            &model.model_name,
            effort,
        );
        ensure!(
            payload["reasoning"]["effort"] == json!(expected),
            "Responses reasoning effort missing or incorrect"
        );
    }
    if payload["thinking"]["type"] == "adaptive" {
        let expected = if effort == ThinkingEffort::Off {
            "high".to_string()
        } else {
            effort.to_string()
        };
        ensure!(
            payload["output_config"]["effort"] == expected,
            "adaptive thinking effort missing or incorrect"
        );
    }
    if payload["thinking"]["type"] == "enabled" {
        let expected = match effort {
            ThinkingEffort::Low => 4000,
            ThinkingEffort::Medium => 10000,
            _ => return Ok(()),
        }
        .min(model.max_output_tokens() - 1024);
        ensure!(
            payload["thinking"]["budget_tokens"] == expected,
            "thinking budget missing or incorrect"
        );
    }
    Ok(())
}

fn validate_response(message: &Message, usage: &ProviderUsage) -> Result<()> {
    ensure!(
        message.role == Role::Assistant,
        "response role is not assistant"
    );
    ensure!(
        message
            .content
            .iter()
            .all(|block| block.as_error().is_none()),
        "response contains an error block"
    );
    for count in [
        usage.usage.input_tokens,
        usage.usage.output_tokens,
        usage.usage.total_tokens,
        usage.usage.cache_read_input_tokens,
        usage.usage.cache_write_input_tokens,
    ]
    .into_iter()
    .flatten()
    {
        ensure!(count >= 0, "negative token usage");
    }
    if let (Some(input), Some(output), Some(total)) = (
        usage.usage.input_tokens,
        usage.usage.output_tokens,
        usage.usage.total_tokens,
    ) {
        ensure!(
            i64::from(total) >= i64::from(input) + i64::from(output),
            "total token usage is less than input plus output"
        );
    }
    if let Some(input) = usage.usage.input_tokens {
        for cached in [
            usage.usage.cache_read_input_tokens,
            usage.usage.cache_write_input_tokens,
        ]
        .into_iter()
        .flatten()
        {
            ensure!(cached <= input, "cache token count exceeds total input");
        }
    }
    for reason in usage.finish_reasons.iter().flatten() {
        let reason = reason.to_lowercase();
        ensure!(
            ![
                "length",
                "max_tokens",
                "max_output_tokens",
                "max_completion_tokens",
                "content_filter",
                "incomplete",
                "failed",
                "safety",
                "recitation",
                "blocked"
            ]
            .contains(&reason.as_str()),
            "unsuccessful finish reason: {reason}"
        );
    }
    Ok(())
}

fn validate_tool_call(message: &Message) -> Result<String> {
    let calls: Vec<_> = message
        .content
        .iter()
        .filter_map(|block| block.as_tool_request())
        .collect();
    ensure!(
        calls.len() == 1,
        "expected exactly one tool call, got {}",
        calls.len()
    );
    let call = calls[0];
    ensure!(!call.id.trim().is_empty(), "tool-call ID is empty");
    let arguments = call
        .tool_call
        .as_ref()
        .map_err(|error| anyhow!("invalid tool call: {error:?}"))?;
    ensure!(
        arguments.name == "matrix_echo",
        "unexpected tool name: {}",
        arguments.name
    );
    ensure!(
        arguments.arguments == Some(json!({"value":"matrix-input"}).as_object().unwrap().clone()),
        "tool arguments do not match schema/fixture"
    );
    Ok(call.id.clone())
}

fn payload_texts(payload: &Value) -> Vec<&str> {
    match payload {
        Value::String(text) => vec![text.as_str()],
        Value::Array(values) => values.iter().flat_map(payload_texts).collect(),
        Value::Object(values) => values.values().flat_map(payload_texts).collect(),
        _ => Vec::new(),
    }
}

fn validate_tool_history(payload: &Value, id: &str) -> Result<()> {
    if let Some(input) = payload["input"].as_array() {
        ensure!(
            input.iter().any(|item| item["type"] == "function_call"
                && item["call_id"] == id
                && item["name"] == "matrix_echo"),
            "Responses tool call missing"
        );
        ensure!(
            input
                .iter()
                .any(|item| item["type"] == "function_call_output"
                    && item["call_id"] == id
                    && payload_texts(&item["output"]).contains(&TOOL_RESULT)),
            "Responses tool result missing or mismatched"
        );
    } else if let Some(contents) = payload["contents"].as_array() {
        let parts: Vec<_> = contents
            .iter()
            .flat_map(|message| message["parts"].as_array().into_iter().flatten())
            .collect();
        ensure!(
            parts.iter().any(|part| part["functionCall"]["id"] == id
                && part["functionCall"]["name"] == "matrix_echo"),
            "Google tool call missing"
        );
        ensure!(
            parts.iter().any(|part| part["functionResponse"]["id"] == id
                && part["functionResponse"]["name"] == "matrix_echo"
                && payload_texts(&part["functionResponse"]).contains(&TOOL_RESULT)),
            "Google tool result missing or mismatched"
        );
    } else if let Some(messages) = payload["messages"].as_array() {
        let blocks: Vec<_> = messages
            .iter()
            .flat_map(|message| message["content"].as_array().into_iter().flatten())
            .collect();
        if blocks.iter().any(|block| block["type"] == "tool_use") {
            ensure!(
                blocks.iter().any(|block| block["type"] == "tool_use"
                    && block["id"] == id
                    && block["name"] == "matrix_echo"),
                "Anthropic tool call missing"
            );
            ensure!(
                blocks.iter().any(|block| block["type"] == "tool_result"
                    && block["tool_use_id"] == id
                    && payload_texts(&block["content"]).contains(&TOOL_RESULT)),
                "Anthropic tool result missing or mismatched"
            );
        } else {
            ensure!(
                messages
                    .iter()
                    .flat_map(|message| message["tool_calls"].as_array().into_iter().flatten())
                    .any(|call| call["id"] == id && call["function"]["name"] == "matrix_echo"),
                "chat tool call missing"
            );
            ensure!(
                messages.iter().any(|message| message["role"] == "tool"
                    && message["tool_call_id"] == id
                    && payload_texts(&message["content"]).contains(&TOOL_RESULT)),
                "chat tool result missing or mismatched"
            );
        }
    } else {
        bail!("unknown tool-history payload format");
    }
    Ok(())
}

fn validate_text(message: &Message) -> Result<()> {
    ensure!(
        !message.as_concat_text().trim().is_empty(),
        "empty live text response"
    );
    ensure!(
        message
            .content
            .iter()
            .all(|block| block.as_tool_request().is_none()),
        "unexpected tool call in text response"
    );
    Ok(())
}

async fn request_turn(
    provider: &dyn Provider,
    model: &ModelConfig,
    messages: &[Message],
    tools: &[Tool],
    label: &str,
    recording: &Arc<Mutex<Recording>>,
    live: bool,
) -> Result<Option<Message>> {
    {
        let mut recording = recording.lock().unwrap();
        recording.inference_attempts = 0;
        recording.document["turns"]
            .as_array_mut()
            .unwrap()
            .push(json!({"label":label}));
    }
    let started = Instant::now();
    let metrics = Arc::new(Mutex::new((0usize, None)));
    let stream_metrics = metrics.clone();
    let result = tokio::time::timeout(Duration::from_secs(90), async {
        let stream = provider.stream(model, SYSTEM, messages, tools).await?;
        let stream = stream.inspect(move |_| {
            let mut metrics = stream_metrics.lock().unwrap();
            metrics.0 += 1;
            metrics
                .1
                .get_or_insert_with(|| started.elapsed().as_millis());
        });
        collect_stream(Box::pin(stream)).await
    })
    .await;
    {
        let mut recording = recording.lock().unwrap();
        let turn = recording.turn_mut()?;
        turn["elapsed_ms"] = json!(started.elapsed().as_millis());
        let metrics = metrics.lock().unwrap();
        turn["stream_item_count"] = json!(metrics.0);
        turn["time_to_first_stream_item_ms"] = json!(metrics.1);
    }
    let reply = result?;
    let message = if live {
        let (message, usage) = reply?;
        {
            let mut recording = recording.lock().unwrap();
            let turn = recording.turn_mut()?;
            turn["response"] = serde_json::to_value(&message)?;
            turn["usage"] = serde_json::to_value(&usage)?;
        }
        validate_response(&message, &usage)?;
        Some(message)
    } else {
        match reply {
            Err(ProviderError::ExecutionError(message)) if message == NOT_SENT => None,
            Err(error) => return Err(error.into()),
            Ok(_) => bail!("recording-only request unexpectedly completed"),
        }
    };
    let recording = recording.lock().unwrap();
    ensure!(
        recording.inference_attempts == 1,
        "expected one inference attempt per turn"
    );
    let turn = recording.document["turns"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    ensure!(turn["payload"].is_object(), "no request payload captured");
    validate_thinking_controls(&turn["payload"], model)?;
    ensure!(
        turn["model_config"]["model_name"] == model.model_name,
        "model ID was not propagated"
    );
    ensure!(
        turn["model_config"]["request_params"]["thinking_effort"] == json!(model.thinking_effort()),
        "thinking effort was not propagated"
    );
    let texts = payload_texts(&turn["payload"]);
    let mut cursor = 0;
    for message in messages {
        let text = message.as_concat_text();
        if text.is_empty() {
            continue;
        }
        let offset = texts[cursor..]
            .iter()
            .position(|value| value.contains(&text))
            .ok_or_else(|| anyhow!("message text absent or out of order in payload: {text}"))?;
        cursor += offset + 1;
    }
    let request = turn["payload"].to_string();
    ensure!(
        request.contains(serde_json::to_string(&messages[0].as_concat_text())?.trim_matches('"')),
        "prompt absent from payload"
    );
    if !tools.is_empty() {
        ensure!(
            request.contains("matrix_echo") && request.contains("matrix-input"),
            "tool schema absent from payload"
        );
    }
    if model.request_param::<String>("tool_choice").as_deref() == Some("required") {
        ensure!(
            turn["payload"]["tool_choice"] == "required",
            "required tool choice missing from payload"
        );
    }
    Ok(message)
}

async fn execute_scenario(
    case: &Case,
    scenario: Scenario,
    recording: &Arc<Mutex<Recording>>,
    live: bool,
) -> Result<()> {
    let provider = case.build(
        transport(recording.clone(), live),
        scenario != Scenario::NonStreaming,
    )?;
    let model = model_config(case, scenario);
    if scenario.uses_tools() {
        let user = Message::user().with_text(TOOL_PROMPT);
        let tools = [echo_tool()];
        let forced = can_force_tools(case);
        recording.lock().unwrap().document["tool_selection"] = json!(if forced {
            "required"
        } else {
            "prompt_directed"
        });
        let tool_model = if forced {
            model.clone().with_merged_request_params(HashMap::from([(
                "tool_choice".into(),
                json!("required"),
            )]))
        } else {
            model.clone()
        };
        let reply = request_turn(
            provider.as_ref(),
            &tool_model,
            std::slice::from_ref(&user),
            &tools,
            "tool_call",
            recording,
            live,
        )
        .await?;
        let (assistant, id) = if let Some(reply) = reply {
            let id = validate_tool_call(&reply)?;
            (reply, id)
        } else {
            let id = "matrix-call-1".to_string();
            let call = CallToolRequestParams::new("matrix_echo")
                .with_arguments(json!({"value":"matrix-input"}).as_object().unwrap().clone());
            (
                Message::assistant().with_tool_request(id.clone(), Ok(call)),
                id,
            )
        };
        if scenario == Scenario::ToolContinuation {
            recording.lock().unwrap().document["synthetic_history"] = json!(!live);
            let messages = [
                user,
                assistant,
                Message::user().with_tool_response(
                    id.clone(),
                    Ok(CallToolResult::success(vec![ContentBlock::text(
                        TOOL_RESULT,
                    )])),
                ),
            ];
            let reply = request_turn(
                provider.as_ref(),
                &model,
                &messages,
                &tools,
                "tool_result",
                recording,
                live,
            )
            .await?;
            let mut recording = recording.lock().unwrap();
            validate_tool_history(&recording.turn_mut()?["payload"], &id)?;
            if let Some(reply) = reply {
                validate_text(&reply)?;
                let exact = reply.as_concat_text().trim() == TOOL_RESULT;
                recording.document["instruction_compliance"] = json!(exact);
                ensure!(
                    exact,
                    "tool-result continuation did not return the fixture result"
                );
            }
        }
        return Ok(());
    }
    let messages = match scenario {
        Scenario::MultiTurn => vec![
            Message::user().with_text(format!("Remember the secret word: {HISTORY_WORD}.")),
            Message::assistant().with_text("I will remember it."),
            Message::user().with_text("Reply with only the secret word from my earlier message."),
        ],
        Scenario::StructuredOutput => {
            vec![Message::user().with_text("Return a JSON object with greeting set to hello.")]
        }
        Scenario::Vision => vec![Message::user()
            .with_text("Name the dominant color of this image. Reply with one color word.")
            .with_image(RED_IMAGE, "image/png")],
        _ => vec![Message::user().with_text(PROMPT)],
    };
    let reply = request_turn(
        provider.as_ref(),
        &model,
        &messages,
        &[],
        scenario.name(),
        recording,
        live,
    )
    .await?;
    if scenario == Scenario::MultiTurn {
        ensure!(
            recording.lock().unwrap().turn_mut()?["payload"]
                .to_string()
                .contains("I will remember it."),
            "assistant history missing from payload"
        );
    }
    if scenario == Scenario::Vision {
        ensure!(
            recording.lock().unwrap().turn_mut()?["payload"]
                .to_string()
                .contains(RED_IMAGE),
            "image data missing from payload"
        );
    }
    if scenario == Scenario::StructuredOutput {
        let mut recording = recording.lock().unwrap();
        let payload = &recording.turn_mut()?["payload"];
        let format = payload
            .pointer("/text/format")
            .unwrap_or(&payload["response_format"]);
        let schema = if format["type"] == "json_schema" && format.get("json_schema").is_some() {
            &format["json_schema"]
        } else {
            format
        };
        ensure!(
            format["type"] == "json_schema",
            "JSON-schema response format missing"
        );
        ensure!(
            schema["strict"] == true && schema["name"] == "matrix_greeting",
            "strict schema envelope missing"
        );
        ensure!(
            schema["schema"]
                == model.request_params.as_ref().unwrap()["response_format"]["json_schema"]
                    ["schema"],
            "schema was not transmitted intact"
        );
    }
    if scenario == Scenario::NonStreaming {
        ensure!(
            recording.lock().unwrap().turn_mut()?["payload"]["stream"] != true,
            "non-streaming request emitted stream=true"
        );
    }
    if let Some(reply) = reply {
        validate_text(&reply)?;
        let text = reply.as_concat_text();
        let normalized = text.trim().to_lowercase();
        let compliant = match scenario {
            Scenario::MultiTurn => normalized == HISTORY_WORD,
            Scenario::Vision => normalized == "red",
            Scenario::StructuredOutput => {
                serde_json::from_str::<Value>(&text)? == json!({"greeting":"hello"})
            }
            _ => normalized == "hello",
        };
        recording.lock().unwrap().document["instruction_compliance"] = json!(compliant);
        // Basic greeting obedience is reported separately from API compatibility.
        if matches!(
            scenario,
            Scenario::MultiTurn | Scenario::Vision | Scenario::StructuredOutput
        ) {
            ensure!(compliant, "response did not satisfy the scenario fixture");
        }
    }
    Ok(())
}

fn path_component(value: &str) -> String {
    // Hex encoding avoids collisions between IDs containing '/', '.', or ':'.
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

const SNAPSHOT_VERSION: u64 = 1;

struct SnapshotCache {
    directory: PathBuf,
    replay_verified: bool,
}

impl SnapshotCache {
    fn from_env() -> Result<Self> {
        Ok(Self {
            directory: env_value("GOOSE_PROVIDER_MATRIX_SNAPSHOT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/snapshots/provider_compatibility")
                }),
            replay_verified: env_flag("REPLAY_VERIFIED_LIVE")?,
        })
    }

    fn path(&self, case: &Case, scenario: Scenario) -> PathBuf {
        self.directory
            .join(&case.name)
            .join(path_component(&case.model))
            .join(format!("{}.json", scenario.name()))
    }

    fn verified(&self, case: &Case, scenario: Scenario) -> Result<Option<Value>> {
        if self.replay_verified {
            return Ok(None);
        }
        let path = self.path(case, scenario);
        let contents = match fs::read(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let snapshot: Value = serde_json::from_slice(&contents).map_err(|error| {
            anyhow!(
                "invalid snapshot {}: {error}; delete it to invalidate",
                path.display()
            )
        })?;
        ensure!(
            snapshot["version"] == SNAPSHOT_VERSION
                && snapshot["status"] == "verified"
                && snapshot["provider"] == case.name
                && snapshot["model"] == case.model
                && snapshot["scenario"] == scenario.name()
                && snapshot["verification"]["live"] == true
                && snapshot["verification"]["scenario_assertions"] == "passed",
            "invalid verified snapshot metadata in {}; delete it to invalidate",
            path.display()
        );
        let requests = snapshot["requests"]
            .as_array()
            .ok_or_else(|| anyhow!("missing requests in snapshot {}", path.display()))?;
        let expected_turns = if scenario == Scenario::ToolContinuation {
            2
        } else {
            1
        };
        ensure!(
            requests.len() == expected_turns
                && requests
                    .iter()
                    .all(|request| request["payload"].is_object() && request["method"] == "POST"),
            "invalid requests in snapshot {}; delete it to invalidate",
            path.display()
        );
        Ok(Some(snapshot))
    }

    fn save_verified(&self, case: &Case, scenario: Scenario, document: &Value) -> Result<()> {
        ensure!(
            document["status"] == "live_ok"
                && document["live_attempted"] == true
                && document["record_only"] == false
                && document["provider"] == case.name
                && document["model"] == case.model
                && document["scenario"] == scenario.name(),
            "only successful live requests can be marked verified"
        );
        let expected_turns = if scenario == Scenario::ToolContinuation {
            2
        } else {
            1
        };
        let turns = document["turns"]
            .as_array()
            .ok_or_else(|| anyhow!("verified row has no turns"))?;
        ensure!(
            turns.len() == expected_turns
                && turns.iter().all(|turn| {
                    turn["payload"].is_object()
                        && turn["method"] == "POST"
                        && turn["response"].is_object()
                        && turn["usage"].is_object()
                        && turn.get("provider_error").is_none()
                }),
            "live scenario did not complete all turns successfully"
        );
        let snapshot = verified_snapshot(document)?;
        let path = self.path(case, scenario);
        let directory = path.parent().unwrap();
        fs::create_dir_all(directory)?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(&serde_json::to_vec_pretty(&snapshot)?)?;
        temporary.write_all(b"\n")?;
        temporary.persist(&path)?;
        Ok(())
    }
}

fn verified_snapshot(document: &Value) -> Result<Value> {
    let turns = document["turns"]
        .as_array()
        .ok_or_else(|| anyhow!("verified row has no turns"))?;
    let requests: Vec<_> = turns.iter().map(|turn| json!({
        "label":turn["label"], "method":turn["method"], "endpoint":turn["endpoint"],
        "model_config":turn["model_config"], "payload":turn["payload"], "wire_controls":turn["wire_controls"],
    })).collect();
    // Only fixture request bodies and safe endpoint metadata are committed. The
    // request logger runs before auth; responses, usage, errors, and timing stay local.
    let mut snapshot = json!({
        "version":SNAPSHOT_VERSION, "status":"verified", "provider":document["provider"],
        "verification":{"live":true,"scenario_assertions":"passed"},
        "model":document["model"], "scenario":document["scenario"],
        "requested_thinking_effort":document["requested_thinking_effort"], "requests":requests,
    });
    for key in [
        "tool_selection",
        "synthetic_history",
        "instruction_compliance",
    ] {
        if let Some(value) = document.get(key) {
            snapshot[key] = value.clone();
        }
    }
    redact_snapshot(&mut snapshot);
    Ok(snapshot)
}

fn redact_snapshot(value: &mut Value) {
    match value {
        Value::String(text) => *text = redact_text(text),
        Value::Array(values) => values.iter_mut().for_each(redact_snapshot),
        Value::Object(values) => values.values_mut().for_each(redact_snapshot),
        _ => {}
    }
}

async fn run_row(
    case: &Case,
    scenario: Scenario,
    output: &Path,
    current: &CurrentRecording,
    record_only: bool,
    cache: &SnapshotCache,
) -> Result<Value> {
    let verified = if record_only {
        None
    } else {
        cache.verified(case, scenario)?
    };
    if let Some(snapshot) = verified {
        let row = json!({"provider":case.name,"model":case.model,"scenario":scenario.name(),
                "status":"verified", "live_attempted":false,"snapshot":cache.path(case, scenario),
                "requests":snapshot["requests"]});
        let directory = output.join(&case.name).join(path_component(&case.model));
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join(format!("{}.json", scenario.name())),
            serde_json::to_vec_pretty(&row)?,
        )?;
        println!(
            "{:<20} {:<18} VERIFIED; skipped ({})",
            case.name,
            scenario.name(),
            case.model
        );
        return Ok(row);
    }
    let directory = output.join(&case.name).join(path_component(&case.model));
    fs::create_dir_all(&directory)?;
    let live = !record_only && case.missing.is_empty();
    let recording = Arc::new(Mutex::new(Recording {
        path: directory.join(format!("{}.json", scenario.name())),
        document: json!({"provider":case.name,"model":case.model,"scenario":scenario.name(),
            "requested_thinking_effort":scenario.effort(),"live_attempted":false,"record_only":record_only,
            "missing_env":case.missing,"status":"pending","turns":[],
            "catalog_tool_support":canonical(case).map(|model| model.tool_call)}),
        inference_attempts: 0,
    }));
    if let Some(reason) = unsupported(case, scenario) {
        let mut recording = recording.lock().unwrap();
        recording.document["status"] = json!("unsupported");
        recording.document["reason"] = json!(reason);
        recording.save()?;
        println!(
            "{:<20} {:<18} UNSUPPORTED: {reason}",
            case.name,
            scenario.name()
        );
        return Ok(recording.document.clone());
    }
    recording.lock().unwrap().save()?;
    *current.lock().unwrap() = Some(recording.clone());
    let result = execute_scenario(case, scenario, &recording, live).await;
    *current.lock().unwrap() = None;
    let mut recording = recording.lock().unwrap();
    match result {
        Ok(()) if live => {
            recording.document["status"] = json!("live_ok");
            cache.save_verified(case, scenario, &recording.document)?;
            recording.document["snapshot"] = json!(cache.path(case, scenario));
            println!(
                "{:<20} {:<18} LIVE OK ({})",
                case.name,
                scenario.name(),
                case.model
            );
        }
        Ok(()) => {
            recording.document["status"] = json!("recorded_only");
            println!(
                "{:<20} {:<18} RECORDED; not sent ({})",
                case.name,
                scenario.name(),
                if record_only {
                    "record-only mode".into()
                } else {
                    format!("missing {}", case.missing.join(", "))
                }
            );
        }
        Err(error) => {
            let mut error = redact_text(&format!("{error:#}"));
            let provider_error = recording.document["turns"]
                .as_array()
                .and_then(|turns| turns.last())
                .and_then(|turn| turn["provider_error"].as_str())
                .or_else(|| recording.document["first_provider_error"].as_str());
            if let Some(original) = provider_error {
                error = format!("{original}; {error}");
            }
            recording.document["status"] = json!("failed");
            recording.document["error"] = json!(error);
            println!("{:<20} {:<18} FAILED: {error}", case.name, scenario.name());
        }
    }
    recording.save()?;
    Ok(recording.document.clone())
}

fn filter_values(key: &str) -> Result<Vec<String>> {
    match env_value(key) {
        None => Ok(Vec::new()),
        Some(value) => {
            let values: Vec<_> = value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect();
            ensure!(!values.is_empty(), "{key} contains no filter values");
            Ok(values)
        }
    }
}

fn validate_filter(key: &str, values: &[String], available: &[&str]) -> Result<()> {
    for value in values {
        ensure!(
            available.contains(&value.as_str()),
            "unknown {key} value {value:?}; available: {}",
            available.join(", ")
        );
    }
    Ok(())
}

fn env_flag(key: &str) -> Result<bool> {
    match env_value(key).as_deref() {
        None | Some("0" | "false") => Ok(false),
        Some("1" | "true") => Ok(true),
        Some(value) => bail!("{key} must be 1/0 or true/false, got {value:?}"),
    }
}

fn redact_text(text: &str) -> String {
    let mut message = text.to_string();
    let mut secrets: Vec<_> = env::vars()
        .filter(|(key, value)| {
            !value.is_empty()
                && [
                    "_KEY",
                    "_TOKEN",
                    "_PASSWORD",
                    "_SECRET",
                    "_HOST",
                    "_BASE_URL",
                    "_ENDPOINT",
                ]
                .iter()
                .any(|suffix| key.ends_with(suffix))
        })
        .map(|(_, value)| value)
        .collect();
    secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
    for secret in secrets {
        message = message.replace(&secret, "[REDACTED]");
    }
    message
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "records provider requests and makes billable requests when environment credentials are present"]
async fn providers_pass_selected_compatibility_scenarios() -> Result<()> {
    let provider_filter = filter_values("GOOSE_PROVIDER_MATRIX_PROVIDERS")?;
    let model_filter = filter_values("GOOSE_PROVIDER_MATRIX_MODELS")?;
    let scenario_filter = filter_values("GOOSE_PROVIDER_MATRIX_SCENARIOS")?;
    let record_only = env_flag("GOOSE_PROVIDER_MATRIX_RECORD_ONLY")?;
    let list_only = env_flag("GOOSE_PROVIDER_MATRIX_LIST")?;
    let cache = SnapshotCache::from_env()?;
    let mut cases = cases()?;
    validate_filter(
        "GOOSE_PROVIDER_MATRIX_PROVIDERS",
        &provider_filter,
        &cases
            .iter()
            .map(|case| case.name.as_str())
            .collect::<Vec<_>>(),
    )?;
    cases.retain(|case| provider_filter.is_empty() || provider_filter.contains(&case.name));
    validate_filter(
        "GOOSE_PROVIDER_MATRIX_MODELS",
        &model_filter,
        &cases
            .iter()
            .map(|case| case.model.as_str())
            .collect::<Vec<_>>(),
    )?;
    cases.retain(|case| model_filter.is_empty() || model_filter.contains(&case.model));
    let available = Scenario::ALL.map(Scenario::name);
    validate_filter(
        "GOOSE_PROVIDER_MATRIX_SCENARIOS",
        &scenario_filter,
        &available,
    )?;
    let scenarios: Vec<_> = Scenario::ALL
        .into_iter()
        .filter(|scenario| {
            scenario_filter.is_empty() || scenario_filter.iter().any(|name| name == scenario.name())
        })
        .collect();
    ensure!(
        !cases.is_empty() && !scenarios.is_empty(),
        "filters matched no matrix rows"
    );
    if list_only {
        for case in &cases {
            for &scenario in &scenarios {
                println!(
                    "{} / {} / {}: {}",
                    case.name,
                    case.model,
                    scenario.name(),
                    if !record_only && cache.verified(case, scenario)?.is_some() {
                        "verified (skip)".into()
                    } else {
                        unsupported(case, scenario).map_or_else(
                            || {
                                if !record_only && case.missing.is_empty() {
                                    "live".into()
                                } else {
                                    "recorded_only".into()
                                }
                            },
                            |reason| format!("unsupported ({reason})"),
                        )
                    }
                );
            }
        }
        return Ok(());
    }
    let output = env_value("GOOSE_PROVIDER_MATRIX_OUTPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/provider-compatibility-matrix")
        });
    fs::create_dir_all(&output)?;
    let current: CurrentRecording = Arc::new(Mutex::new(None));
    install_logger(MatrixLogger(current.clone()))?;
    let _subscriber = tracing::subscriber::set_default(MatrixSubscriber(current.clone()));
    let mut rows = Vec::new();
    for case in &cases {
        for &scenario in &scenarios {
            rows.push(run_row(case, scenario, &output, &current, record_only, &cache).await?);
        }
    }
    let failures: Vec<_> = rows
        .iter()
        .filter(|row| row["status"] == "failed")
        .map(|row| {
            format!(
                "{}/{}/{}",
                row["provider"].as_str().unwrap(),
                row["model"].as_str().unwrap(),
                row["scenario"].as_str().unwrap()
            )
        })
        .collect();
    let mut statuses = BTreeMap::new();
    for row in &rows {
        *statuses
            .entry(row["status"].as_str().unwrap())
            .or_insert(0usize) += 1;
    }
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&rows)?,
    )?;
    println!(
        "{} rows for {} provider/model pairs in {}: {:?}",
        rows.len(),
        cases.len(),
        output.display(),
        statuses
    );
    ensure!(
        failures.is_empty(),
        "matrix failures: {} (see summary.json)",
        failures.join(", ")
    );
    Ok(())
}

#[test]
fn filters_reject_unknown_values() {
    assert!(validate_filter("scenario", &["typo".into()], &["text"]).is_err());
    assert!(validate_filter("scenario", &["text".into()], &["text"]).is_ok());
    assert!(validate_filter("scenario", &[], &["text"]).is_ok());
}

#[test]
fn artifact_model_paths_do_not_collide() {
    let paths: HashSet<_> = ["a/b", "a_b", "a.b", "a:b", "../b"]
        .into_iter()
        .map(path_component)
        .collect();
    assert_eq!(paths.len(), 5);
    assert!(paths
        .iter()
        .all(|path| path.chars().all(|character| character.is_ascii_hexdigit())));
}

#[test]
fn response_validation_rejects_errors_and_incomplete_usage() {
    let message = Message::assistant().with_text("hello");
    let mut usage = ProviderUsage::new("model".into(), Default::default());
    assert!(validate_response(&message, &usage).is_ok());
    usage.finish_reasons = Some(vec!["length".into()]);
    assert!(validate_response(&message, &usage).is_err());
    usage.finish_reasons = Some(vec!["completed".into()]);
    usage.usage.input_tokens = Some(-1);
    assert!(validate_response(&message, &usage).is_err());
    usage.usage.input_tokens = Some(10);
    usage.usage.output_tokens = Some(5);
    usage.usage.total_tokens = Some(12);
    assert!(validate_response(&message, &usage).is_err());
    usage.usage.total_tokens = Some(15);
    assert!(validate_response(&message, &usage).is_ok());
    let user = Message::user().with_text("hello");
    assert!(validate_response(&user, &usage).is_err());
}

#[test]
fn tool_validation_checks_id_name_and_schema() {
    let arguments = json!({"value":"matrix-input"}).as_object().unwrap().clone();
    let call = CallToolRequestParams::new("matrix_echo").with_arguments(arguments);
    let valid = Message::assistant().with_tool_request("call-1", Ok(call.clone()));
    assert_eq!(validate_tool_call(&valid).unwrap(), "call-1");
    let empty_id = Message::assistant().with_tool_request("", Ok(call));
    assert!(validate_tool_call(&empty_id).is_err());
    let wrong = CallToolRequestParams::new("other");
    assert!(
        validate_tool_call(&Message::assistant().with_tool_request("call-1", Ok(wrong))).is_err()
    );
    assert!(validate_tool_call(&Message::assistant().with_text("hello")).is_err());
}

#[test]
fn default_text_leaves_effort_unspecified() {
    let case = Case::native("openai", "gpt-6-luna", &[]);
    assert_eq!(model_config(&case, Scenario::Text).thinking_effort(), None);
    for (scenario, effort) in [
        (Scenario::ThinkingOff, ThinkingEffort::Off),
        (Scenario::ThinkingLow, ThinkingEffort::Low),
        (Scenario::ThinkingMedium, ThinkingEffort::Medium),
    ] {
        assert_eq!(
            model_config(&case, scenario).thinking_effort(),
            Some(effort)
        );
    }
}

fn successful_snapshot_fixture(case: &Case, scenario: Scenario) -> Value {
    let turn = json!({"label":"fixture","method":"POST","endpoint":"https://example.invalid/v1/responses",
        "payload":{"model":case.model,"input":"hello"},"model_config":{"model_name":case.model},
        "wire_controls":{},"response":{"role":"assistant","content":"hello"},
        "usage":{},"elapsed_ms":123});
    let turns = if scenario == Scenario::ToolContinuation {
        vec![turn.clone(), turn]
    } else {
        vec![turn]
    };
    json!({"provider":case.name,"model":case.model,"scenario":scenario.name(),
        "status":"live_ok","live_attempted":true,"record_only":false,"turns":turns})
}

#[test]
fn verified_snapshots_survive_changed_requests_and_are_invalidated_by_deletion() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let cache = SnapshotCache {
        directory: directory.path().into(),
        replay_verified: false,
    };
    let case = Case::native(
        "openai",
        "snapshot-test-model",
        &["MATRIX_TEST_MISSING_KEY"],
    );
    let scenario = Scenario::Text;
    assert!(cache.verified(&case, scenario)?.is_none());
    let document = successful_snapshot_fixture(&case, scenario);
    cache.save_verified(&case, scenario, &document)?;
    let snapshot = cache.verified(&case, scenario)?.unwrap();
    assert_eq!(snapshot["status"], "verified");
    assert_eq!(
        snapshot["requests"][0]["payload"],
        document["turns"][0]["payload"]
    );
    assert!(snapshot["requests"][0].get("response").is_none());
    assert!(snapshot["requests"][0].get("usage").is_none());
    assert!(snapshot["requests"][0].get("elapsed_ms").is_none());
    let mut changed = document.clone();
    changed["turns"][0]["payload"] = json!({"input":"changed"});
    assert_eq!(cache.verified(&case, scenario)?.unwrap(), snapshot);
    assert!(cache.verified(&case, Scenario::ThinkingLow)?.is_none());
    let other = Case::native("openai", "other-model", &[]);
    assert!(cache.verified(&other, scenario)?.is_none());
    fs::remove_file(cache.path(&case, scenario))?;
    assert!(cache.verified(&case, scenario)?.is_none());
    Ok(())
}

#[test]
fn failed_or_offline_replays_preserve_verified_snapshot() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let cache = SnapshotCache {
        directory: directory.path().into(),
        replay_verified: false,
    };
    let case = Case::native("openai", "snapshot-test-model", &[]);
    let scenario = Scenario::ToolContinuation;
    let original = successful_snapshot_fixture(&case, scenario);
    cache.save_verified(&case, scenario, &original)?;
    let path = cache.path(&case, scenario);
    let bytes = fs::read(&path)?;
    for status in [
        "pending",
        "recorded_only",
        "failed",
        "unsupported",
        "verified",
    ] {
        let mut document = original.clone();
        document["status"] = json!(status);
        assert!(cache.save_verified(&case, scenario, &document).is_err());
        assert_eq!(fs::read(&path)?, bytes);
    }
    let mut incomplete = original.clone();
    incomplete["turns"].as_array_mut().unwrap().pop();
    assert!(cache.save_verified(&case, scenario, &incomplete).is_err());
    assert_eq!(fs::read(&path)?, bytes);
    let mut offline = original.clone();
    offline["record_only"] = json!(true);
    assert!(cache.save_verified(&case, scenario, &offline).is_err());
    let mut replacement = original;
    replacement["turns"][0]["payload"]["input"] = json!("replacement");
    cache.save_verified(&case, scenario, &replacement)?;
    assert_ne!(fs::read(&path)?, bytes);
    let forced = SnapshotCache {
        directory: cache.directory.clone(),
        replay_verified: true,
    };
    assert!(forced.verified(&case, scenario)?.is_none());
    Ok(())
}

#[test]
fn corrupt_snapshot_fails_instead_of_billing_a_new_request() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let cache = SnapshotCache {
        directory: directory.path().into(),
        replay_verified: false,
    };
    let case = Case::native("openai", "snapshot-test-model", &[]);
    let scenario = Scenario::Text;
    let document = successful_snapshot_fixture(&case, scenario);
    cache.save_verified(&case, scenario, &document)?;
    let path = cache.path(&case, scenario);
    fs::write(&path, b"not JSON")?;
    assert!(cache.verified(&case, scenario).is_err());
    let mut snapshot = verified_snapshot(&document)?;
    snapshot["provider"] = json!("other");
    fs::write(&path, serde_json::to_vec(&snapshot)?)?;
    assert!(cache.verified(&case, scenario).is_err());
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn cached_rows_skip_provider_construction_without_credentials() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let output = tempfile::tempdir()?;
    let cache = SnapshotCache {
        directory: directory.path().into(),
        replay_verified: false,
    };
    // No factory exists for this name: trying to construct it would fail.
    let case = Case::native("cached-fixture", "model", &["MATRIX_TEST_MISSING_KEY"]);
    let scenario = Scenario::Text;
    cache.save_verified(
        &case,
        scenario,
        &successful_snapshot_fixture(&case, scenario),
    )?;
    let current: CurrentRecording = Arc::new(Mutex::new(None));
    let row = run_row(&case, scenario, output.path(), &current, false, &cache).await?;
    assert_eq!(row["status"], "verified");
    assert_eq!(row["live_attempted"], false);
    let before = fs::read(cache.path(&case, scenario))?;
    let offline = run_row(&case, scenario, output.path(), &current, true, &cache).await?;
    assert_eq!(offline["status"], "failed"); // Record-only bypassed cache and tried the factory.
    assert_eq!(offline["live_attempted"], false);
    assert_eq!(fs::read(cache.path(&case, scenario))?, before);
    Ok(())
}
