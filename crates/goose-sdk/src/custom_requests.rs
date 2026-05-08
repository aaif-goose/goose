use agent_client_protocol::{JsonRpcRequest, JsonRpcResponse};
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

/// Call a tool from an extension.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/tool/call", response = GooseToolCallResponse)]
#[serde(rename_all = "camelCase")]
pub struct GooseToolCallRequest {
    pub session_id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// Tool call response.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct GooseToolCallResponse {
    #[serde(default)]
    pub content: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<serde_json::Value>,
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "_meta")]
    pub meta: Option<serde_json::Value>,
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

/// Read allowlisted user preferences. Empty `keys` means all supported preferences.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/preferences/read", response = PreferencesReadResponse)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesReadRequest {
    #[serde(default)]
    pub keys: Vec<PreferenceKey>,
}

/// Save allowlisted user preferences.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/preferences/save", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesSaveRequest {
    #[serde(default)]
    pub values: Vec<PreferenceValue>,
}

/// Remove allowlisted user preferences.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/preferences/remove", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesRemoveRequest {
    #[serde(default)]
    pub keys: Vec<PreferenceKey>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PreferenceKey {
    #[default]
    AutoCompactThreshold,
    VoiceAutoSubmitPhrases,
    VoiceDictationProvider,
    VoiceDictationPreferredMic,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceValue {
    pub key: PreferenceKey,
    #[serde(default)]
    pub value: serde_json::Value,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesReadResponse {
    pub values: Vec<PreferenceValue>,
}

/// Read Goose default provider and model configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/defaults/read", response = DefaultsReadResponse)]
#[serde(rename_all = "camelCase")]
pub struct DefaultsReadRequest {}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct DefaultsReadResponse {
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
}

/// Save Goose default provider and model configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/defaults/save", response = DefaultsReadResponse)]
#[serde(rename_all = "camelCase")]
pub struct DefaultsSaveRequest {
    pub provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

/// Sources that onboarding knows how to discover and import.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingImportSourceKind {
    #[default]
    GooseConfig,
    ClaudeDesktop,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingImportCounts {
    pub providers: u32,
    pub extensions: u32,
    pub sessions: u32,
    pub skills: u32,
    pub projects: u32,
    pub preferences: u32,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingImportCandidate {
    pub id: String,
    pub source_kind: OnboardingImportSourceKind,
    pub display_name: String,
    pub path: String,
    pub counts: OnboardingImportCounts,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Scan for existing Goose and compatible app data that onboarding can import.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/onboarding/import/scan",
    response = OnboardingImportScanResponse
)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingImportScanRequest {
    /// Empty means all supported import sources.
    #[serde(default)]
    pub sources: Vec<OnboardingImportSourceKind>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingImportScanResponse {
    pub candidates: Vec<OnboardingImportCandidate>,
}

/// Import selected onboarding candidates.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/onboarding/import/apply",
    response = OnboardingImportApplyResponse
)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingImportApplyRequest {
    #[serde(default)]
    pub candidate_ids: Vec<String>,
    #[serde(default)]
    pub enable_imported_extensions: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingImportApplyResponse {
    pub imported: OnboardingImportCounts,
    pub skipped: OnboardingImportCounts,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_defaults: Option<DefaultsReadResponse>,
}

/// Set a dictation provider secret value.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/dictation/secret/save", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct DictationSecretSaveRequest {
    pub provider: String,
    pub value: String,
}

/// Remove a dictation provider secret value.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/dictation/secret/delete", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct DictationSecretDeleteRequest {
    pub provider: String,
}

/// Update the project association for a session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/session/update_project", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionProjectRequest {
    pub session_id: String,
    pub project_id: Option<String>,
}

/// Rename a session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/session/rename", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct RenameSessionRequest {
    pub session_id: String,
    pub title: String,
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

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigFieldValueDto {
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
    pub is_set: bool,
    pub is_secret: bool,
    pub required: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigStatusDto {
    pub provider_id: String,
    pub is_configured: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigFieldUpdate {
    pub key: String,
    pub value: String,
}

/// Read saved configuration field values for one provider.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/config/read",
    response = ProviderConfigReadResponse
)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigReadRequest {
    pub provider_id: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigReadResponse {
    pub fields: Vec<ProviderConfigFieldValueDto>,
}

/// Return provider configured statuses. Empty provider_ids means all providers.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/config/status",
    response = ProviderConfigStatusResponse
)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigStatusRequest {
    #[serde(default)]
    pub provider_ids: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigStatusResponse {
    pub statuses: Vec<ProviderConfigStatusDto>,
}

/// Save provider configuration fields and start an inventory refresh when supported.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/config/save",
    response = ProviderConfigChangeResponse
)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigSaveRequest {
    pub provider_id: String,
    pub fields: Vec<ProviderConfigFieldUpdate>,
}

/// Delete provider configuration fields and start an inventory refresh when supported.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/config/delete",
    response = ProviderConfigChangeResponse
)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigDeleteRequest {
    pub provider_id: String,
}

/// Run a provider-owned native authentication flow and start an inventory refresh when supported.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/config/authenticate",
    response = ProviderConfigChangeResponse
)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigAuthenticateRequest {
    pub provider_id: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigChangeResponse {
    pub status: ProviderConfigStatusDto,
    pub refresh: RefreshProviderInventoryResponse,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTemplateCatalogEntryDto {
    pub provider_id: String,
    pub name: String,
    pub format: String,
    pub api_url: String,
    pub model_count: usize,
    pub doc_url: String,
    pub env_var: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupCategoryDto {
    Agent,
    #[default]
    Model,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupMethodDto {
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupGroupDto {
    Default,
    Additional,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupFieldDto {
    pub key: String,
    pub label: String,
    pub secret: bool,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupCatalogEntryDto {
    pub provider_id: String,
    pub name: String,
    pub category: ProviderSetupCategoryDto,
    pub description: String,
    pub setup_method: ProviderSetupMethodDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_connect_query: Option<String>,
    #[serde(default)]
    pub fields: Vec<ProviderSetupFieldDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_url: Option<String>,
    pub group: ProviderSetupGroupDto,
    pub show_only_when_installed: bool,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub supports_install: bool,
    pub supports_auth: bool,
    pub supports_auth_status: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTemplateCapabilitiesDto {
    pub tool_call: bool,
    pub reasoning: bool,
    pub attachment: bool,
    pub temperature: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTemplateModelDto {
    pub id: String,
    pub name: String,
    pub context_limit: usize,
    pub capabilities: ProviderTemplateCapabilitiesDto,
    pub deprecated: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTemplateDto {
    pub provider_id: String,
    pub name: String,
    pub format: String,
    pub api_url: String,
    pub models: Vec<ProviderTemplateModelDto>,
    pub supports_streaming: bool,
    pub env_var: String,
    pub doc_url: String,
}

/// List custom-provider catalog entries. Omit `format` to list all formats.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/catalog/list",
    response = ProviderCatalogListResponse
)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalogListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalogListResponse {
    pub providers: Vec<ProviderTemplateCatalogEntryDto>,
}

/// List provider setup catalog entries
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/setup/catalog/list",
    response = ProviderSetupCatalogListResponse
)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupCatalogListRequest {}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupCatalogListResponse {
    pub providers: Vec<ProviderSetupCatalogEntryDto>,
}

/// Return the editable template for one catalog provider.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/catalog/template",
    response = ProviderCatalogTemplateResponse
)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalogTemplateRequest {
    pub provider_id: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalogTemplateResponse {
    pub template: ProviderTemplateDto,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderConfigDto {
    pub provider_id: String,
    pub engine: String,
    pub display_name: String,
    pub api_url: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_streaming: Option<bool>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub requires_auth: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    pub api_key_set: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderUpsertDto {
    pub engine: String,
    pub display_name: String,
    pub api_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_streaming: Option<bool>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub requires_auth: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
}

/// Create a custom provider backed by Goose's declarative provider store.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/custom/create",
    response = CustomProviderCreateResponse
)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderCreateRequest {
    #[serde(flatten)]
    pub provider: CustomProviderUpsertDto,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderCreateResponse {
    pub provider_id: String,
    pub status: ProviderConfigStatusDto,
    pub refresh: RefreshProviderInventoryResponse,
}

/// Read a declarative provider config. Custom configs are editable; bundled configs are read-only.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/custom/read",
    response = CustomProviderReadResponse
)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderReadRequest {
    pub provider_id: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderReadResponse {
    pub provider: CustomProviderConfigDto,
    pub editable: bool,
    pub status: ProviderConfigStatusDto,
}

/// Update a custom provider backed by Goose's declarative provider store.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/custom/update",
    response = CustomProviderUpdateResponse
)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderUpdateRequest {
    pub provider_id: String,
    #[serde(flatten)]
    pub provider: CustomProviderUpsertDto,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderUpdateResponse {
    pub provider_id: String,
    pub status: ProviderConfigStatusDto,
    pub refresh: RefreshProviderInventoryResponse,
}

/// Delete a custom provider from Goose's declarative provider store.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/providers/custom/delete",
    response = CustomProviderDeleteResponse
)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderDeleteRequest {
    pub provider_id: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderDeleteResponse {
    pub provider_id: String,
    pub refresh: RefreshProviderInventoryResponse,
}

/// The type of source entity.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum SourceType {
    #[default]
    Skill,
    BuiltinSkill,
    Recipe,
    Subrecipe,
    Agent,
    Project,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::Skill => write!(f, "skill"),
            SourceType::BuiltinSkill => write!(f, "builtin skill"),
            SourceType::Recipe => write!(f, "recipe"),
            SourceType::Subrecipe => write!(f, "subrecipe"),
            SourceType::Agent => write!(f, "agent"),
            SourceType::Project => write!(f, "project"),
        }
    }
}

/// A source discovered by Goose. Filesystem sources use an on-disk path;
/// built-in sources use a stable synthetic path. Sources may be either
/// `global` (shared across all projects) or project-specific.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceEntry {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub name: String,
    pub description: String,
    pub content: String,
    /// Stable on-disk path identifying this source. Pass it back to
    /// update/delete/export to operate on this entry. Skills use the directory
    /// containing `SKILL.md`; projects use the project file path; built-in
    /// skills use `builtin://skills/<name>` synthetic paths.
    pub path: String,
    /// True when the source lives in the user's global sources directory; false
    /// when it lives inside a specific project.
    pub global: bool,
    /// Paths (absolute) of additional files that live alongside the source.
    /// Only skills currently populate this; empty for other source types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supporting_files: Vec<String>,
    /// Arbitrary key/value pairs for type-specific metadata (e.g. icon, color,
    /// preferredProvider for projects). Stored in the frontmatter.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub properties: std::collections::HashMap<String, serde_json::Value>,
}

impl SourceEntry {
    /// Render this source as a markdown block suitable for injecting into an
    /// LLM context. Used by the skills and summon runtimes when loading a
    /// source into the current conversation.
    pub fn to_load_text(&self) -> String {
        format!(
            "## {} ({})\n\n{}\n\n### Content\n\n{}",
            self.name, self.source_type, self.description, self.content
        )
    }
}

/// Create a new source in an explicit target scope (global or project-scoped).
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
    /// Project source ID. When set with `global: false`, the backend resolves
    /// the project's first working directory automatically. Takes precedence
    /// over `project_dir`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Arbitrary key/value metadata.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub properties: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct CreateSourceResponse {
    pub source: SourceEntry,
}

/// List discovered sources.
///
/// If `type` is omitted or `skill`, this lists filesystem/plugin skills only.
/// Both global and project-scoped skills are included when `project_dir` is
/// set. If `type` is `builtinSkill`, this lists shipped read-only built-in
/// skills.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/sources/list", response = ListSourcesResponse)]
#[serde(rename_all = "camelCase")]
pub struct ListSourcesRequest {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<SourceType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
    /// When true, also scan the working directories of all known projects for
    /// project-scoped sources (e.g. skills stored under `{workingDir}/.agents/skills/`).
    #[serde(default)]
    pub include_project_sources: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ListSourcesResponse {
    pub sources: Vec<SourceEntry>,
}

/// Update an existing source's name, description, and content by absolute path.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/sources/update", response = UpdateSourceResponse)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSourceRequest {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub path: String,
    pub name: String,
    pub description: String,
    pub content: String,
    /// When `Some`, replaces all stored properties on the source. When
    /// `None` (or omitted), the source's existing properties are
    /// preserved. Callers that don't model the full property bag (e.g.
    /// the skills editor, which only edits name/description/content)
    /// should omit this so per-skill metadata isn't silently erased.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<std::collections::HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSourceResponse {
    pub source: SourceEntry,
}

/// Delete a source and its on-disk directory by absolute path.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/sources/delete", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSourceRequest {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub path: String,
}

/// Export a source at an absolute path as a portable JSON payload.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/sources/export", response = ExportSourceResponse)]
#[serde(rename_all = "camelCase")]
pub struct ExportSourceRequest {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub path: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct ExportSourceResponse {
    pub json: String,
    pub filename: String,
}

/// Import a source from a JSON export payload produced by `_goose/sources/export`.
/// The imported source is written into the explicit target scope; on name
/// collisions a `-imported` suffix is appended.
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
    /// Whether this inventory entry represents an agent provider or a model provider.
    pub category: ProviderSetupCategoryDto,
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
