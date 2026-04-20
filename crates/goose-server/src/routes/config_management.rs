use crate::routes::errors::ErrorResponse;
use crate::routes::utils::check_provider_configured;
use crate::state::AppState;
use axum::routing::put;
use axum::{
    extract::Path,
    routing::{delete, get, post},
    Json, Router,
};
use goose::config::declarative_providers::LoadedProvider;
use goose::config::paths::Paths;
use goose::config::ExtensionEntry;
use goose::config::{Config, ConfigError};
use goose::model::ModelConfig;
use goose::providers::base::{ProviderMetadata, ProviderType};
use goose::providers::canonical::maybe_get_canonical_model;
use goose::providers::catalog::{
    get_provider_template, get_providers_by_format, ProviderCatalogEntry, ProviderFormat,
    ProviderTemplate,
};
use goose::providers::create_with_default_model;
use goose::providers::providers as get_providers;
use goose::{
    agents::execute_commands, agents::ExtensionConfig, config::permission::PermissionLevel,
    slash_commands,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_yaml;
use std::{collections::HashMap, sync::Arc};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct ExtensionResponse {
    pub extensions: Vec<ExtensionEntry>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ExtensionQuery {
    pub name: String,
    pub config: ExtensionConfig,
    pub enabled: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct UpsertConfigQuery {
    pub key: String,
    pub value: Value,
    pub is_secret: bool,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct ConfigKeyQuery {
    pub key: String,
    pub is_secret: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ConfigResponse {
    pub config: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProviderDetails {
    pub name: String,
    pub metadata: ProviderMetadata,
    pub is_configured: bool,
    pub provider_type: ProviderType,
}

#[derive(Serialize, ToSchema)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderDetails>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ToolPermission {
    pub tool_name: String,
    pub permission: PermissionLevel,
}

#[derive(Deserialize, ToSchema)]
pub struct UpsertPermissionsQuery {
    pub tool_permissions: Vec<ToolPermission>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateCustomProviderRequest {
    pub engine: String,
    pub display_name: String,
    pub api_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    pub supports_streaming: Option<bool>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    #[serde(default = "default_requires_auth")]
    pub requires_auth: bool,
    #[serde(default)]
    pub catalog_provider_id: Option<String>,
    #[serde(default)]
    pub base_path: Option<String>,
}

fn default_requires_auth() -> bool {
    true
}

#[derive(Deserialize, ToSchema)]
pub struct CheckProviderRequest {
    pub provider: String,
}

#[derive(Deserialize, ToSchema)]
pub struct SetProviderRequest {
    pub provider: String,
    pub model: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaskedSecret {
    pub masked_value: String,
}

#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub enum ConfigValueResponse {
    Value(Value),
    MaskedValue(MaskedSecret),
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum CommandType {
    Builtin,
    Recipe,
    Skill,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SlashCommand {
    pub command: String,
    pub help: String,
    pub command_type: CommandType,
}
#[derive(Serialize, ToSchema)]
pub struct SlashCommandsResponse {
    pub commands: Vec<SlashCommand>,
}

#[utoipa::path(
    post,
    path = "/config/upsert",
    request_body = UpsertConfigQuery,
    responses(
        (status = 200, description = "Configuration value upserted successfully", body = String),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn upsert_config(
    Json(query): Json<UpsertConfigQuery>,
) -> Result<Json<Value>, ErrorResponse> {
    let config = Config::global();
    config.set(&query.key, &query.value, query.is_secret)?;
    Ok(Json(Value::String(format!("Upserted key {}", query.key))))
}

#[utoipa::path(
    post,
    path = "/config/remove",
    request_body = ConfigKeyQuery,
    responses(
        (status = 200, description = "Configuration value removed successfully", body = String),
        (status = 404, description = "Configuration key not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn remove_config(
    Json(query): Json<ConfigKeyQuery>,
) -> Result<Json<String>, ErrorResponse> {
    let config = Config::global();

    if query.is_secret {
        config.delete_secret(&query.key)?;
    } else {
        config.delete(&query.key)?;
    }

    Ok(Json(format!("Removed key {}", query.key)))
}

const SECRET_MASK_SHOW_LEN: usize = 8;

fn mask_secret(secret: Value) -> String {
    let as_string = match secret {
        Value::String(s) => s,
        _ => serde_json::to_string(&secret).unwrap_or_else(|_| secret.to_string()),
    };

    let chars: Vec<_> = as_string.chars().collect();
    let show_len = std::cmp::min(chars.len() / 2, SECRET_MASK_SHOW_LEN);
    let visible: String = chars.iter().take(show_len).collect();
    let mask = "*".repeat(chars.len() - show_len);

    format!("{}{}", visible, mask)
}

fn is_valid_provider_name(provider_name: &str) -> bool {
    !provider_name.is_empty()
        && provider_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[utoipa::path(
    post,
    path = "/config/read",
    request_body = ConfigKeyQuery,
    responses(
        (status = 200, description = "Configuration value retrieved successfully", body = Value),
        (status = 500, description = "Unable to get the configuration value"),
    )
)]
pub async fn read_config(
    Json(query): Json<ConfigKeyQuery>,
) -> Result<Json<ConfigValueResponse>, ErrorResponse> {
    let config = Config::global();

    let response_value = match config.get(&query.key, query.is_secret) {
        Ok(value) => {
            if query.is_secret {
                ConfigValueResponse::MaskedValue(MaskedSecret {
                    masked_value: mask_secret(value),
                })
            } else {
                ConfigValueResponse::Value(value)
            }
        }
        Err(ConfigError::NotFound(_)) => ConfigValueResponse::Value(Value::Null),
        Err(e) => return Err(e.into()),
    };
    Ok(Json(response_value))
}

#[utoipa::path(
    get,
    path = "/config/extensions",
    responses(
        (status = 200, description = "All extensions retrieved successfully", body = ExtensionResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_extensions() -> Result<Json<ExtensionResponse>, ErrorResponse> {
    let extensions = goose::config::get_all_extensions()
        .into_iter()
        .filter(|ext| !goose::agents::extension_manager::is_hidden_extension(&ext.config.name()))
        .collect();
    let warnings = goose::config::get_warnings();
    Ok(Json(ExtensionResponse {
        extensions,
        warnings,
    }))
}

#[utoipa::path(
    post,
    path = "/config/extensions",
    request_body = ExtensionQuery,
    responses(
        (status = 200, description = "Extension added or updated successfully", body = String),
        (status = 400, description = "Invalid request"),
        (status = 422, description = "Could not serialize config.yaml"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn add_extension(
    Json(extension_query): Json<ExtensionQuery>,
) -> Result<Json<String>, ErrorResponse> {
    let extensions = goose::config::get_all_extensions();
    let key = goose::config::extensions::name_to_key(&extension_query.name);

    let is_update = extensions.iter().any(|e| e.config.key() == key);

    goose::config::set_extension(ExtensionEntry {
        enabled: extension_query.enabled,
        config: extension_query.config,
    });

    if is_update {
        Ok(Json(format!("Updated extension {}", extension_query.name)))
    } else {
        Ok(Json(format!("Added extension {}", extension_query.name)))
    }
}

#[utoipa::path(
    delete,
    path = "/config/extensions/{name}",
    responses(
        (status = 200, description = "Extension removed successfully", body = String),
        (status = 404, description = "Extension not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn remove_extension(Path(name): Path<String>) -> Result<Json<String>, ErrorResponse> {
    let key = goose::config::extensions::name_to_key(&name);
    goose::config::remove_extension(&key);
    Ok(Json(format!("Removed extension {}", name)))
}

#[utoipa::path(
    get,
    path = "/config",
    responses(
        (status = 200, description = "All configuration values retrieved successfully", body = ConfigResponse)
    )
)]
pub async fn read_all_config() -> Result<Json<ConfigResponse>, ErrorResponse> {
    let config = Config::global();
    let values = config
        .all_values()
        .map_err(|e| ErrorResponse::unprocessable(e.to_string()))?;
    Ok(Json(ConfigResponse { config: values }))
}

#[utoipa::path(
    get,
    path = "/config/providers",
    responses(
        (status = 200, description = "All configuration values retrieved successfully", body = [ProviderDetails])
    )
)]
pub async fn providers() -> Result<Json<Vec<ProviderDetails>>, ErrorResponse> {
    let providers = get_providers().await;
    let providers_response: Vec<ProviderDetails> = providers
        .into_iter()
        .map(|(metadata, provider_type)| {
            let is_configured = check_provider_configured(&metadata, provider_type);

            ProviderDetails {
                name: metadata.name.clone(),
                metadata,
                is_configured,
                provider_type,
            }
        })
        .collect();

    Ok(Json(providers_response))
}

#[utoipa::path(
    get,
    path = "/config/providers/{name}/models",
    params(
        ("name" = String, Path, description = "Provider name (e.g., openai)")
    ),
    responses(
        (status = 200, description = "Models fetched successfully", body = [String]),
        (status = 400, description = "Unknown provider, provider not configured, or authentication error"),
        (status = 429, description = "Rate limit exceeded"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_provider_models(
    Path(name): Path<String>,
) -> Result<Json<Vec<String>>, ErrorResponse> {
    let all = get_providers().await.into_iter().collect::<Vec<_>>();
    let Some((metadata, provider_type)) = all.into_iter().find(|(m, _)| m.name == name) else {
        return Err(ErrorResponse::bad_request(format!(
            "Unknown provider: {}",
            name
        )));
    };
    if !check_provider_configured(&metadata, provider_type) {
        return Err(ErrorResponse::bad_request(format!(
            "Provider '{}' is not configured",
            name
        )));
    }

    let model_config = ModelConfig::new(&metadata.default_model)?.with_canonical_limits(&name);
    let provider = goose::providers::create(&name, model_config, Vec::new()).await?;

    let models_result = provider.fetch_recommended_models().await;

    match models_result {
        Ok(models) => Ok(Json(models)),
        Err(provider_error) => Err(provider_error.into()),
    }
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct SlashCommandsQuery {
    /// Optional working directory to discover local skills from
    pub working_dir: Option<String>,
}

#[utoipa::path(
    get,
    path = "/config/slash_commands",
    params(SlashCommandsQuery),
    responses(
        (status = 200, description = "Slash commands retrieved successfully", body = SlashCommandsResponse)
    )
)]
pub async fn get_slash_commands(
    axum::extract::Query(query): axum::extract::Query<SlashCommandsQuery>,
) -> Result<Json<SlashCommandsResponse>, ErrorResponse> {
    let mut commands: Vec<_> = slash_commands::list_commands()
        .iter()
        .map(|command| SlashCommand {
            command: command.command.clone(),
            help: command.recipe_path.clone(),
            command_type: CommandType::Recipe,
        })
        .collect();

    for cmd_def in execute_commands::list_commands() {
        commands.push(SlashCommand {
            command: cmd_def.name.to_string(),
            help: cmd_def.description.to_string(),
            command_type: CommandType::Builtin,
        });
    }

    let working_dir = query.working_dir.map(std::path::PathBuf::from);
    for source in
        goose::agents::platform_extensions::skills::list_installed_skills(working_dir.as_deref())
    {
        commands.push(SlashCommand {
            command: source.name,
            help: source.description,
            command_type: CommandType::Skill,
        });
    }

    let discover_dir = working_dir
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new("."));
    for source in
        goose::agents::platform_extensions::summon::discover_filesystem_sources(discover_dir)
    {
        use goose::agents::platform_extensions::SourceKind;
        if matches!(
            source.kind,
            SourceKind::Agent | SourceKind::Recipe | SourceKind::Subrecipe
        ) && !source.content.is_empty()
        {
            commands.push(SlashCommand {
                command: source.name,
                help: source.description,
                command_type: CommandType::Agent,
            });
        }
    }

    Ok(Json(SlashCommandsResponse { commands }))
}

#[derive(Serialize, ToSchema)]
pub struct ModelInfoData {
    pub provider: String,
    pub model: String,
    pub context_limit: usize,
    pub max_output_tokens: Option<usize>,
    pub input_token_cost: Option<f64>,
    pub output_token_cost: Option<f64>,
    pub cache_read_token_cost: Option<f64>,
    pub cache_write_token_cost: Option<f64>,
    pub currency: String,
}

#[derive(Serialize, ToSchema)]
pub struct ModelInfoResponse {
    pub model_info: Option<ModelInfoData>,
    pub source: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ModelInfoQuery {
    pub provider: String,
    pub model: String,
}

#[utoipa::path(
    post,
    path = "/config/canonical-model-info",
    request_body = ModelInfoQuery,
    responses(
        (status = 200, description = "Model information retrieved successfully", body = ModelInfoResponse)
    )
)]
pub async fn get_canonical_model_info(
    Json(query): Json<ModelInfoQuery>,
) -> Json<ModelInfoResponse> {
    let canonical_model = maybe_get_canonical_model(&query.provider, &query.model);

    let model_info = canonical_model.map(|canonical_model| ModelInfoData {
        provider: query.provider.clone(),
        model: query.model.clone(),
        context_limit: canonical_model.limit.context,
        max_output_tokens: canonical_model.limit.output,
        // Costs are per million tokens - client handles division for display
        input_token_cost: canonical_model.cost.input,
        output_token_cost: canonical_model.cost.output,
        cache_read_token_cost: canonical_model.cost.cache_read,
        cache_write_token_cost: canonical_model.cost.cache_write,
        currency: "$".to_string(),
    });

    Json(ModelInfoResponse {
        model_info,
        source: "canonical".to_string(),
    })
}

#[utoipa::path(
    post,
    path = "/config/init",
    responses(
        (status = 200, description = "Config initialization check completed", body = String),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn init_config() -> Result<Json<String>, ErrorResponse> {
    let config = Config::global();

    if config.exists() {
        return Ok(Json("Config already exists".to_string()));
    }

    // Use the shared function to load init-config.yaml
    match goose::config::base::load_init_config_from_workspace() {
        Ok(init_values) => {
            config.initialize_if_empty(init_values)?;
            Ok(Json("Config initialized successfully".to_string()))
        }
        Err(_) => Ok(Json(
            "No init-config.yaml found, using default configuration".to_string(),
        )),
    }
}

#[utoipa::path(
    post,
    path = "/config/permissions",
    request_body = UpsertPermissionsQuery,
    responses(
        (status = 200, description = "Permission update completed", body = String),
        (status = 400, description = "Invalid request"),
    )
)]
pub async fn upsert_permissions(
    Json(query): Json<UpsertPermissionsQuery>,
) -> Result<Json<String>, ErrorResponse> {
    let permission_manager = goose::config::PermissionManager::instance();

    for tool_permission in &query.tool_permissions {
        permission_manager.update_user_permission(
            &tool_permission.tool_name,
            tool_permission.permission.clone(),
        );
    }

    Ok(Json("Permissions updated successfully".to_string()))
}

#[utoipa::path(
    post,
    path = "/config/backup",
    responses(
        (status = 200, description = "Config file backed up", body = String),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn backup_config() -> Result<Json<String>, ErrorResponse> {
    let config_path = Paths::config_dir().join("config.yaml");

    if !config_path.exists() {
        return Err(ErrorResponse::not_found("Config file does not exist"));
    }

    let file_name = config_path
        .file_name()
        .ok_or_else(|| ErrorResponse::internal("Invalid config file path"))?;

    let mut backup_name = file_name.to_os_string();
    backup_name.push(".bak");

    let backup = config_path.with_file_name(backup_name);
    std::fs::copy(&config_path, &backup)?;
    Ok(Json(format!("Copied {:?} to {:?}", config_path, backup)))
}

#[utoipa::path(
    post,
    path = "/config/recover",
    responses(
        (status = 200, description = "Config recovery attempted", body = String),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn recover_config() -> Result<Json<String>, ErrorResponse> {
    let config = Config::global();

    // Force a reload which will trigger recovery if needed
    let values = config.all_values()?;
    let recovered_keys: Vec<String> = values.keys().cloned().collect();

    if recovered_keys.is_empty() {
        Ok(Json("Config recovery completed, but no data was recoverable. Starting with empty configuration.".to_string()))
    } else {
        Ok(Json(format!(
            "Config recovery completed. Recovered {} keys: {}",
            recovered_keys.len(),
            recovered_keys.join(", ")
        )))
    }
}

#[utoipa::path(
    get,
    path = "/config/validate",
    responses(
        (status = 200, description = "Config validation result", body = String),
        (status = 422, description = "Config file is corrupted")
    )
)]
pub async fn validate_config() -> Result<Json<String>, ErrorResponse> {
    let config_path = Paths::config_dir().join("config.yaml");

    if !config_path.exists() {
        return Ok(Json("Config file does not exist".to_string()));
    }

    let content = std::fs::read_to_string(&config_path)?;
    serde_yaml::from_str::<serde_yaml::Value>(&content)
        .map_err(|e| ErrorResponse::unprocessable(format!("Config file is corrupted: {}", e)))?;

    Ok(Json("Config file is valid".to_string()))
}
#[derive(Serialize, ToSchema)]
pub struct CreateCustomProviderResponse {
    pub provider_name: String,
}

#[utoipa::path(
    post,
    path = "/config/custom-providers",
    request_body = UpdateCustomProviderRequest,
    responses(
        (status = 200, description = "Custom provider created successfully", body = CreateCustomProviderResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_custom_provider(
    Json(request): Json<UpdateCustomProviderRequest>,
) -> Result<Json<CreateCustomProviderResponse>, ErrorResponse> {
    let config = goose::config::declarative_providers::create_custom_provider(
        goose::config::declarative_providers::CreateCustomProviderParams {
            engine: request.engine,
            display_name: request.display_name,
            api_url: request.api_url,
            api_key: request.api_key,
            models: request.models,
            supports_streaming: request.supports_streaming,
            headers: request.headers,
            requires_auth: request.requires_auth,
            catalog_provider_id: request.catalog_provider_id,
            base_path: request.base_path,
        },
    )?;

    goose::providers::refresh_custom_providers().await?;

    Ok(Json(CreateCustomProviderResponse {
        provider_name: config.id().to_string(),
    }))
}

#[utoipa::path(
    get,
    path = "/config/custom-providers/{id}",
    responses(
        (status = 200, description = "Custom provider retrieved successfully", body = LoadedProvider),
        (status = 404, description = "Provider not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_custom_provider(
    Path(id): Path<String>,
) -> Result<Json<LoadedProvider>, ErrorResponse> {
    let loaded_provider = goose::config::declarative_providers::load_provider(id.as_str())
        .map_err(|e| {
            ErrorResponse::not_found(format!("Custom provider '{}' not found: {}", id, e))
        })?;

    Ok(Json(loaded_provider))
}

#[utoipa::path(
    delete,
    path = "/config/custom-providers/{id}",
    responses(
        (status = 200, description = "Custom provider removed successfully", body = String),
        (status = 404, description = "Provider not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn remove_custom_provider(Path(id): Path<String>) -> Result<Json<String>, ErrorResponse> {
    goose::config::declarative_providers::remove_custom_provider(&id)?;

    goose::providers::refresh_custom_providers().await?;

    Ok(Json(format!("Removed custom provider: {}", id)))
}

#[utoipa::path(
    post,
    path = "/config/providers/{name}/cleanup",
    params(
        ("name" = String, Path, description = "Provider name (e.g., githubcopilot)")
    ),
    responses(
        (status = 200, description = "Provider cache cleaned up successfully", body = String),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn cleanup_provider_cache(
    Path(name): Path<String>,
) -> Result<Json<String>, ErrorResponse> {
    goose::providers::cleanup_provider(&name).await?;
    Ok(Json(format!("Cleaned up provider cache: {}", name)))
}

#[utoipa::path(
    put,
    path = "/config/custom-providers/{id}",
    request_body = UpdateCustomProviderRequest,
    responses(
        (status = 200, description = "Custom provider updated successfully", body = String),
        (status = 404, description = "Provider not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_custom_provider(
    Path(id): Path<String>,
    Json(request): Json<UpdateCustomProviderRequest>,
) -> Result<Json<String>, ErrorResponse> {
    goose::config::declarative_providers::update_custom_provider(
        goose::config::declarative_providers::UpdateCustomProviderParams {
            id: id.clone(),
            engine: request.engine,
            display_name: request.display_name,
            api_url: request.api_url,
            api_key: request.api_key,
            models: request.models,
            supports_streaming: request.supports_streaming,
            headers: request.headers,
            requires_auth: request.requires_auth,
            catalog_provider_id: request.catalog_provider_id,
            base_path: request.base_path,
        },
    )?;

    goose::providers::refresh_custom_providers().await?;

    Ok(Json(format!("Updated custom provider: {}", id)))
}

#[utoipa::path(
    post,
    path = "/config/check_provider",
    request_body = CheckProviderRequest,
)]
pub async fn check_provider(
    Json(CheckProviderRequest { provider }): Json<CheckProviderRequest>,
) -> Result<(), ErrorResponse> {
    // Provider check does not use extensions.
    create_with_default_model(&provider, Vec::new())
        .await
        .map_err(|err| {
            ErrorResponse::bad_request(format!("Provider '{}' check failed: {}", provider, err))
        })?;
    Ok(())
}

#[utoipa::path(
    post,
    path = "/config/set_provider",
    request_body = SetProviderRequest,
)]
pub async fn set_config_provider(
    Json(SetProviderRequest { provider, model }): Json<SetProviderRequest>,
) -> Result<(), ErrorResponse> {
    // Provider validation does not use extensions.
    create_with_default_model(&provider, Vec::new())
        .await
        .and_then(|_| {
            let config = Config::global();
            config
                .set_goose_provider(provider.clone())
                .and_then(|_| config.set_goose_model(model.clone()))
                .map_err(|e| anyhow::anyhow!(e))
        })
        .map_err(|err| {
            ErrorResponse::bad_request(format!(
                "Failed to set provider to '{}' with model '{}': {}",
                provider, model, err
            ))
        })?;
    Ok(())
}

#[utoipa::path(
    get,
    path = "/config/provider-catalog",
    params(
        ("format" = Option<String>, Query, description = "Filter by provider format (openai, anthropic, ollama)")
    ),
    responses(
        (status = 200, description = "Provider catalog retrieved successfully", body = [ProviderCatalogEntry]),
        (status = 400, description = "Invalid format parameter")
    )
)]
pub async fn get_provider_catalog(
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProviderCatalogEntry>>, ErrorResponse> {
    let format_str = params.get("format").map(|s| s.as_str()).unwrap_or("openai");

    let format = format_str.parse::<ProviderFormat>().map_err(|_| {
        ErrorResponse::bad_request(format!(
            "Invalid format '{}'. Must be one of: openai, anthropic, ollama",
            format_str
        ))
    })?;

    let providers = get_providers_by_format(format).await;
    Ok(Json(providers))
}

#[utoipa::path(
    get,
    path = "/config/provider-catalog/{id}",
    params(
        ("id" = String, Path, description = "Provider ID from models.dev")
    ),
    responses(
        (status = 200, description = "Provider template retrieved successfully", body = ProviderTemplate),
        (status = 404, description = "Provider not found in catalog")
    )
)]
pub async fn get_provider_catalog_template(
    Path(id): Path<String>,
) -> Result<Json<ProviderTemplate>, ErrorResponse> {
    let template = get_provider_template(&id).ok_or_else(|| {
        ErrorResponse::not_found(format!("Provider '{}' not found in catalog", id))
    })?;

    Ok(Json(template))
}

#[utoipa::path(
    post,
    path = "/config/providers/{name}/oauth",
    params(
        ("name" = String, Path, description = "Provider name")
    ),
    responses(
        (status = 200, description = "OAuth configuration completed"),
        (status = 400, description = "OAuth configuration failed")
    )
)]
pub async fn configure_provider_oauth(
    Path(provider_name): Path<String>,
) -> Result<Json<String>, ErrorResponse> {
    use goose::model::ModelConfig;
    use goose::providers::create;

    if !is_valid_provider_name(&provider_name) {
        return Err(ErrorResponse::bad_request(format!(
            "Invalid provider name: '{}'",
            provider_name
        )));
    }

    let temp_model = ModelConfig::new("temp")
        .map_err(|e| {
            ErrorResponse::bad_request(format!("Failed to create temporary model config: {}", e))
        })?
        .with_canonical_limits(&provider_name);

    // OAuth configuration does not use extensions.
    let provider = create(&provider_name, temp_model, Vec::new())
        .await
        .map_err(|e| {
            ErrorResponse::bad_request(format!(
                "Failed to create provider '{}': {}",
                provider_name, e
            ))
        })?;

    provider.configure_oauth().await.map_err(|e| {
        ErrorResponse::bad_request(format!(
            "OAuth configuration failed for provider '{}': {}",
            provider_name, e
        ))
    })?;

    // Mark the provider as configured after successful OAuth
    let configured_marker = format!("{}_configured", provider_name);
    let config = goose::config::Config::global();
    config.set_param(&configured_marker, true)?;

    Ok(Json("OAuth configuration completed".to_string()))
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/config", get(read_all_config))
        .route("/config/upsert", post(upsert_config))
        .route("/config/remove", post(remove_config))
        .route("/config/read", post(read_config))
        .route("/config/extensions", get(get_extensions))
        .route("/config/extensions", post(add_extension))
        .route("/config/extensions/{name}", delete(remove_extension))
        .route("/config/providers", get(providers))
        .route("/config/providers/{name}/models", get(get_provider_models))
        .route("/config/provider-catalog", get(get_provider_catalog))
        .route(
            "/config/provider-catalog/{id}",
            get(get_provider_catalog_template),
        )
        .route(
            "/config/providers/{name}/cleanup",
            post(cleanup_provider_cache),
        )
        .route("/config/slash_commands", get(get_slash_commands))
        .route(
            "/config/canonical-model-info",
            post(get_canonical_model_info),
        )
        .route("/config/init", post(init_config))
        .route("/config/backup", post(backup_config))
        .route("/config/recover", post(recover_config))
        .route("/config/validate", get(validate_config))
        .route("/config/permissions", post(upsert_permissions))
        .route("/config/custom-providers", post(create_custom_provider))
        .route(
            "/config/custom-providers/{id}",
            delete(remove_custom_provider),
        )
        .route("/config/custom-providers/{id}", put(update_custom_provider))
        .route("/config/custom-providers/{id}", get(get_custom_provider))
        .route("/config/check_provider", post(check_provider))
        .route("/config/set_provider", post(set_config_provider))
        .route(
            "/config/providers/{name}/oauth",
            post(configure_provider_oauth),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {}
