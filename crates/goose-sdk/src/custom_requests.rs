use sacp::{JsonRpcRequest, JsonRpcResponse};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Schema descriptor for a single custom method, produced by the
/// `#[custom_methods]` macro's generated `custom_method_schemas()` function.
///
/// `params_schema` / `response_schema` hold `$ref` pointers or inline schemas
/// produced by `SchemaGenerator::subschema_for`. All referenced types are
/// collected in the generator's `$defs` map.
///
/// `params_type_name` / `response_type_name` carry the Rust struct name so the
/// binary can key `$defs` entries and annotate them with `x-method` / `x-side`.
#[derive(Debug, Serialize)]
pub struct CustomMethodSchema {
    pub method: String,
    pub params_schema: Option<schemars::Schema>,
    pub params_type_name: Option<String>,
    pub response_schema: Option<schemars::Schema>,
    pub response_type_name: Option<String>,
}

/// Add an extension to an active session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/extensions/add", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct AddExtensionRequest {
    pub session_id: String,
    /// Extension configuration (see ExtensionConfig variants: Stdio, StreamableHttp, Builtin, Platform).
    #[serde(default)]
    pub config: serde_json::Value,
}

/// Remove an extension from an active session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/extensions/remove", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct RemoveExtensionRequest {
    pub session_id: String,
    pub name: String,
}

/// List all tools available in a session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/tools", response = GetToolsResponse)]
#[serde(rename_all = "camelCase")]
pub struct GetToolsRequest {
    pub session_id: String,
}

/// Tools response.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
pub struct GetToolsResponse {
    /// Array of tool info objects with `name`, `description`, `parameters`, and optional `permission`.
    pub tools: Vec<serde_json::Value>,
}

/// Read a resource from an extension.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/resource/read", response = ReadResourceResponse)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceRequest {
    pub session_id: String,
    pub uri: String,
    pub extension_name: String,
}

/// Resource read response.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
pub struct ReadResourceResponse {
    /// The resource result from the extension (MCP ReadResourceResult).
    #[serde(default)]
    pub result: serde_json::Value,
}

/// Update the working directory for a session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/working_dir/update", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkingDirRequest {
    pub session_id: String,
    pub working_dir: String,
}

/// Delete a session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "session/delete", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionRequest {
    pub session_id: String,
}

/// List configured extensions and any warnings.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/config/extensions", response = GetExtensionsResponse)]
pub struct GetExtensionsRequest {}

/// List configured extensions and any warnings.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
pub struct GetExtensionsResponse {
    /// Array of ExtensionEntry objects with `enabled` flag, `configKey`, and flattened config details.
    pub extensions: Vec<serde_json::Value>,
    pub warnings: Vec<String>,
}

/// Persist a new extension to the user's global goose config.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/config/extensions/add", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct AddConfigExtensionRequest {
    pub name: String,
    /// Extension configuration. Must be a JSON object matching one of the
    /// `ExtensionConfig` variants (e.g. `stdio`, `streamable_http`, `builtin`).
    /// `name` and `enabled` are injected server-side.
    #[serde(default)]
    pub extension_config: serde_json::Value,
    #[serde(default)]
    pub enabled: bool,
}

/// Remove a persisted extension from the user's global goose config.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/config/extensions/remove", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct RemoveConfigExtensionRequest {
    pub config_key: String,
}

/// Toggle the `enabled` flag for a persisted extension in the user's global goose config.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/config/extensions/toggle", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct ToggleConfigExtensionRequest {
    pub config_key: String,
    pub enabled: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/session/extensions", response = GetSessionExtensionsResponse)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionExtensionsRequest {
    pub session_id: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
pub struct GetSessionExtensionsResponse {
    pub extensions: Vec<serde_json::Value>,
}

/// Read a single non-secret config value.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/config/read", response = ReadConfigResponse)]
#[serde(rename_all = "camelCase")]
pub struct ReadConfigRequest {
    pub key: String,
}

/// Config read response.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ReadConfigResponse {
    #[serde(default)]
    pub value: serde_json::Value,
}

/// Upsert a single non-secret config value.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/config/upsert", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct UpsertConfigRequest {
    pub key: String,
    pub value: serde_json::Value,
}

/// Remove a single non-secret config value.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/config/remove", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct RemoveConfigRequest {
    pub key: String,
}

/// Check whether a secret exists. Never returns the actual value.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/secret/check", response = CheckSecretResponse)]
#[serde(rename_all = "camelCase")]
pub struct CheckSecretRequest {
    pub key: String,
}

/// Secret check response.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct CheckSecretResponse {
    pub exists: bool,
}

/// Set a secret value (write-only).
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/secret/upsert", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct UpsertSecretRequest {
    pub key: String,
    pub value: serde_json::Value,
}

/// Remove a secret.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/secret/remove", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct RemoveSecretRequest {
    pub key: String,
}

/// Update the project association for a session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/session/update_project", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionProjectRequest {
    pub session_id: String,
    pub project_id: Option<String>,
}

/// Archive a session (soft delete).
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/session/archive", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSessionRequest {
    pub session_id: String,
}

/// Unarchive a previously archived session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/session/unarchive", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct UnarchiveSessionRequest {
    pub session_id: String,
}

/// Export a session as a JSON string.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/session/export", response = ExportSessionResponse)]
#[serde(rename_all = "camelCase")]
pub struct ExportSessionRequest {
    pub session_id: String,
}

/// Export session response — raw JSON of the goose session with `conversation`.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
pub struct ExportSessionResponse {
    pub data: String,
}

/// Import a session from a JSON string.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/session/import", response = ImportSessionResponse)]
pub struct ImportSessionRequest {
    pub data: String,
}

/// Import session response — metadata about the newly created session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ImportSessionResponse {
    pub session_id: String,
    pub title: Option<String>,
    pub updated_at: Option<String>,
    pub message_count: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigKey {
    pub name: String,
    pub required: bool,
    pub secret: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub oauth_flow: bool,
    #[serde(default)]
    pub device_code_flow: bool,
    #[serde(default)]
    pub primary: bool,
}

/// The type of source entity.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SourceType {
    #[default]
    Skill,
}

/// A source — a user-editable entity backed by an on-disk directory. Sources
/// may be either `global` (shared across all projects) or project-specific.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceEntry {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub name: String,
    pub description: String,
    pub content: String,
    /// Absolute path to the source's directory on disk.
    pub directory: String,
    /// True when the source lives in the user's global sources directory; false
    /// when it lives inside a specific project.
    pub global: bool,
}

/// Create a new source (global or project-scoped).
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/sources/create", response = CreateSourceResponse)]
#[serde(rename_all = "camelCase")]
pub struct CreateSourceRequest {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub name: String,
    pub description: String,
    pub content: String,
    pub global: bool,
    /// Absolute path to the project root. Required when `global` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct CreateSourceResponse {
    pub source: SourceEntry,
}

/// List sources. If `type` is omitted, sources of all known types are returned.
/// Both global and project-scoped sources are included when `project_dir` is set.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/sources/list", response = ListSourcesResponse)]
#[serde(rename_all = "camelCase")]
pub struct ListSourcesRequest {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<SourceType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ListSourcesResponse {
    pub sources: Vec<SourceEntry>,
}

/// Update an existing source's description and content.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/sources/update", response = UpdateSourceResponse)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSourceRequest {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub name: String,
    pub description: String,
    pub content: String,
    pub global: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSourceResponse {
    pub source: SourceEntry,
}

/// Delete a source and its on-disk directory.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/sources/delete", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSourceRequest {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub name: String,
    pub global: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
}

/// Export a source as a portable JSON payload.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/sources/export", response = ExportSourceResponse)]
#[serde(rename_all = "camelCase")]
pub struct ExportSourceRequest {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub name: String,
    pub global: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ExportSourceResponse {
    pub json: String,
    pub filename: String,
}

/// Import a source from a JSON export payload produced by `_goose/sources/export`.
/// The imported source is written under the given scope; on name collisions a
/// `-imported` suffix is appended.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/sources/import", response = ImportSourcesResponse)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourcesRequest {
    pub data: String,
    pub global: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourcesResponse {
    pub sources: Vec<SourceEntry>,
}

/// Transcribe audio via a dictation provider.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/dictation/transcribe", response = DictationTranscribeResponse)]
#[serde(rename_all = "camelCase")]
pub struct DictationTranscribeRequest {
    /// Base64-encoded audio data
    pub audio: String,
    /// MIME type (e.g. "audio/wav", "audio/webm")
    pub mime_type: String,
    /// Provider to use: "openai", "groq", "elevenlabs", or "local"
    pub provider: String,
}

/// Transcription result.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
pub struct DictationTranscribeResponse {
    pub text: String,
}

/// Get the configuration status of all dictation providers.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/dictation/config", response = DictationConfigResponse)]
pub struct DictationConfigRequest {}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DictationModelOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

/// Per-provider configuration status.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DictationProviderStatusEntry {
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub description: String,
    pub uses_provider_config: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_config_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_model: Option<String>,
    #[serde(default)]
    pub available_models: Vec<DictationModelOption>,
}

/// Dictation config response — map of provider name to status.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
pub struct DictationConfigResponse {
    pub providers: HashMap<String, DictationProviderStatusEntry>,
}

/// List providers with setup metadata and the current model inventory snapshot.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/providers/list", response = ListProvidersResponse)]
#[serde(rename_all = "camelCase")]
pub struct ListProvidersRequest {
    /// Only return entries for these providers. Empty means all.
    #[serde(default)]
    pub provider_ids: Vec<String>,
}

/// Provider list response.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
pub struct ListProvidersResponse {
    pub entries: Vec<ProviderInventoryEntryDto>,
}

/// Trigger a background refresh of provider inventories.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/inventory/refresh",
    response = RefreshProviderInventoryResponse
)]
#[serde(rename_all = "camelCase")]
pub struct RefreshProviderInventoryRequest {
    /// Which providers to refresh. Empty means all known providers.
    #[serde(default)]
    pub provider_ids: Vec<String>,
}

/// Refresh acknowledgement.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct RefreshProviderInventoryResponse {
    /// Which providers will be refreshed.
    pub started: Vec<String>,
    /// Which providers were skipped and why.
    #[serde(default)]
    pub skipped: Vec<RefreshProviderInventorySkipDto>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefreshProviderInventorySkipDto {
    pub provider_id: String,
    pub reason: RefreshProviderInventorySkipReasonDto,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RefreshProviderInventorySkipReasonDto {
    #[default]
    UnknownProvider,
    NotConfigured,
    DoesNotSupportRefresh,
    AlreadyRefreshing,
}

/// A single model in provider inventory.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInventoryModelDto {
    /// Model identifier as the provider knows it.
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Model family for grouping in UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Context window size in tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<usize>,
    /// Whether the model supports reasoning/extended thinking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    /// Whether this model should appear in the compact recommended picker.
    #[serde(default)]
    pub recommended: bool,
}

/// Provider inventory entry.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInventoryEntryDto {
    /// Provider identifier.
    pub provider_id: String,
    /// Human-readable provider name.
    pub provider_name: String,
    /// Description of the provider's capabilities.
    pub description: String,
    /// The default/recommended model for this provider.
    pub default_model: String,
    /// Whether Goose has enough configuration to use this provider.
    pub configured: bool,
    /// Provider classification such as `Preferred`, `Builtin`, `Declarative`, or `Custom`.
    pub provider_type: String,
    /// Required configuration keys and setup metadata.
    pub config_keys: Vec<ProviderConfigKey>,
    /// Step-by-step setup instructions, when present.
    pub setup_steps: Vec<String>,
    /// Whether this provider supports background inventory refresh.
    pub supports_refresh: bool,
    /// Whether a refresh is currently in flight.
    pub refreshing: bool,
    /// The list of available models.
    pub models: Vec<ProviderInventoryModelDto>,
    /// When this entry was last successfully refreshed (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated_at: Option<String>,
    /// When a refresh was most recently attempted (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_refresh_attempt_at: Option<String>,
    /// The last refresh failure message, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_refresh_error: Option<String>,
    /// Whether we believe this data may be outdated.
    pub stale: bool,
    /// Guidance message shown when this provider manages its own model selection externally.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_selection_hint: Option<String>,
}

/// Empty success response for operations that return no data.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
pub struct EmptyResponse {}

/// List available local Whisper models with their download status.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/dictation/models/list",
    response = DictationModelsListResponse
)]
#[serde(rename_all = "camelCase")]
pub struct DictationModelsListRequest {}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct DictationModelsListResponse {
    pub models: Vec<DictationLocalModelStatus>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DictationLocalModelStatus {
    pub id: String,
    pub label: String,
    pub description: String,
    pub size_mb: u32,
    pub downloaded: bool,
    pub download_in_progress: bool,
}

/// Kick off a background download of a local Whisper model.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/dictation/models/download", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct DictationModelDownloadRequest {
    pub model_id: String,
}

/// Poll the progress of an in-flight download.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/dictation/models/download/progress",
    response = DictationModelDownloadProgressResponse
)]
#[serde(rename_all = "camelCase")]
pub struct DictationModelDownloadProgressRequest {
    pub model_id: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct DictationModelDownloadProgressResponse {
    /// None when no download is active for this model id.
    pub progress: Option<DictationDownloadProgress>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DictationDownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub progress_percent: f32,
    /// serde lowercase of DownloadStatus: "downloading" | "completed" | "failed" | "cancelled"
    pub status: String,
    pub error: Option<String>,
}

/// Cancel an in-flight download.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/dictation/models/cancel", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct DictationModelCancelRequest {
    pub model_id: String,
}

/// Delete a downloaded local Whisper model from disk.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/dictation/models/delete", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct DictationModelDeleteRequest {
    pub model_id: String,
}

/// Persist the user's model selection for a given provider.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/dictation/model/select", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct DictationModelSelectRequest {
    pub provider: String,
    pub model_id: String,
}
