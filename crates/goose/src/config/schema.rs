use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::base::{Config, ConfigError};
use crate::config::extensions::ExtensionEntry;
use crate::config::goose_mode::GooseMode;
use crate::slash_commands::SlashCommandMapping;

/// JSON Schema representation of Goose's config.yaml.
///
/// All keys are optional. Unknown keys are allowed (additionalProperties: true)
/// because Goose passes undocumented provider-specific keys through as
/// environment variable overrides.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct GooseConfigSchema {
    // === Core Goose Settings ===
    #[serde(rename = "GOOSE_PROVIDER")]
    pub goose_provider: Option<String>,
    #[serde(rename = "GOOSE_MODEL")]
    pub goose_model: Option<String>,
    #[serde(rename = "GOOSE_MODE")]
    pub goose_mode: Option<GooseMode>,
    #[serde(rename = "GOOSE_MAX_TOKENS")]
    pub goose_max_tokens: Option<i32>,
    #[serde(rename = "GOOSE_CONTEXT_LIMIT")]
    pub goose_context_limit: Option<u64>,
    #[serde(rename = "GOOSE_INPUT_LIMIT")]
    pub goose_input_limit: Option<u64>,
    #[serde(rename = "GOOSE_MAX_TURNS")]
    pub goose_max_turns: Option<u32>,
    #[serde(rename = "GOOSE_MAX_ACTIVE_AGENTS")]
    pub goose_max_active_agents: Option<u64>,
    #[serde(rename = "GOOSE_AUTO_COMPACT_THRESHOLD")]
    pub goose_auto_compact_threshold: Option<f64>,
    #[serde(rename = "GOOSE_TOOL_PAIR_SUMMARIZATION")]
    pub goose_tool_pair_summarization: Option<bool>,
    #[serde(rename = "GOOSE_TOOL_CALL_CUTOFF")]
    pub goose_tool_call_cutoff: Option<u64>,
    #[serde(rename = "GOOSE_STREAM_TIMEOUT")]
    pub goose_stream_timeout: Option<u64>,
    #[serde(rename = "GOOSE_SEARCH_PATHS")]
    pub goose_search_paths: Option<Vec<String>>,
    #[serde(rename = "GOOSE_DISABLE_SESSION_NAMING")]
    pub goose_disable_session_naming: Option<bool>,
    #[serde(rename = "GOOSE_DISABLE_KEYRING")]
    pub goose_disable_keyring: Option<bool>,
    #[serde(rename = "GOOSE_TELEMETRY_ENABLED")]
    pub goose_telemetry_enabled: Option<bool>,
    #[serde(rename = "GOOSE_DEFAULT_EXTENSION_TIMEOUT")]
    pub goose_default_extension_timeout: Option<u64>,
    #[serde(rename = "GOOSE_PROMPT_EDITOR")]
    pub goose_prompt_editor: Option<String>,
    #[serde(rename = "GOOSE_PROMPT_EDITOR_ALWAYS")]
    pub goose_prompt_editor_always: Option<bool>,
    #[serde(rename = "GOOSE_ALLOWLIST")]
    pub goose_allowlist: Option<String>,
    #[serde(rename = "GOOSE_SYSTEM_PROMPT_FILE_PATH")]
    pub goose_system_prompt_file_path: Option<String>,
    #[serde(rename = "GOOSE_DEBUG")]
    pub goose_debug: Option<bool>,
    #[serde(rename = "GOOSE_SHOW_FULL_OUTPUT")]
    pub goose_show_full_output: Option<bool>,
    #[serde(rename = "GOOSE_STATUS_HOOK")]
    pub goose_status_hook: Option<String>,
    #[serde(rename = "GOOSE_LOCAL_ENABLE_THINKING")]
    pub goose_local_enable_thinking: Option<bool>,
    #[serde(rename = "GOOSE_DATABRICKS_CLIENT_REQUEST_ID")]
    pub goose_databricks_client_request_id: Option<bool>,
    #[serde(rename = "CONTEXT_FILE_NAMES")]
    pub context_file_names: Option<Vec<String>>,
    #[serde(rename = "EDIT_MODE")]
    pub edit_mode: Option<String>,
    #[serde(rename = "RANDOM_THINKING_MESSAGES")]
    pub random_thinking_messages: Option<bool>,
    #[serde(rename = "CODE_MODE_TOOL_DISCLOSURE")]
    pub code_mode_tool_disclosure: Option<String>,

    // === mTLS Settings ===
    #[serde(rename = "GOOSE_CLIENT_CERT_PATH")]
    pub goose_client_cert_path: Option<String>,
    #[serde(rename = "GOOSE_CLIENT_KEY_PATH")]
    pub goose_client_key_path: Option<String>,
    #[serde(rename = "GOOSE_CA_CERT_PATH")]
    pub goose_ca_cert_path: Option<String>,

    // === Planner & Subagent Settings ===
    #[serde(rename = "GOOSE_PLANNER_PROVIDER")]
    pub goose_planner_provider: Option<String>,
    #[serde(rename = "GOOSE_PLANNER_MODEL")]
    pub goose_planner_model: Option<String>,
    #[serde(rename = "GOOSE_SUBAGENT_PROVIDER")]
    pub goose_subagent_provider: Option<String>,
    #[serde(rename = "GOOSE_SUBAGENT_MODEL")]
    pub goose_subagent_model: Option<String>,
    #[serde(rename = "GOOSE_SUBAGENT_MAX_TURNS")]
    pub goose_subagent_max_turns: Option<u64>,
    #[serde(rename = "GOOSE_MAX_BACKGROUND_TASKS")]
    pub goose_max_background_tasks: Option<u64>,

    // === Recipe Settings ===
    #[serde(rename = "GOOSE_RECIPE_GITHUB_REPO")]
    pub goose_recipe_github_repo: Option<String>,
    #[serde(rename = "GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS")]
    pub goose_recipe_retry_timeout_seconds: Option<u64>,
    #[serde(rename = "GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS")]
    pub goose_recipe_on_failure_timeout_seconds: Option<u64>,

    // === CLI Settings ===
    #[serde(rename = "GOOSE_CLI_MIN_PRIORITY")]
    pub goose_cli_min_priority: Option<f32>,
    #[serde(rename = "GOOSE_CLI_THEME")]
    pub goose_cli_theme: Option<String>,
    #[serde(rename = "GOOSE_CLI_LIGHT_THEME")]
    pub goose_cli_light_theme: Option<String>,
    #[serde(rename = "GOOSE_CLI_DARK_THEME")]
    pub goose_cli_dark_theme: Option<String>,
    #[serde(rename = "GOOSE_CLI_SHOW_COST")]
    pub goose_cli_show_cost: Option<bool>,
    #[serde(rename = "GOOSE_CLI_SHOW_THINKING")]
    pub goose_cli_show_thinking: Option<bool>,
    #[serde(rename = "GOOSE_CLI_NEWLINE_KEY")]
    pub goose_cli_newline_key: Option<String>,

    // === AI Agent / Thinking Settings ===
    #[serde(rename = "CLAUDE_CODE_COMMAND")]
    pub claude_code_command: Option<String>,
    #[serde(rename = "GEMINI_CLI_COMMAND")]
    pub gemini_cli_command: Option<String>,
    #[serde(rename = "CURSOR_AGENT_COMMAND")]
    pub cursor_agent_command: Option<String>,
    #[serde(rename = "CODEX_COMMAND")]
    pub codex_command: Option<String>,
    #[serde(rename = "CODEX_REASONING_EFFORT")]
    pub codex_reasoning_effort: Option<String>,
    #[serde(rename = "CODEX_ENABLE_SKILLS")]
    pub codex_enable_skills: Option<String>,
    #[serde(rename = "CODEX_SKIP_GIT_CHECK")]
    pub codex_skip_git_check: Option<String>,
    #[serde(rename = "CHATGPT_CODEX_REASONING_EFFORT")]
    pub chatgpt_codex_reasoning_effort: Option<String>,
    #[serde(rename = "CLAUDE_THINKING_TYPE")]
    pub claude_thinking_type: Option<String>,
    #[serde(rename = "CLAUDE_THINKING_EFFORT")]
    pub claude_thinking_effort: Option<String>,
    #[serde(rename = "CLAUDE_THINKING_BUDGET")]
    pub claude_thinking_budget: Option<i32>,
    #[serde(rename = "GEMINI3_THINKING_LEVEL")]
    pub gemini3_thinking_level: Option<String>,
    #[serde(rename = "GEMINI25_THINKING_BUDGET")]
    pub gemini25_thinking_budget: Option<i32>,

    // === Security Settings ===
    #[serde(rename = "SECURITY_PROMPT_ENABLED")]
    pub security_prompt_enabled: Option<bool>,
    #[serde(rename = "SECURITY_PROMPT_THRESHOLD")]
    pub security_prompt_threshold: Option<f64>,
    #[serde(rename = "SECURITY_PROMPT_CLASSIFIER_ENABLED")]
    pub security_prompt_classifier_enabled: Option<bool>,
    #[serde(rename = "SECURITY_PROMPT_CLASSIFIER_MODEL")]
    pub security_prompt_classifier_model: Option<String>,
    #[serde(rename = "SECURITY_PROMPT_CLASSIFIER_ENDPOINT")]
    pub security_prompt_classifier_endpoint: Option<String>,
    #[serde(rename = "SECURITY_COMMAND_CLASSIFIER_ENABLED")]
    pub security_command_classifier_enabled: Option<bool>,

    // === Provider Settings ===
    #[serde(rename = "OPENAI_HOST")]
    pub openai_host: Option<String>,
    #[serde(rename = "OPENAI_BASE_URL")]
    pub openai_base_url: Option<String>,
    #[serde(rename = "OPENAI_BASE_PATH")]
    pub openai_base_path: Option<String>,
    #[serde(rename = "OPENAI_ORGANIZATION")]
    pub openai_organization: Option<String>,
    #[serde(rename = "OPENAI_PROJECT")]
    pub openai_project: Option<String>,
    #[serde(rename = "OPENAI_TIMEOUT")]
    pub openai_timeout: Option<u64>,
    #[serde(rename = "ANTHROPIC_HOST")]
    pub anthropic_host: Option<String>,
    #[serde(rename = "OLLAMA_HOST")]
    pub ollama_host: Option<String>,
    #[serde(rename = "OLLAMA_TIMEOUT")]
    pub ollama_timeout: Option<u64>,
    #[serde(rename = "OLLAMA_STREAM_TIMEOUT")]
    pub ollama_stream_timeout: Option<u64>,
    #[serde(rename = "OLLAMA_STREAM_USAGE")]
    pub ollama_stream_usage: Option<bool>,
    #[serde(rename = "DATABRICKS_HOST")]
    pub databricks_host: Option<String>,
    #[serde(rename = "DATABRICKS_MAX_RETRIES")]
    pub databricks_max_retries: Option<String>,
    #[serde(rename = "DATABRICKS_INITIAL_RETRY_INTERVAL_MS")]
    pub databricks_initial_retry_interval_ms: Option<String>,
    #[serde(rename = "DATABRICKS_BACKOFF_MULTIPLIER")]
    pub databricks_backoff_multiplier: Option<String>,
    #[serde(rename = "DATABRICKS_MAX_RETRY_INTERVAL_MS")]
    pub databricks_max_retry_interval_ms: Option<String>,
    #[serde(rename = "AZURE_OPENAI_ENDPOINT")]
    pub azure_openai_endpoint: Option<String>,
    #[serde(rename = "AZURE_OPENAI_DEPLOYMENT_NAME")]
    pub azure_openai_deployment_name: Option<String>,
    #[serde(rename = "AZURE_OPENAI_API_VERSION")]
    pub azure_openai_api_version: Option<String>,
    #[serde(rename = "GOOGLE_HOST")]
    pub google_host: Option<String>,
    #[serde(rename = "GCP_PROJECT_ID")]
    pub gcp_project_id: Option<String>,
    #[serde(rename = "GCP_LOCATION")]
    pub gcp_location: Option<String>,
    #[serde(rename = "GCP_MAX_RETRIES")]
    pub gcp_max_retries: Option<String>,
    #[serde(rename = "GCP_INITIAL_RETRY_INTERVAL_MS")]
    pub gcp_initial_retry_interval_ms: Option<String>,
    #[serde(rename = "GCP_BACKOFF_MULTIPLIER")]
    pub gcp_backoff_multiplier: Option<String>,
    #[serde(rename = "GCP_MAX_RETRY_INTERVAL_MS")]
    pub gcp_max_retry_interval_ms: Option<String>,
    #[serde(rename = "AWS_REGION")]
    pub aws_region: Option<String>,
    #[serde(rename = "AWS_PROFILE")]
    pub aws_profile: Option<String>,
    #[serde(rename = "BEDROCK_MAX_RETRIES")]
    pub bedrock_max_retries: Option<u64>,
    #[serde(rename = "BEDROCK_INITIAL_RETRY_INTERVAL_MS")]
    pub bedrock_initial_retry_interval_ms: Option<u64>,
    #[serde(rename = "BEDROCK_BACKOFF_MULTIPLIER")]
    pub bedrock_backoff_multiplier: Option<f64>,
    #[serde(rename = "BEDROCK_MAX_RETRY_INTERVAL_MS")]
    pub bedrock_max_retry_interval_ms: Option<u64>,
    #[serde(rename = "BEDROCK_ENABLE_CACHING")]
    pub bedrock_enable_caching: Option<bool>,
    #[serde(rename = "SAGEMAKER_ENDPOINT_NAME")]
    pub sagemaker_endpoint_name: Option<String>,
    #[serde(rename = "LITELLM_HOST")]
    pub litellm_host: Option<String>,
    #[serde(rename = "LITELLM_BASE_PATH")]
    pub litellm_base_path: Option<String>,
    #[serde(rename = "LITELLM_TIMEOUT")]
    pub litellm_timeout: Option<u64>,
    #[serde(rename = "SNOWFLAKE_HOST")]
    pub snowflake_host: Option<String>,
    #[serde(rename = "GITHUB_COPILOT_HOST")]
    pub github_copilot_host: Option<String>,
    #[serde(rename = "GITHUB_COPILOT_CLIENT_ID")]
    pub github_copilot_client_id: Option<String>,
    #[serde(rename = "GITHUB_COPILOT_TOKEN_URL")]
    pub github_copilot_token_url: Option<String>,
    #[serde(rename = "XAI_HOST")]
    pub xai_host: Option<String>,
    #[serde(rename = "OPENROUTER_HOST")]
    pub openrouter_host: Option<String>,
    #[serde(rename = "VENICE_HOST")]
    pub venice_host: Option<String>,
    #[serde(rename = "VENICE_BASE_PATH")]
    pub venice_base_path: Option<String>,
    #[serde(rename = "VENICE_MODELS_PATH")]
    pub venice_models_path: Option<String>,
    #[serde(rename = "TETRATE_HOST")]
    pub tetrate_host: Option<String>,
    #[serde(rename = "AVIAN_HOST")]
    pub avian_host: Option<String>,

    // === Observability Settings (lowercase keys) ===
    pub otel_exporter_otlp_endpoint: Option<String>,
    pub otel_exporter_otlp_timeout: Option<u64>,

    // === Tunnel Settings (lowercase keys) ===
    pub tunnel_auto_start: Option<bool>,

    // Category B: not in ALL_KEYS — these use dedicated module helpers, not config_value! macro
    pub extensions: Option<HashMap<String, ExtensionEntry>>,
    pub slash_commands: Option<Vec<SlashCommandMapping>>,
    pub experiments: Option<HashMap<String, bool>>,
}

impl GooseConfigSchema {
    /// All user-facing config keys that get `config_value!` typed accessors.
    /// Category B keys (extensions, slash_commands, experiments) are in the struct
    /// for schema generation but NOT here — they use dedicated module helpers.
    pub const ALL_KEYS: &[&str] = &[
        // Core Goose Settings
        "GOOSE_PROVIDER",
        "GOOSE_MODEL",
        "GOOSE_MODE",
        "GOOSE_MAX_TOKENS",
        "GOOSE_CONTEXT_LIMIT",
        "GOOSE_INPUT_LIMIT",
        "GOOSE_MAX_TURNS",
        "GOOSE_MAX_ACTIVE_AGENTS",
        "GOOSE_AUTO_COMPACT_THRESHOLD",
        "GOOSE_TOOL_PAIR_SUMMARIZATION",
        "GOOSE_TOOL_CALL_CUTOFF",
        "GOOSE_STREAM_TIMEOUT",
        "GOOSE_SEARCH_PATHS",
        "GOOSE_DISABLE_SESSION_NAMING",
        "GOOSE_DISABLE_KEYRING",
        "GOOSE_TELEMETRY_ENABLED",
        "GOOSE_DEFAULT_EXTENSION_TIMEOUT",
        "GOOSE_PROMPT_EDITOR",
        "GOOSE_PROMPT_EDITOR_ALWAYS",
        "GOOSE_ALLOWLIST",
        "GOOSE_SYSTEM_PROMPT_FILE_PATH",
        "GOOSE_DEBUG",
        "GOOSE_SHOW_FULL_OUTPUT",
        "GOOSE_STATUS_HOOK",
        "GOOSE_LOCAL_ENABLE_THINKING",
        "GOOSE_DATABRICKS_CLIENT_REQUEST_ID",
        "CONTEXT_FILE_NAMES",
        "EDIT_MODE",
        "RANDOM_THINKING_MESSAGES",
        "CODE_MODE_TOOL_DISCLOSURE",
        // mTLS Settings
        "GOOSE_CLIENT_CERT_PATH",
        "GOOSE_CLIENT_KEY_PATH",
        "GOOSE_CA_CERT_PATH",
        // Planner & Subagent Settings
        "GOOSE_PLANNER_PROVIDER",
        "GOOSE_PLANNER_MODEL",
        "GOOSE_SUBAGENT_PROVIDER",
        "GOOSE_SUBAGENT_MODEL",
        "GOOSE_SUBAGENT_MAX_TURNS",
        "GOOSE_MAX_BACKGROUND_TASKS",
        // Recipe Settings
        "GOOSE_RECIPE_GITHUB_REPO",
        "GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS",
        "GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS",
        // CLI Settings
        "GOOSE_CLI_MIN_PRIORITY",
        "GOOSE_CLI_THEME",
        "GOOSE_CLI_LIGHT_THEME",
        "GOOSE_CLI_DARK_THEME",
        "GOOSE_CLI_SHOW_COST",
        "GOOSE_CLI_SHOW_THINKING",
        "GOOSE_CLI_NEWLINE_KEY",
        // AI Agent / Thinking Settings
        "CLAUDE_CODE_COMMAND",
        "GEMINI_CLI_COMMAND",
        "CURSOR_AGENT_COMMAND",
        "CODEX_COMMAND",
        "CODEX_REASONING_EFFORT",
        "CODEX_ENABLE_SKILLS",
        "CODEX_SKIP_GIT_CHECK",
        "CHATGPT_CODEX_REASONING_EFFORT",
        "CLAUDE_THINKING_TYPE",
        "CLAUDE_THINKING_EFFORT",
        "CLAUDE_THINKING_BUDGET",
        "GEMINI3_THINKING_LEVEL",
        "GEMINI25_THINKING_BUDGET",
        // Security Settings
        "SECURITY_PROMPT_ENABLED",
        "SECURITY_PROMPT_THRESHOLD",
        "SECURITY_PROMPT_CLASSIFIER_ENABLED",
        "SECURITY_PROMPT_CLASSIFIER_MODEL",
        "SECURITY_PROMPT_CLASSIFIER_ENDPOINT",
        "SECURITY_COMMAND_CLASSIFIER_ENABLED",
        // Provider Settings
        "OPENAI_HOST",
        "OPENAI_BASE_URL",
        "OPENAI_BASE_PATH",
        "OPENAI_ORGANIZATION",
        "OPENAI_PROJECT",
        "OPENAI_TIMEOUT",
        "ANTHROPIC_HOST",
        "OLLAMA_HOST",
        "OLLAMA_TIMEOUT",
        "OLLAMA_STREAM_TIMEOUT",
        "OLLAMA_STREAM_USAGE",
        "DATABRICKS_HOST",
        "DATABRICKS_MAX_RETRIES",
        "DATABRICKS_INITIAL_RETRY_INTERVAL_MS",
        "DATABRICKS_BACKOFF_MULTIPLIER",
        "DATABRICKS_MAX_RETRY_INTERVAL_MS",
        "AZURE_OPENAI_ENDPOINT",
        "AZURE_OPENAI_DEPLOYMENT_NAME",
        "AZURE_OPENAI_API_VERSION",
        "GOOGLE_HOST",
        "GCP_PROJECT_ID",
        "GCP_LOCATION",
        "GCP_MAX_RETRIES",
        "GCP_INITIAL_RETRY_INTERVAL_MS",
        "GCP_BACKOFF_MULTIPLIER",
        "GCP_MAX_RETRY_INTERVAL_MS",
        "AWS_REGION",
        "AWS_PROFILE",
        "BEDROCK_MAX_RETRIES",
        "BEDROCK_INITIAL_RETRY_INTERVAL_MS",
        "BEDROCK_BACKOFF_MULTIPLIER",
        "BEDROCK_MAX_RETRY_INTERVAL_MS",
        "BEDROCK_ENABLE_CACHING",
        "SAGEMAKER_ENDPOINT_NAME",
        "LITELLM_HOST",
        "LITELLM_BASE_PATH",
        "LITELLM_TIMEOUT",
        "SNOWFLAKE_HOST",
        "GITHUB_COPILOT_HOST",
        "GITHUB_COPILOT_CLIENT_ID",
        "GITHUB_COPILOT_TOKEN_URL",
        "XAI_HOST",
        "OPENROUTER_HOST",
        "VENICE_HOST",
        "VENICE_BASE_PATH",
        "VENICE_MODELS_PATH",
        "TETRATE_HOST",
        "AVIAN_HOST",
        // Observability Settings
        "otel_exporter_otlp_endpoint",
        "otel_exporter_otlp_timeout",
        // Tunnel Settings
        "tunnel_auto_start",
    ];

    // const fn cannot use == on &str in stable Rust; manual byte comparison required
    pub const fn has_key(key: &str) -> bool {
        let key_bytes = key.as_bytes();
        let mut i = 0;
        while i < Self::ALL_KEYS.len() {
            let candidate = Self::ALL_KEYS[i].as_bytes();
            if candidate.len() == key_bytes.len() {
                let mut j = 0;
                let mut eq = true;
                while j < key_bytes.len() {
                    if candidate[j] != key_bytes[j] {
                        eq = false;
                        break;
                    }
                    j += 1;
                }
                if eq {
                    return true;
                }
            }
            i += 1;
        }
        false
    }

    pub fn from_config(config: &Config) -> Self {
        GooseConfigSchema {
            goose_provider: config.get_param("GOOSE_PROVIDER").ok(),
            goose_model: config.get_param("GOOSE_MODEL").ok(),
            goose_mode: config.get_param("GOOSE_MODE").ok(),
            goose_max_tokens: config.get_param("GOOSE_MAX_TOKENS").ok(),
            goose_context_limit: config.get_param("GOOSE_CONTEXT_LIMIT").ok(),
            goose_input_limit: config.get_param("GOOSE_INPUT_LIMIT").ok(),
            goose_max_turns: config.get_param("GOOSE_MAX_TURNS").ok(),
            goose_max_active_agents: config.get_param("GOOSE_MAX_ACTIVE_AGENTS").ok(),
            goose_auto_compact_threshold: config.get_param("GOOSE_AUTO_COMPACT_THRESHOLD").ok(),
            goose_tool_pair_summarization: config.get_param("GOOSE_TOOL_PAIR_SUMMARIZATION").ok(),
            goose_tool_call_cutoff: config.get_param("GOOSE_TOOL_CALL_CUTOFF").ok(),
            goose_stream_timeout: config.get_param("GOOSE_STREAM_TIMEOUT").ok(),
            goose_search_paths: config.get_param("GOOSE_SEARCH_PATHS").ok(),
            goose_disable_session_naming: config.get_param("GOOSE_DISABLE_SESSION_NAMING").ok(),
            goose_disable_keyring: config.get_param("GOOSE_DISABLE_KEYRING").ok(),
            goose_telemetry_enabled: config.get_param("GOOSE_TELEMETRY_ENABLED").ok(),
            goose_default_extension_timeout: config
                .get_param("GOOSE_DEFAULT_EXTENSION_TIMEOUT")
                .ok(),
            goose_prompt_editor: config.get_param("GOOSE_PROMPT_EDITOR").ok(),
            goose_prompt_editor_always: config.get_param("GOOSE_PROMPT_EDITOR_ALWAYS").ok(),
            goose_allowlist: config.get_param("GOOSE_ALLOWLIST").ok(),
            goose_system_prompt_file_path: config.get_param("GOOSE_SYSTEM_PROMPT_FILE_PATH").ok(),
            goose_debug: config.get_param("GOOSE_DEBUG").ok(),
            goose_show_full_output: config.get_param("GOOSE_SHOW_FULL_OUTPUT").ok(),
            goose_status_hook: config.get_param("GOOSE_STATUS_HOOK").ok(),
            goose_local_enable_thinking: config.get_param("GOOSE_LOCAL_ENABLE_THINKING").ok(),
            goose_databricks_client_request_id: config
                .get_param("GOOSE_DATABRICKS_CLIENT_REQUEST_ID")
                .ok(),
            context_file_names: config.get_param("CONTEXT_FILE_NAMES").ok(),
            edit_mode: config.get_param("EDIT_MODE").ok(),
            random_thinking_messages: config.get_param("RANDOM_THINKING_MESSAGES").ok(),
            code_mode_tool_disclosure: config.get_param("CODE_MODE_TOOL_DISCLOSURE").ok(),
            goose_client_cert_path: config.get_param("GOOSE_CLIENT_CERT_PATH").ok(),
            goose_client_key_path: config.get_param("GOOSE_CLIENT_KEY_PATH").ok(),
            goose_ca_cert_path: config.get_param("GOOSE_CA_CERT_PATH").ok(),
            goose_planner_provider: config.get_param("GOOSE_PLANNER_PROVIDER").ok(),
            goose_planner_model: config.get_param("GOOSE_PLANNER_MODEL").ok(),
            goose_subagent_provider: config.get_param("GOOSE_SUBAGENT_PROVIDER").ok(),
            goose_subagent_model: config.get_param("GOOSE_SUBAGENT_MODEL").ok(),
            goose_subagent_max_turns: config.get_param("GOOSE_SUBAGENT_MAX_TURNS").ok(),
            goose_max_background_tasks: config.get_param("GOOSE_MAX_BACKGROUND_TASKS").ok(),
            goose_recipe_github_repo: config.get_param("GOOSE_RECIPE_GITHUB_REPO").ok(),
            goose_recipe_retry_timeout_seconds: config
                .get_param("GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS")
                .ok(),
            goose_recipe_on_failure_timeout_seconds: config
                .get_param("GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS")
                .ok(),
            goose_cli_min_priority: config.get_param("GOOSE_CLI_MIN_PRIORITY").ok(),
            goose_cli_theme: config.get_param("GOOSE_CLI_THEME").ok(),
            goose_cli_light_theme: config.get_param("GOOSE_CLI_LIGHT_THEME").ok(),
            goose_cli_dark_theme: config.get_param("GOOSE_CLI_DARK_THEME").ok(),
            goose_cli_show_cost: config.get_param("GOOSE_CLI_SHOW_COST").ok(),
            goose_cli_show_thinking: config.get_param("GOOSE_CLI_SHOW_THINKING").ok(),
            goose_cli_newline_key: config.get_param("GOOSE_CLI_NEWLINE_KEY").ok(),
            claude_code_command: config.get_param("CLAUDE_CODE_COMMAND").ok(),
            gemini_cli_command: config.get_param("GEMINI_CLI_COMMAND").ok(),
            cursor_agent_command: config.get_param("CURSOR_AGENT_COMMAND").ok(),
            codex_command: config.get_param("CODEX_COMMAND").ok(),
            codex_reasoning_effort: config.get_param("CODEX_REASONING_EFFORT").ok(),
            codex_enable_skills: config.get_param("CODEX_ENABLE_SKILLS").ok(),
            codex_skip_git_check: config.get_param("CODEX_SKIP_GIT_CHECK").ok(),
            chatgpt_codex_reasoning_effort: config.get_param("CHATGPT_CODEX_REASONING_EFFORT").ok(),
            claude_thinking_type: config.get_param("CLAUDE_THINKING_TYPE").ok(),
            claude_thinking_effort: config.get_param("CLAUDE_THINKING_EFFORT").ok(),
            claude_thinking_budget: config.get_param("CLAUDE_THINKING_BUDGET").ok(),
            gemini3_thinking_level: config.get_param("GEMINI3_THINKING_LEVEL").ok(),
            gemini25_thinking_budget: config.get_param("GEMINI25_THINKING_BUDGET").ok(),
            security_prompt_enabled: config.get_param("SECURITY_PROMPT_ENABLED").ok(),
            security_prompt_threshold: config.get_param("SECURITY_PROMPT_THRESHOLD").ok(),
            security_prompt_classifier_enabled: config
                .get_param("SECURITY_PROMPT_CLASSIFIER_ENABLED")
                .ok(),
            security_prompt_classifier_model: config
                .get_param("SECURITY_PROMPT_CLASSIFIER_MODEL")
                .ok(),
            security_prompt_classifier_endpoint: config
                .get_param("SECURITY_PROMPT_CLASSIFIER_ENDPOINT")
                .ok(),
            security_command_classifier_enabled: config
                .get_param("SECURITY_COMMAND_CLASSIFIER_ENABLED")
                .ok(),
            openai_host: config.get_param("OPENAI_HOST").ok(),
            openai_base_url: config.get_param("OPENAI_BASE_URL").ok(),
            openai_base_path: config.get_param("OPENAI_BASE_PATH").ok(),
            openai_organization: config.get_param("OPENAI_ORGANIZATION").ok(),
            openai_project: config.get_param("OPENAI_PROJECT").ok(),
            openai_timeout: config.get_param("OPENAI_TIMEOUT").ok(),
            anthropic_host: config.get_param("ANTHROPIC_HOST").ok(),
            ollama_host: config.get_param("OLLAMA_HOST").ok(),
            ollama_timeout: config.get_param("OLLAMA_TIMEOUT").ok(),
            ollama_stream_timeout: config.get_param("OLLAMA_STREAM_TIMEOUT").ok(),
            ollama_stream_usage: config.get_param("OLLAMA_STREAM_USAGE").ok(),
            databricks_host: config.get_param("DATABRICKS_HOST").ok(),
            databricks_max_retries: config.get_param("DATABRICKS_MAX_RETRIES").ok(),
            databricks_initial_retry_interval_ms: config
                .get_param("DATABRICKS_INITIAL_RETRY_INTERVAL_MS")
                .ok(),
            databricks_backoff_multiplier: config.get_param("DATABRICKS_BACKOFF_MULTIPLIER").ok(),
            databricks_max_retry_interval_ms: config
                .get_param("DATABRICKS_MAX_RETRY_INTERVAL_MS")
                .ok(),
            azure_openai_endpoint: config.get_param("AZURE_OPENAI_ENDPOINT").ok(),
            azure_openai_deployment_name: config.get_param("AZURE_OPENAI_DEPLOYMENT_NAME").ok(),
            azure_openai_api_version: config.get_param("AZURE_OPENAI_API_VERSION").ok(),
            google_host: config.get_param("GOOGLE_HOST").ok(),
            gcp_project_id: config.get_param("GCP_PROJECT_ID").ok(),
            gcp_location: config.get_param("GCP_LOCATION").ok(),
            gcp_max_retries: config.get_param("GCP_MAX_RETRIES").ok(),
            gcp_initial_retry_interval_ms: config.get_param("GCP_INITIAL_RETRY_INTERVAL_MS").ok(),
            gcp_backoff_multiplier: config.get_param("GCP_BACKOFF_MULTIPLIER").ok(),
            gcp_max_retry_interval_ms: config.get_param("GCP_MAX_RETRY_INTERVAL_MS").ok(),
            aws_region: config.get_param("AWS_REGION").ok(),
            aws_profile: config.get_param("AWS_PROFILE").ok(),
            bedrock_max_retries: config.get_param("BEDROCK_MAX_RETRIES").ok(),
            bedrock_initial_retry_interval_ms: config
                .get_param("BEDROCK_INITIAL_RETRY_INTERVAL_MS")
                .ok(),
            bedrock_backoff_multiplier: config.get_param("BEDROCK_BACKOFF_MULTIPLIER").ok(),
            bedrock_max_retry_interval_ms: config.get_param("BEDROCK_MAX_RETRY_INTERVAL_MS").ok(),
            bedrock_enable_caching: config.get_param("BEDROCK_ENABLE_CACHING").ok(),
            sagemaker_endpoint_name: config.get_param("SAGEMAKER_ENDPOINT_NAME").ok(),
            litellm_host: config.get_param("LITELLM_HOST").ok(),
            litellm_base_path: config.get_param("LITELLM_BASE_PATH").ok(),
            litellm_timeout: config.get_param("LITELLM_TIMEOUT").ok(),
            snowflake_host: config.get_param("SNOWFLAKE_HOST").ok(),
            github_copilot_host: config.get_param("GITHUB_COPILOT_HOST").ok(),
            github_copilot_client_id: config.get_param("GITHUB_COPILOT_CLIENT_ID").ok(),
            github_copilot_token_url: config.get_param("GITHUB_COPILOT_TOKEN_URL").ok(),
            xai_host: config.get_param("XAI_HOST").ok(),
            openrouter_host: config.get_param("OPENROUTER_HOST").ok(),
            venice_host: config.get_param("VENICE_HOST").ok(),
            venice_base_path: config.get_param("VENICE_BASE_PATH").ok(),
            venice_models_path: config.get_param("VENICE_MODELS_PATH").ok(),
            tetrate_host: config.get_param("TETRATE_HOST").ok(),
            avian_host: config.get_param("AVIAN_HOST").ok(),
            otel_exporter_otlp_endpoint: config.get_param("otel_exporter_otlp_endpoint").ok(),
            otel_exporter_otlp_timeout: config.get_param("otel_exporter_otlp_timeout").ok(),
            tunnel_auto_start: config.get_param("tunnel_auto_start").ok(),
            extensions: config.get_param("extensions").ok(),
            slash_commands: config.get_param("slash_commands").ok(),
            experiments: config.get_param("experiments").ok(),
        }
    }

    pub fn apply_to_config(&self, config: &Config) -> Result<(), ConfigError> {
        let mut updates: Vec<(String, serde_json::Value)> = Vec::new();

        macro_rules! push_if_some {
            ($field:expr, $key:expr) => {
                if let Some(ref v) = $field {
                    match serde_json::to_value(v) {
                        Ok(json) => updates.push(($key.to_string(), json)),
                        Err(e) => tracing::warn!("Failed to serialize config key {}: {}", $key, e),
                    }
                }
            };
        }

        push_if_some!(self.goose_provider, "GOOSE_PROVIDER");
        push_if_some!(self.goose_model, "GOOSE_MODEL");
        push_if_some!(self.goose_mode, "GOOSE_MODE");
        push_if_some!(self.goose_max_tokens, "GOOSE_MAX_TOKENS");
        push_if_some!(self.goose_context_limit, "GOOSE_CONTEXT_LIMIT");
        push_if_some!(self.goose_input_limit, "GOOSE_INPUT_LIMIT");
        push_if_some!(self.goose_max_turns, "GOOSE_MAX_TURNS");
        push_if_some!(self.goose_max_active_agents, "GOOSE_MAX_ACTIVE_AGENTS");
        push_if_some!(
            self.goose_auto_compact_threshold,
            "GOOSE_AUTO_COMPACT_THRESHOLD"
        );
        push_if_some!(
            self.goose_tool_pair_summarization,
            "GOOSE_TOOL_PAIR_SUMMARIZATION"
        );
        push_if_some!(self.goose_tool_call_cutoff, "GOOSE_TOOL_CALL_CUTOFF");
        push_if_some!(self.goose_stream_timeout, "GOOSE_STREAM_TIMEOUT");
        push_if_some!(self.goose_search_paths, "GOOSE_SEARCH_PATHS");
        push_if_some!(
            self.goose_disable_session_naming,
            "GOOSE_DISABLE_SESSION_NAMING"
        );
        push_if_some!(self.goose_disable_keyring, "GOOSE_DISABLE_KEYRING");
        push_if_some!(self.goose_telemetry_enabled, "GOOSE_TELEMETRY_ENABLED");
        push_if_some!(
            self.goose_default_extension_timeout,
            "GOOSE_DEFAULT_EXTENSION_TIMEOUT"
        );
        push_if_some!(self.goose_prompt_editor, "GOOSE_PROMPT_EDITOR");
        push_if_some!(
            self.goose_prompt_editor_always,
            "GOOSE_PROMPT_EDITOR_ALWAYS"
        );
        push_if_some!(self.goose_allowlist, "GOOSE_ALLOWLIST");
        push_if_some!(
            self.goose_system_prompt_file_path,
            "GOOSE_SYSTEM_PROMPT_FILE_PATH"
        );
        push_if_some!(self.goose_debug, "GOOSE_DEBUG");
        push_if_some!(self.goose_show_full_output, "GOOSE_SHOW_FULL_OUTPUT");
        push_if_some!(self.goose_status_hook, "GOOSE_STATUS_HOOK");
        push_if_some!(
            self.goose_local_enable_thinking,
            "GOOSE_LOCAL_ENABLE_THINKING"
        );
        push_if_some!(
            self.goose_databricks_client_request_id,
            "GOOSE_DATABRICKS_CLIENT_REQUEST_ID"
        );
        push_if_some!(self.context_file_names, "CONTEXT_FILE_NAMES");
        push_if_some!(self.edit_mode, "EDIT_MODE");
        push_if_some!(self.random_thinking_messages, "RANDOM_THINKING_MESSAGES");
        push_if_some!(self.code_mode_tool_disclosure, "CODE_MODE_TOOL_DISCLOSURE");
        push_if_some!(self.goose_client_cert_path, "GOOSE_CLIENT_CERT_PATH");
        push_if_some!(self.goose_client_key_path, "GOOSE_CLIENT_KEY_PATH");
        push_if_some!(self.goose_ca_cert_path, "GOOSE_CA_CERT_PATH");
        push_if_some!(self.goose_planner_provider, "GOOSE_PLANNER_PROVIDER");
        push_if_some!(self.goose_planner_model, "GOOSE_PLANNER_MODEL");
        push_if_some!(self.goose_subagent_provider, "GOOSE_SUBAGENT_PROVIDER");
        push_if_some!(self.goose_subagent_model, "GOOSE_SUBAGENT_MODEL");
        push_if_some!(self.goose_subagent_max_turns, "GOOSE_SUBAGENT_MAX_TURNS");
        push_if_some!(
            self.goose_max_background_tasks,
            "GOOSE_MAX_BACKGROUND_TASKS"
        );
        push_if_some!(self.goose_recipe_github_repo, "GOOSE_RECIPE_GITHUB_REPO");
        push_if_some!(
            self.goose_recipe_retry_timeout_seconds,
            "GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS"
        );
        push_if_some!(
            self.goose_recipe_on_failure_timeout_seconds,
            "GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS"
        );
        push_if_some!(self.goose_cli_min_priority, "GOOSE_CLI_MIN_PRIORITY");
        push_if_some!(self.goose_cli_theme, "GOOSE_CLI_THEME");
        push_if_some!(self.goose_cli_light_theme, "GOOSE_CLI_LIGHT_THEME");
        push_if_some!(self.goose_cli_dark_theme, "GOOSE_CLI_DARK_THEME");
        push_if_some!(self.goose_cli_show_cost, "GOOSE_CLI_SHOW_COST");
        push_if_some!(self.goose_cli_show_thinking, "GOOSE_CLI_SHOW_THINKING");
        push_if_some!(self.goose_cli_newline_key, "GOOSE_CLI_NEWLINE_KEY");
        push_if_some!(self.claude_code_command, "CLAUDE_CODE_COMMAND");
        push_if_some!(self.gemini_cli_command, "GEMINI_CLI_COMMAND");
        push_if_some!(self.cursor_agent_command, "CURSOR_AGENT_COMMAND");
        push_if_some!(self.codex_command, "CODEX_COMMAND");
        push_if_some!(self.codex_reasoning_effort, "CODEX_REASONING_EFFORT");
        push_if_some!(self.codex_enable_skills, "CODEX_ENABLE_SKILLS");
        push_if_some!(self.codex_skip_git_check, "CODEX_SKIP_GIT_CHECK");
        push_if_some!(
            self.chatgpt_codex_reasoning_effort,
            "CHATGPT_CODEX_REASONING_EFFORT"
        );
        push_if_some!(self.claude_thinking_type, "CLAUDE_THINKING_TYPE");
        push_if_some!(self.claude_thinking_effort, "CLAUDE_THINKING_EFFORT");
        push_if_some!(self.claude_thinking_budget, "CLAUDE_THINKING_BUDGET");
        push_if_some!(self.gemini3_thinking_level, "GEMINI3_THINKING_LEVEL");
        push_if_some!(self.gemini25_thinking_budget, "GEMINI25_THINKING_BUDGET");
        push_if_some!(self.security_prompt_enabled, "SECURITY_PROMPT_ENABLED");
        push_if_some!(self.security_prompt_threshold, "SECURITY_PROMPT_THRESHOLD");
        push_if_some!(
            self.security_prompt_classifier_enabled,
            "SECURITY_PROMPT_CLASSIFIER_ENABLED"
        );
        push_if_some!(
            self.security_prompt_classifier_model,
            "SECURITY_PROMPT_CLASSIFIER_MODEL"
        );
        push_if_some!(
            self.security_prompt_classifier_endpoint,
            "SECURITY_PROMPT_CLASSIFIER_ENDPOINT"
        );
        push_if_some!(
            self.security_command_classifier_enabled,
            "SECURITY_COMMAND_CLASSIFIER_ENABLED"
        );
        push_if_some!(self.openai_host, "OPENAI_HOST");
        push_if_some!(self.openai_base_url, "OPENAI_BASE_URL");
        push_if_some!(self.openai_base_path, "OPENAI_BASE_PATH");
        push_if_some!(self.openai_organization, "OPENAI_ORGANIZATION");
        push_if_some!(self.openai_project, "OPENAI_PROJECT");
        push_if_some!(self.openai_timeout, "OPENAI_TIMEOUT");
        push_if_some!(self.anthropic_host, "ANTHROPIC_HOST");
        push_if_some!(self.ollama_host, "OLLAMA_HOST");
        push_if_some!(self.ollama_timeout, "OLLAMA_TIMEOUT");
        push_if_some!(self.ollama_stream_timeout, "OLLAMA_STREAM_TIMEOUT");
        push_if_some!(self.ollama_stream_usage, "OLLAMA_STREAM_USAGE");
        push_if_some!(self.databricks_host, "DATABRICKS_HOST");
        push_if_some!(self.databricks_max_retries, "DATABRICKS_MAX_RETRIES");
        push_if_some!(
            self.databricks_initial_retry_interval_ms,
            "DATABRICKS_INITIAL_RETRY_INTERVAL_MS"
        );
        push_if_some!(
            self.databricks_backoff_multiplier,
            "DATABRICKS_BACKOFF_MULTIPLIER"
        );
        push_if_some!(
            self.databricks_max_retry_interval_ms,
            "DATABRICKS_MAX_RETRY_INTERVAL_MS"
        );
        push_if_some!(self.azure_openai_endpoint, "AZURE_OPENAI_ENDPOINT");
        push_if_some!(
            self.azure_openai_deployment_name,
            "AZURE_OPENAI_DEPLOYMENT_NAME"
        );
        push_if_some!(self.azure_openai_api_version, "AZURE_OPENAI_API_VERSION");
        push_if_some!(self.google_host, "GOOGLE_HOST");
        push_if_some!(self.gcp_project_id, "GCP_PROJECT_ID");
        push_if_some!(self.gcp_location, "GCP_LOCATION");
        push_if_some!(self.gcp_max_retries, "GCP_MAX_RETRIES");
        push_if_some!(
            self.gcp_initial_retry_interval_ms,
            "GCP_INITIAL_RETRY_INTERVAL_MS"
        );
        push_if_some!(self.gcp_backoff_multiplier, "GCP_BACKOFF_MULTIPLIER");
        push_if_some!(self.gcp_max_retry_interval_ms, "GCP_MAX_RETRY_INTERVAL_MS");
        push_if_some!(self.aws_region, "AWS_REGION");
        push_if_some!(self.aws_profile, "AWS_PROFILE");
        push_if_some!(self.bedrock_max_retries, "BEDROCK_MAX_RETRIES");
        push_if_some!(
            self.bedrock_initial_retry_interval_ms,
            "BEDROCK_INITIAL_RETRY_INTERVAL_MS"
        );
        push_if_some!(
            self.bedrock_backoff_multiplier,
            "BEDROCK_BACKOFF_MULTIPLIER"
        );
        push_if_some!(
            self.bedrock_max_retry_interval_ms,
            "BEDROCK_MAX_RETRY_INTERVAL_MS"
        );
        push_if_some!(self.bedrock_enable_caching, "BEDROCK_ENABLE_CACHING");
        push_if_some!(self.sagemaker_endpoint_name, "SAGEMAKER_ENDPOINT_NAME");
        push_if_some!(self.litellm_host, "LITELLM_HOST");
        push_if_some!(self.litellm_base_path, "LITELLM_BASE_PATH");
        push_if_some!(self.litellm_timeout, "LITELLM_TIMEOUT");
        push_if_some!(self.snowflake_host, "SNOWFLAKE_HOST");
        push_if_some!(self.github_copilot_host, "GITHUB_COPILOT_HOST");
        push_if_some!(self.github_copilot_client_id, "GITHUB_COPILOT_CLIENT_ID");
        push_if_some!(self.github_copilot_token_url, "GITHUB_COPILOT_TOKEN_URL");
        push_if_some!(self.xai_host, "XAI_HOST");
        push_if_some!(self.openrouter_host, "OPENROUTER_HOST");
        push_if_some!(self.venice_host, "VENICE_HOST");
        push_if_some!(self.venice_base_path, "VENICE_BASE_PATH");
        push_if_some!(self.venice_models_path, "VENICE_MODELS_PATH");
        push_if_some!(self.tetrate_host, "TETRATE_HOST");
        push_if_some!(self.avian_host, "AVIAN_HOST");
        push_if_some!(
            self.otel_exporter_otlp_endpoint,
            "otel_exporter_otlp_endpoint"
        );
        push_if_some!(
            self.otel_exporter_otlp_timeout,
            "otel_exporter_otlp_timeout"
        );
        push_if_some!(self.tunnel_auto_start, "tunnel_auto_start");
        push_if_some!(self.extensions, "extensions");
        push_if_some!(self.slash_commands, "slash_commands");
        push_if_some!(self.experiments, "experiments");

        config.set_param_values(&updates)
    }
}

/// Config update payload for `PATCH /config/typed`.
///
/// Embeds all non-secret fields from [`GooseConfigSchema`] via `#[serde(flatten)]`,
/// plus provider API key fields that route to the system keyring.
/// Fields set to `null` are left unchanged — to delete a key, use `POST /config/remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct GooseConfigUpdate {
    #[serde(flatten)]
    pub config: GooseConfigSchema,

    // === Provider API Keys (secrets, stored in keyring) ===
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    #[serde(rename = "ANTHROPIC_API_KEY")]
    pub anthropic_api_key: Option<String>,
    #[serde(rename = "GOOGLE_API_KEY")]
    pub google_api_key: Option<String>,
    #[serde(rename = "DATABRICKS_TOKEN")]
    pub databricks_token: Option<String>,
    #[serde(rename = "AZURE_OPENAI_API_KEY")]
    pub azure_openai_api_key: Option<String>,
    #[serde(rename = "OPENROUTER_API_KEY")]
    pub openrouter_api_key: Option<String>,
    #[serde(rename = "XAI_API_KEY")]
    pub xai_api_key: Option<String>,
    #[serde(rename = "AVIAN_API_KEY")]
    pub avian_api_key: Option<String>,
    #[serde(rename = "VENICE_API_KEY")]
    pub venice_api_key: Option<String>,
    #[serde(rename = "TETRATE_API_KEY")]
    pub tetrate_api_key: Option<String>,
    #[serde(rename = "LITELLM_API_KEY")]
    pub litellm_api_key: Option<String>,
    #[serde(rename = "SNOWFLAKE_TOKEN")]
    pub snowflake_token: Option<String>,
    #[serde(rename = "GROQ_API_KEY")]
    pub groq_api_key: Option<String>,
}

impl GooseConfigUpdate {
    pub fn apply_to_config(&self, config: &Config) -> Result<(), ConfigError> {
        self.config.apply_to_config(config)?;

        let mut secret_updates: Vec<(String, serde_json::Value)> = Vec::new();

        macro_rules! push_secret {
            ($field:expr, $key:expr) => {
                if let Some(ref v) = $field {
                    match serde_json::to_value(v) {
                        Ok(json) => secret_updates.push(($key.to_string(), json)),
                        Err(e) => tracing::warn!("Failed to serialize secret key {}: {}", $key, e),
                    }
                }
            };
        }

        push_secret!(self.openai_api_key, "OPENAI_API_KEY");
        push_secret!(self.anthropic_api_key, "ANTHROPIC_API_KEY");
        push_secret!(self.google_api_key, "GOOGLE_API_KEY");
        push_secret!(self.databricks_token, "DATABRICKS_TOKEN");
        push_secret!(self.azure_openai_api_key, "AZURE_OPENAI_API_KEY");
        push_secret!(self.openrouter_api_key, "OPENROUTER_API_KEY");
        push_secret!(self.xai_api_key, "XAI_API_KEY");
        push_secret!(self.avian_api_key, "AVIAN_API_KEY");
        push_secret!(self.venice_api_key, "VENICE_API_KEY");
        push_secret!(self.tetrate_api_key, "TETRATE_API_KEY");
        push_secret!(self.litellm_api_key, "LITELLM_API_KEY");
        push_secret!(self.snowflake_token, "SNOWFLAKE_TOKEN");
        push_secret!(self.groq_api_key, "GROQ_API_KEY");

        config.set_secret_values(&secret_updates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::schema_for;

    #[test]
    fn all_keys_matches_struct_fields() {
        let schema = schema_for!(GooseConfigSchema);
        let obj = schema.as_object().expect("schema should be an object");
        let properties = obj
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("schema should have properties");

        let schema_keys: std::collections::HashSet<&str> =
            properties.keys().map(|k| k.as_str()).collect();

        let category_b: std::collections::HashSet<&str> =
            ["extensions", "slash_commands", "experiments"]
                .iter()
                .copied()
                .collect();

        // Forward: every ALL_KEYS entry must exist in the struct
        for key in GooseConfigSchema::ALL_KEYS {
            assert!(
                schema_keys.contains(key),
                "ALL_KEYS contains '{key}' but GooseConfigSchema has no field with serde(rename = \"{key}\")"
            );
        }

        // Reverse: every struct field (except Category B) must be in ALL_KEYS
        for key in &schema_keys {
            if !category_b.contains(key) {
                assert!(
                    GooseConfigSchema::has_key(key),
                    "GooseConfigSchema has field '{key}' but it's missing from ALL_KEYS (add it, or mark as Category B)"
                );
            }
        }

        // Category B keys are in the struct but NOT in ALL_KEYS — that's intentional
        for key in &category_b {
            assert!(
                schema_keys.contains(key),
                "Category B key '{key}' should be in the schema struct for IDE autocomplete"
            );
            assert!(
                !GooseConfigSchema::has_key(key),
                "Category B key '{key}' should NOT be in ALL_KEYS"
            );
        }
    }

    #[test]
    fn roundtrip_config_values() {
        let config_file = tempfile::NamedTempFile::new().unwrap();
        let secrets_file = tempfile::NamedTempFile::new().unwrap();
        let config =
            Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap();

        config
            .set_param_values(&[
                (
                    "GOOSE_PROVIDER".to_string(),
                    serde_json::Value::String("anthropic".to_string()),
                ),
                (
                    "GOOSE_MAX_TOKENS".to_string(),
                    serde_json::Value::Number(4096.into()),
                ),
                ("GOOSE_DEBUG".to_string(), serde_json::Value::Bool(true)),
                (
                    "GOOSE_DISABLE_KEYRING".to_string(),
                    serde_json::Value::Bool(true),
                ),
                (
                    "SECURITY_PROMPT_THRESHOLD".to_string(),
                    serde_json::json!(0.75),
                ),
            ])
            .expect("set_param_values should succeed");

        let typed = GooseConfigSchema::from_config(&config);
        assert_eq!(typed.goose_provider.as_deref(), Some("anthropic"));
        assert_eq!(typed.goose_max_tokens, Some(4096));
        assert_eq!(typed.goose_debug, Some(true));
        assert_eq!(typed.goose_disable_keyring, Some(true));
        assert_eq!(typed.security_prompt_threshold, Some(0.75));

        // Roundtrip: apply to a fresh config and verify
        let config_file2 = tempfile::NamedTempFile::new().unwrap();
        let secrets_file2 = tempfile::NamedTempFile::new().unwrap();
        let config2 =
            Config::new_with_file_secrets(config_file2.path(), secrets_file2.path()).unwrap();
        typed
            .apply_to_config(&config2)
            .expect("apply_to_config should succeed");
        let typed2 = GooseConfigSchema::from_config(&config2);
        assert_eq!(typed2.goose_provider, typed.goose_provider);
        assert_eq!(typed2.goose_max_tokens, typed.goose_max_tokens);
        assert_eq!(typed2.goose_debug, typed.goose_debug);
        assert_eq!(typed2.goose_disable_keyring, typed.goose_disable_keyring);
    }
}
