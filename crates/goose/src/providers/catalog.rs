use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{
    base::{ConfigKey, ProviderMetadata},
    canonical::CanonicalModelRegistry,
};

const PROVIDER_METADATA_JSON: &str = include_str!("canonical/data/provider_metadata.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderMetadataEntry {
    pub id: String,
    pub display_name: String,
    pub npm: Option<String>,
    pub api: Option<String>,
    pub doc: Option<String>,
    pub env: Vec<String>,
    pub model_count: usize,
}

static PROVIDER_METADATA: Lazy<HashMap<String, ProviderMetadataEntry>> = Lazy::new(|| {
    serde_json::from_str::<Vec<ProviderMetadataEntry>>(PROVIDER_METADATA_JSON)
        .unwrap_or_else(|e| {
            eprintln!("Failed to parse provider metadata: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect()
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderFormat {
    OpenAI,
    Anthropic,
    Ollama,
}

impl ProviderFormat {
    pub fn as_str(&self) -> &str {
        match self {
            ProviderFormat::OpenAI => "openai",
            ProviderFormat::Anthropic => "anthropic",
            ProviderFormat::Ollama => "ollama",
        }
    }
}

impl std::str::FromStr for ProviderFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "openai" | "openai_compatible" => Ok(ProviderFormat::OpenAI),
            "anthropic" | "anthropic_compatible" => Ok(ProviderFormat::Anthropic),
            "ollama" | "ollama_compatible" => Ok(ProviderFormat::Ollama),
            _ => Err(format!("unknown provider format: {}", s)),
        }
    }
}

fn detect_format_from_npm(npm: &str) -> Option<ProviderFormat> {
    if npm.contains("openai") {
        Some(ProviderFormat::OpenAI)
    } else if npm.contains("anthropic") {
        Some(ProviderFormat::Anthropic)
    } else if npm.contains("ollama") {
        Some(ProviderFormat::Ollama)
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ProviderCatalogEntry {
    pub id: String,
    pub name: String,
    pub format: String,
    pub api_url: String,
    pub model_count: usize,
    pub doc_url: String,
    pub env_var: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ProviderTemplate {
    pub id: String,
    pub name: String,
    pub format: String,
    pub api_url: String,
    pub models: Vec<ModelTemplate>,
    pub supports_streaming: bool,
    pub env_var: String,
    pub doc_url: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ModelTemplate {
    pub id: String,
    pub name: String,
    pub context_limit: usize,
    pub capabilities: ModelCapabilities,
    pub deprecated: bool,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ModelCapabilities {
    pub tool_call: bool,
    pub reasoning: bool,
    pub attachment: bool,
    pub temperature: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupCategory {
    Agent,
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupMethod {
    None,
    SingleApiKey,
    ConfigFields,
    HostWithOauthFallback,
    OauthBrowser,
    OauthDeviceCode,
    CloudCredentials,
    Local,
    CliAuth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupGroup {
    Default,
    Additional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSetupField {
    pub key: String,
    pub label: String,
    pub secret: bool,
    pub required: bool,
    pub placeholder: Option<String>,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProviderSetupCapabilities {
    pub install: bool,
    pub auth: bool,
    pub auth_status: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSetupCatalogEntry {
    pub provider_id: String,
    pub display_name: String,
    pub category: ProviderSetupCategory,
    pub description: String,
    pub setup_method: ProviderSetupMethod,
    pub docs_url: Option<String>,
    pub group: ProviderSetupGroup,
    pub fields: Vec<ProviderSetupField>,
    pub aliases: Vec<String>,
    pub native_connect_query: Option<String>,
    pub binary_name: Option<String>,
    pub setup_capabilities: ProviderSetupCapabilities,
    pub show_only_when_installed: bool,
}

#[derive(Debug, Clone, Copy)]
struct CuratedSetupMetadata {
    provider_id: &'static str,
    category: ProviderSetupCategory,
    setup_method: ProviderSetupMethod,
    group: ProviderSetupGroup,
    display_name: Option<&'static str>,
    description: Option<&'static str>,
    docs_url: Option<&'static str>,
    aliases: &'static [&'static str],
    native_connect_query: Option<&'static str>,
    binary_name: Option<&'static str>,
    setup_capabilities: ProviderSetupCapabilities,
    show_only_when_installed: bool,
    synthetic: bool,
    secret_field_default: Option<CuratedFieldMetadata>,
    field_overrides: &'static [CuratedFieldMetadata],
}

#[derive(Debug, Clone, Copy)]
struct CuratedFieldMetadata {
    key: &'static str,
    label: &'static str,
    placeholder: Option<&'static str>,
    default_value: Option<&'static str>,
}

const fn setup_capabilities(
    install: bool,
    auth: bool,
    auth_status: bool,
) -> ProviderSetupCapabilities {
    ProviderSetupCapabilities {
        install,
        auth,
        auth_status,
    }
}

const API_KEY_FIELD: CuratedFieldMetadata = CuratedFieldMetadata {
    key: "",
    label: "API Key",
    placeholder: Some("Paste your API key"),
    default_value: None,
};

const SETUP_METADATA: &[CuratedSetupMetadata] = &[
    CuratedSetupMetadata {
        provider_id: "goose",
        category: ProviderSetupCategory::Agent,
        setup_method: ProviderSetupMethod::None,
        group: ProviderSetupGroup::Default,
        display_name: Some("Goose"),
        description: Some("Block's open-source coding agent"),
        docs_url: None,
        aliases: &["goose"],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: true,
        secret_field_default: None,
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "claude-acp",
        category: ProviderSetupCategory::Agent,
        setup_method: ProviderSetupMethod::CliAuth,
        group: ProviderSetupGroup::Default,
        display_name: Some("Claude Code"),
        description: Some("Anthropic's agentic coding tool"),
        docs_url: Some("https://docs.anthropic.com/en/docs/claude-code"),
        aliases: &["claude-acp", "claude_code", "claude"],
        native_connect_query: None,
        binary_name: Some("claude-agent-acp"),
        setup_capabilities: setup_capabilities(true, true, true),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "codex-acp",
        category: ProviderSetupCategory::Agent,
        setup_method: ProviderSetupMethod::CliAuth,
        group: ProviderSetupGroup::Default,
        display_name: Some("Codex"),
        description: Some("OpenAI's coding agent"),
        docs_url: Some("https://github.com/openai/codex"),
        aliases: &["codex-acp", "codex_cli", "codex"],
        native_connect_query: None,
        binary_name: Some("codex-acp"),
        setup_capabilities: setup_capabilities(true, true, true),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "copilot-acp",
        category: ProviderSetupCategory::Agent,
        setup_method: ProviderSetupMethod::CliAuth,
        group: ProviderSetupGroup::Default,
        display_name: Some("GitHub Copilot"),
        description: Some("GitHub's AI pair programmer"),
        docs_url: Some("https://docs.github.com/en/copilot/github-copilot-in-the-cli"),
        aliases: &["copilot-acp", "github_copilot", "github_copilot_cli"],
        native_connect_query: None,
        binary_name: Some("copilot"),
        setup_capabilities: setup_capabilities(true, true, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "amp-acp",
        category: ProviderSetupCategory::Agent,
        setup_method: ProviderSetupMethod::CliAuth,
        group: ProviderSetupGroup::Default,
        display_name: None,
        description: Some("Sourcegraph's coding agent"),
        docs_url: Some("https://ampcode.com"),
        aliases: &["amp-acp", "amp"],
        native_connect_query: None,
        binary_name: Some("amp-acp"),
        setup_capabilities: setup_capabilities(true, true, true),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "cursor-agent",
        category: ProviderSetupCategory::Agent,
        setup_method: ProviderSetupMethod::CliAuth,
        group: ProviderSetupGroup::Default,
        display_name: None,
        description: Some("Cursor's AI agent"),
        docs_url: Some("https://docs.cursor.com/en/cli/overview"),
        aliases: &["cursor-agent", "cursor_agent", "cursor"],
        native_connect_query: None,
        binary_name: Some("cursor-agent"),
        setup_capabilities: setup_capabilities(true, true, true),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "pi-acp",
        category: ProviderSetupCategory::Agent,
        setup_method: ProviderSetupMethod::CliAuth,
        group: ProviderSetupGroup::Default,
        display_name: None,
        description: Some("Open-source AI coding agent"),
        docs_url: Some("https://github.com/badlogic/pi-mono"),
        aliases: &["pi-acp", "pi"],
        native_connect_query: None,
        binary_name: Some("pi-acp"),
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: true,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "anthropic",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::SingleApiKey,
        group: ProviderSetupGroup::Default,
        display_name: None,
        description: Some("Claude models"),
        docs_url: Some("https://console.anthropic.com/settings/keys"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "google",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::SingleApiKey,
        group: ProviderSetupGroup::Default,
        display_name: Some("Google Gemini"),
        description: Some("Gemini models"),
        docs_url: Some("https://aistudio.google.com/apikey"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "chatgpt_codex",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::OauthDeviceCode,
        group: ProviderSetupGroup::Default,
        display_name: Some("ChatGPT"),
        description: Some("OpenAI via ChatGPT subscription"),
        docs_url: Some("https://chatgpt.com"),
        aliases: &[],
        native_connect_query: Some("ChatGPT Codex"),
        binary_name: None,
        setup_capabilities: setup_capabilities(false, true, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "openai",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::ConfigFields,
        group: ProviderSetupGroup::Default,
        display_name: None,
        description: Some("GPT and o-series models"),
        docs_url: Some("https://platform.openai.com/api-keys"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "mistral",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::SingleApiKey,
        group: ProviderSetupGroup::Default,
        display_name: None,
        description: None,
        docs_url: Some("https://console.mistral.ai/api-keys"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "ollama",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::ConfigFields,
        group: ProviderSetupGroup::Default,
        display_name: None,
        description: Some("Run local or self-hosted models"),
        docs_url: Some("https://ollama.com"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[CuratedFieldMetadata {
            key: "OLLAMA_HOST",
            label: "Host",
            placeholder: Some("localhost or http://localhost:11434"),
            default_value: Some("http://localhost:11434"),
        }],
    },
    CuratedSetupMetadata {
        provider_id: "openrouter",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::SingleApiKey,
        group: ProviderSetupGroup::Default,
        display_name: None,
        description: Some("Unified API for many models"),
        docs_url: Some("https://openrouter.ai/keys"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "databricks",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::HostWithOauthFallback,
        group: ProviderSetupGroup::Default,
        display_name: None,
        description: Some("Databricks Foundation Models"),
        docs_url: None,
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, true, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[
            CuratedFieldMetadata {
                key: "DATABRICKS_HOST",
                label: "Host URL",
                placeholder: Some("https://dbc-...cloud.databricks.com"),
                default_value: None,
            },
            CuratedFieldMetadata {
                key: "DATABRICKS_TOKEN",
                label: "Access Token",
                placeholder: Some("Paste your access token"),
                default_value: None,
            },
        ],
    },
    CuratedSetupMetadata {
        provider_id: "github_copilot",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::OauthDeviceCode,
        group: ProviderSetupGroup::Default,
        display_name: Some("GitHub Copilot Models"),
        description: Some("Models via GitHub Copilot subscription"),
        docs_url: None,
        aliases: &[],
        native_connect_query: Some("GitHub Copilot"),
        binary_name: None,
        setup_capabilities: setup_capabilities(false, true, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "custom_deepseek",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::SingleApiKey,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: Some("DeepSeek chat and reasoning models"),
        docs_url: Some("https://platform.deepseek.com/api_keys"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "zai",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::SingleApiKey,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: Some("GLM models via Z.AI"),
        docs_url: Some("https://docs.z.ai/devpack/tool/goose"),
        aliases: &["z.ai", "zhipu"],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "xai",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::SingleApiKey,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: Some("Grok models"),
        docs_url: None,
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "groq",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::SingleApiKey,
        group: ProviderSetupGroup::Additional,
        display_name: Some("Groq"),
        description: None,
        docs_url: Some("https://console.groq.com/keys"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "azure_openai",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::ConfigFields,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: None,
        docs_url: None,
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[
            CuratedFieldMetadata {
                key: "AZURE_OPENAI_ENDPOINT",
                label: "Endpoint",
                placeholder: Some("https://your-resource.openai.azure.com"),
                default_value: None,
            },
            CuratedFieldMetadata {
                key: "AZURE_OPENAI_DEPLOYMENT_NAME",
                label: "Deployment",
                placeholder: Some("gpt-4o"),
                default_value: None,
            },
            CuratedFieldMetadata {
                key: "AZURE_OPENAI_API_KEY",
                label: "API Key",
                placeholder: Some("Paste your API key"),
                default_value: None,
            },
        ],
    },
    CuratedSetupMetadata {
        provider_id: "aws_bedrock",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::CloudCredentials,
        group: ProviderSetupGroup::Additional,
        display_name: Some("AWS Bedrock"),
        description: Some("Models on AWS"),
        docs_url: None,
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[CuratedFieldMetadata {
            key: "AWS_REGION",
            label: "AWS Region",
            placeholder: Some("us-west-2"),
            default_value: None,
        }],
    },
    CuratedSetupMetadata {
        provider_id: "gcp_vertex_ai",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::CloudCredentials,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: Some("Models on Google Cloud"),
        docs_url: None,
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[
            CuratedFieldMetadata {
                key: "GCP_PROJECT_ID",
                label: "Project ID",
                placeholder: Some("my-gcp-project"),
                default_value: None,
            },
            CuratedFieldMetadata {
                key: "GCP_LOCATION",
                label: "Location",
                placeholder: Some("us-central1"),
                default_value: None,
            },
        ],
    },
    CuratedSetupMetadata {
        provider_id: "litellm",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::ConfigFields,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: Some("LiteLLM proxy gateway"),
        docs_url: None,
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[
            CuratedFieldMetadata {
                key: "LITELLM_HOST",
                label: "Host URL",
                placeholder: Some("https://your-proxy.example.com"),
                default_value: None,
            },
            CuratedFieldMetadata {
                key: "LITELLM_API_KEY",
                label: "API Key",
                placeholder: Some("Paste your API key"),
                default_value: None,
            },
        ],
    },
    CuratedSetupMetadata {
        provider_id: "lmstudio",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::ConfigFields,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: None,
        docs_url: Some("https://lmstudio.ai/docs/app/api"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[CuratedFieldMetadata {
            key: "LMSTUDIO_HOST",
            label: "Host URL",
            placeholder: Some("http://localhost:1234/v1/chat/completions"),
            default_value: None,
        }],
    },
    CuratedSetupMetadata {
        provider_id: "atomic_chat",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::ConfigFields,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: None,
        docs_url: Some("https://github.com/AtomicBot-ai/Atomic-Chat?tab=readme-ov-file#readme"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[CuratedFieldMetadata {
            key: "ATOMIC_CHAT_HOST",
            label: "Host URL",
            placeholder: Some("http://localhost:1337"),
            default_value: Some("http://localhost:1337"),
        }],
    },
    CuratedSetupMetadata {
        provider_id: "nvidia",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::SingleApiKey,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: None,
        docs_url: Some("https://build.nvidia.com/models"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "cerebras",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::SingleApiKey,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: None,
        docs_url: Some("https://cloud.cerebras.ai/platform"),
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: Some(API_KEY_FIELD),
        field_overrides: &[],
    },
    CuratedSetupMetadata {
        provider_id: "snowflake",
        category: ProviderSetupCategory::Model,
        setup_method: ProviderSetupMethod::ConfigFields,
        group: ProviderSetupGroup::Additional,
        display_name: None,
        description: Some("Snowflake Cortex"),
        docs_url: None,
        aliases: &[],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: setup_capabilities(false, false, false),
        show_only_when_installed: false,
        synthetic: false,
        secret_field_default: None,
        field_overrides: &[
            CuratedFieldMetadata {
                key: "SNOWFLAKE_HOST",
                label: "Host URL",
                placeholder: Some("https://your-account.snowflakecomputing.com"),
                default_value: None,
            },
            CuratedFieldMetadata {
                key: "SNOWFLAKE_TOKEN",
                label: "Access Token",
                placeholder: Some("Paste your access token"),
                default_value: None,
            },
        ],
    },
];

fn field_label(key: &str) -> String {
    let label = key
        .strip_prefix("GOOSE_")
        .unwrap_or(key)
        .replace('_', " ")
        .to_lowercase();
    label
        .split_whitespace()
        .map(|word| {
            if matches!(
                word,
                "api" | "url" | "id" | "openai" | "aws" | "gcp" | "llm" | "oauth"
            ) {
                word.to_uppercase()
            } else {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn field_override<'a>(
    key: &str,
    config_key: &ConfigKey,
    curated: &'a CuratedSetupMetadata,
) -> Option<&'a CuratedFieldMetadata> {
    if let Some(field) = curated
        .field_overrides
        .iter()
        .find(|field| field.key == key)
    {
        return Some(field);
    }

    if config_key.secret {
        return curated.secret_field_default.as_ref();
    }

    None
}

fn setup_field(config_key: &ConfigKey, curated: &CuratedSetupMetadata) -> ProviderSetupField {
    let field_override = field_override(&config_key.name, config_key, curated);
    ProviderSetupField {
        key: config_key.name.clone(),
        label: field_override
            .map(|field| field.label.to_string())
            .unwrap_or_else(|| field_label(&config_key.name)),
        secret: config_key.secret,
        required: config_key.required,
        placeholder: field_override.and_then(|field| field.placeholder.map(str::to_string)),
        default_value: field_override
            .and_then(|field| field.default_value.map(str::to_string))
            .or_else(|| config_key.default.clone()),
    }
}

fn setup_entry_from_metadata(
    curated: &CuratedSetupMetadata,
    metadata: &ProviderMetadata,
) -> ProviderSetupCatalogEntry {
    ProviderSetupCatalogEntry {
        provider_id: curated.provider_id.to_string(),
        display_name: curated
            .display_name
            .unwrap_or(metadata.display_name.as_str())
            .to_string(),
        category: curated.category,
        description: curated
            .description
            .unwrap_or(metadata.description.as_str())
            .to_string(),
        setup_method: curated.setup_method,
        docs_url: curated.docs_url.map(str::to_string).or_else(|| {
            (!metadata.model_doc_link.is_empty()).then(|| metadata.model_doc_link.clone())
        }),
        group: curated.group,
        fields: metadata
            .config_keys
            .iter()
            .filter(|key| key.primary)
            .map(|key| setup_field(key, curated))
            .collect(),
        aliases: curated
            .aliases
            .iter()
            .map(|alias| alias.to_string())
            .collect(),
        native_connect_query: curated.native_connect_query.map(str::to_string),
        binary_name: curated.binary_name.map(str::to_string),
        setup_capabilities: curated.setup_capabilities,
        show_only_when_installed: curated.show_only_when_installed,
    }
}

fn synthetic_goose_setup_entry(curated: &CuratedSetupMetadata) -> ProviderSetupCatalogEntry {
    ProviderSetupCatalogEntry {
        provider_id: curated.provider_id.to_string(),
        display_name: curated.display_name.unwrap_or("Goose").to_string(),
        category: ProviderSetupCategory::Agent,
        description: curated.description.unwrap_or_default().to_string(),
        setup_method: ProviderSetupMethod::None,
        docs_url: curated.docs_url.map(str::to_string),
        group: curated.group,
        fields: Vec::new(),
        aliases: curated
            .aliases
            .iter()
            .map(|alias| alias.to_string())
            .collect(),
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: curated.setup_capabilities,
        show_only_when_installed: false,
    }
}

pub async fn get_providers_by_format(format: ProviderFormat) -> Vec<ProviderCatalogEntry> {
    let native_provider_ids = super::init::providers()
        .await
        .into_iter()
        .map(|(metadata, _)| metadata.name)
        .collect::<std::collections::HashSet<_>>();

    let mut entries: Vec<ProviderCatalogEntry> = PROVIDER_METADATA
        .values()
        .filter_map(|metadata| {
            if native_provider_ids.contains(&metadata.id) {
                return None;
            }

            let npm = metadata.npm.as_ref()?;
            let detected_format = detect_format_from_npm(npm)?;

            if detected_format != format {
                return None;
            }

            let api_url = metadata.api.as_ref()?.clone();

            let env_var = metadata.env.first().cloned().unwrap_or_else(|| {
                format!("{}_API_KEY", metadata.id.to_uppercase().replace('-', "_"))
            });

            Some(ProviderCatalogEntry {
                id: metadata.id.clone(),
                name: metadata.display_name.clone(),
                format: detected_format.as_str().to_string(),
                api_url,
                model_count: metadata.model_count,
                doc_url: metadata.doc.clone().unwrap_or_default(),
                env_var,
            })
        })
        .collect();

    // Sort by name
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

pub async fn get_setup_catalog_entries() -> Vec<ProviderSetupCatalogEntry> {
    let registry_metadata = super::providers()
        .await
        .into_iter()
        .map(|(metadata, _)| (metadata.name.clone(), metadata))
        .collect::<HashMap<_, _>>();

    SETUP_METADATA
        .iter()
        .filter_map(|curated| {
            if curated.synthetic {
                return Some(synthetic_goose_setup_entry(curated));
            }

            registry_metadata
                .get(curated.provider_id)
                .map(|metadata| setup_entry_from_metadata(curated, metadata))
        })
        .collect()
}

pub fn get_provider_setup_category(provider_id: &str) -> Option<ProviderSetupCategory> {
    SETUP_METADATA
        .iter()
        .find(|curated| curated.provider_id == provider_id)
        .map(|curated| curated.category)
}

pub fn get_provider_template(provider_id: &str) -> Option<ProviderTemplate> {
    let metadata = PROVIDER_METADATA.get(provider_id)?;

    let npm = metadata.npm.as_ref()?;
    let format = detect_format_from_npm(npm)?;

    let api_url = metadata.api.as_ref()?.clone();

    let models: Vec<ModelTemplate> = CanonicalModelRegistry::bundled()
        .ok()
        .map(|registry| {
            registry
                .get_all_models_for_provider(provider_id)
                .into_iter()
                .map(|model| {
                    // Extract just the model ID (without provider prefix)
                    let model_id = model
                        .id
                        .strip_prefix(&format!("{}/", provider_id))
                        .unwrap_or(&model.id)
                        .to_string();

                    ModelTemplate {
                        id: model_id,
                        name: model.name.clone(),
                        context_limit: model.limit.context,
                        capabilities: ModelCapabilities {
                            tool_call: model.tool_call,
                            reasoning: model.reasoning.unwrap_or(false),
                            attachment: model.attachment.unwrap_or(false),
                            temperature: model.temperature.unwrap_or(false),
                        },
                        deprecated: false, // Canonical models don't have deprecated flag
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let env_var = metadata
        .env
        .first()
        .cloned()
        .unwrap_or_else(|| format!("{}_API_KEY", provider_id.to_uppercase().replace('-', "_")));

    Some(ProviderTemplate {
        id: metadata.id.clone(),
        name: metadata.display_name.clone(),
        format: format.as_str().to_string(),
        api_url,
        models,
        supports_streaming: true, // Default to true
        env_var,
        doc_url: metadata.doc.clone().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::ProviderType;

    #[tokio::test]
    async fn test_zai_provider() {
        let zai = crate::providers::get_from_registry("zai")
            .await
            .expect("z.ai should be registered as a declarative provider");
        assert_eq!(zai.provider_type(), ProviderType::Declarative);

        let metadata = zai.metadata();
        assert_eq!(metadata.display_name, "Z.AI");
        assert!(
            !metadata.known_models.is_empty(),
            "z.ai should have known models"
        );
        assert!(
            metadata
                .config_keys
                .iter()
                .any(|key| key.name == "ZHIPU_API_KEY"),
            "z.ai should expose its API key config"
        );

        let setup_entries = get_setup_catalog_entries().await;
        let setup_entry = setup_entries
            .iter()
            .find(|entry| entry.provider_id == "zai")
            .expect("z.ai should be in the setup catalog");
        assert_eq!(setup_entry.setup_method, ProviderSetupMethod::SingleApiKey);

        let template = get_provider_template("zai");
        assert!(template.is_some(), "z.ai should have a template");

        let template = template.unwrap();
        println!("Z.AI template: {} models", template.models.len());
        for model in template.models.iter().take(3) {
            println!(
                "  - {} ({}K context)",
                model.name,
                model.context_limit / 1000
            );
        }
        assert!(
            !template.models.is_empty(),
            "z.ai template should have models"
        );
    }

    #[tokio::test]
    async fn setup_catalog_includes_goose_and_curated_fields() {
        let entries = get_setup_catalog_entries().await;

        let goose = entries
            .iter()
            .find(|entry| entry.provider_id == "goose")
            .expect("setup catalog should include synthetic goose");
        assert_eq!(goose.category, ProviderSetupCategory::Agent);
        assert_eq!(goose.setup_method, ProviderSetupMethod::None);
        assert!(goose.fields.is_empty());

        let ollama = entries
            .iter()
            .find(|entry| entry.provider_id == "ollama")
            .expect("setup catalog should include ollama");
        assert_eq!(ollama.setup_method, ProviderSetupMethod::ConfigFields);
        assert_eq!(ollama.fields.len(), 1);
        assert_eq!(ollama.fields[0].key, "OLLAMA_HOST");
        assert_eq!(ollama.fields[0].label, "Host");
        assert_eq!(
            ollama.fields[0].default_value.as_deref(),
            Some("http://localhost:11434")
        );

        let databricks = entries
            .iter()
            .find(|entry| entry.provider_id == "databricks")
            .expect("setup catalog should include databricks");
        assert_eq!(
            databricks.setup_method,
            ProviderSetupMethod::HostWithOauthFallback
        );
        assert_eq!(
            databricks
                .fields
                .iter()
                .map(|field| field.key.as_str())
                .collect::<Vec<_>>(),
            ["DATABRICKS_HOST", "DATABRICKS_TOKEN"]
        );

        let atomic_chat = entries
            .iter()
            .find(|entry| entry.provider_id == "atomic_chat")
            .expect("setup catalog should include atomic_chat declarative provider");
        assert_eq!(atomic_chat.setup_method, ProviderSetupMethod::ConfigFields);
        let host_field = atomic_chat
            .fields
            .iter()
            .find(|field| field.key == "ATOMIC_CHAT_HOST")
            .expect("atomic_chat should expose ATOMIC_CHAT_HOST");
        assert_eq!(host_field.label, "Host URL");
        assert_eq!(
            host_field.default_value.as_deref(),
            Some("http://localhost:1337")
        );
    }

    #[tokio::test]
    async fn setup_catalog_excludes_uncurated_deprecated_providers() {
        let provider_ids = get_setup_catalog_entries()
            .await
            .into_iter()
            .map(|entry| entry.provider_id)
            .collect::<std::collections::HashSet<_>>();

        assert!(provider_ids.contains("claude-acp"));
        assert!(provider_ids.contains("codex-acp"));
        assert!(provider_ids.contains("atomic_chat"));
        assert!(!provider_ids.contains("claude_code"));
        assert!(!provider_ids.contains("codex"));
        assert!(!provider_ids.contains("gemini_cli"));
    }
}
