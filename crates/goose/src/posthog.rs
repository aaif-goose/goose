use crate::config::paths::Paths;
use crate::config::{get_enabled_extensions, Config};
use crate::session::session_manager::CURRENT_SCHEMA_VERSION;
use crate::session::SessionManager;
#[cfg(target_os = "windows")]
use crate::subprocess::SubprocessExt;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use uuid::Uuid;

const POSTHOG_API_KEY: &str = "phc_RyX5CaY01VtZJCQyhSR5KFh6qimUy81YwxsEpotAftT";
const POSTHOG_CAPTURE_URL: &str = "https://us.i.posthog.com/capture/";

/// Config key for telemetry opt-out preference
pub const TELEMETRY_ENABLED_KEY: &str = "GOOSE_TELEMETRY_ENABLED";

static TELEMETRY_DISABLED_BY_ENV: Lazy<AtomicBool> = Lazy::new(|| {
    std::env::var("GOOSE_TELEMETRY_OFF")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
        .into()
});

/// Check if the user has made a telemetry choice.
///
/// Returns Some(true) if telemetry is enabled, Some(false) if disabled,
/// or None if the user hasn't made a choice yet.
pub fn get_telemetry_choice() -> Option<bool> {
    if TELEMETRY_DISABLED_BY_ENV.load(Ordering::Relaxed) {
        return Some(false);
    }

    let config = Config::global();
    config.get_param::<bool>(TELEMETRY_ENABLED_KEY).ok()
}

/// Check if telemetry is enabled.
///
/// Returns false if:
/// - GOOSE_TELEMETRY_OFF environment variable is set to "1" or "true"
/// - GOOSE_TELEMETRY_ENABLED config value is set to false
/// - User has not made a telemetry choice yet (opt-in required)
///
/// Returns true only if the user has explicitly opted in.
pub fn is_telemetry_enabled() -> bool {
    get_telemetry_choice().unwrap_or(false)
}

// ============================================================================
// PostHog HTTP API
// ============================================================================

#[derive(Serialize)]
struct CaptureEvent {
    api_key: &'static str,
    event: String,
    distinct_id: String,
    properties: HashMap<String, serde_json::Value>,
    timestamp: Option<String>,
}

async fn posthog_capture(
    event_name: &str,
    distinct_id: &str,
    properties: HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    let payload = CaptureEvent {
        api_key: POSTHOG_API_KEY,
        event: event_name.to_string(),
        distinct_id: distinct_id.to_string(),
        properties,
        timestamp: Some(Utc::now().to_rfc3339()),
    };

    let client = reqwest::Client::new();
    client
        .post(POSTHOG_CAPTURE_URL)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("{e}"))?;

    Ok(())
}

// ============================================================================
// Installation Tracking
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallationData {
    installation_id: String,
    first_seen: DateTime<Utc>,
    session_count: u32,
}

impl Default for InstallationData {
    fn default() -> Self {
        Self {
            installation_id: Uuid::new_v4().to_string(),
            first_seen: Utc::now(),
            session_count: 0,
        }
    }
}

fn installation_file_path() -> std::path::PathBuf {
    Paths::state_dir().join("telemetry_installation.json")
}

fn load_or_create_installation() -> InstallationData {
    let path = installation_file_path();

    if let Ok(contents) = fs::read_to_string(&path) {
        if let Ok(data) = serde_json::from_str::<InstallationData>(&contents) {
            return data;
        }
    }

    let data = InstallationData::default();
    save_installation(&data);
    data
}

fn save_installation(data: &InstallationData) {
    let path = installation_file_path();

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string_pretty(data) {
        let _ = fs::write(path, json);
    }
}

fn increment_session_count() -> InstallationData {
    let mut data = load_or_create_installation();
    data.session_count += 1;
    save_installation(&data);
    data
}

// ============================================================================
// Platform Info
// ============================================================================

fn get_platform_version() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }
    #[cfg(target_os = "linux")]
    {
        fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|line| line.starts_with("VERSION_ID="))
                    .map(|line| {
                        line.trim_start_matches("VERSION_ID=")
                            .trim_matches('"')
                            .to_string()
                    })
            })
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "ver"])
            .set_no_window()
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn detect_install_method() -> String {
    let exe_path = std::env::current_exe().ok();

    if let Some(path) = exe_path {
        let path_str = path.to_string_lossy().to_lowercase();

        if path_str.contains("homebrew") || path_str.contains("/opt/homebrew") {
            return "homebrew".to_string();
        }
        if path_str.contains(".cargo") {
            return "cargo".to_string();
        }
        if path_str.contains("applications") || path_str.contains(".app") {
            return "desktop".to_string();
        }
    }

    if std::env::var("GOOSE_DESKTOP").is_ok() {
        return "desktop".to_string();
    }

    "binary".to_string()
}

fn is_dev_mode() -> bool {
    cfg!(debug_assertions)
}

// ============================================================================
// Session Context (set by CLI/Desktop at startup)
// ============================================================================

static SESSION_INTERFACE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static SESSION_IS_RESUMED: AtomicBool = AtomicBool::new(false);

pub fn set_session_context(interface: &str, is_resumed: bool) {
    if let Ok(mut iface) = SESSION_INTERFACE.lock() {
        *iface = Some(interface.to_string());
    }
    SESSION_IS_RESUMED.store(is_resumed, Ordering::Relaxed);
}

fn get_session_interface() -> String {
    SESSION_INTERFACE
        .lock()
        .ok()
        .and_then(|i| i.clone())
        .unwrap_or_else(|| "unknown".to_string())
}

fn get_session_is_resumed() -> bool {
    SESSION_IS_RESUMED.load(Ordering::Relaxed)
}

// ============================================================================
// Property Helpers
// ============================================================================

fn insert(
    props: &mut HashMap<String, serde_json::Value>,
    key: &str,
    val: impl Into<serde_json::Value>,
) {
    props.insert(key.to_string(), val.into());
}

// ============================================================================
// Telemetry Events
// ============================================================================

pub fn emit_session_started() {
    if !is_telemetry_enabled() {
        return;
    }

    let installation = increment_session_count();

    tokio::spawn(async move {
        let _ = send_session_event(&installation).await;
    });
}

#[derive(Default, Clone)]
pub struct ErrorContext {
    pub component: Option<String>,
    pub action: Option<String>,
    pub error_message: Option<String>,
}

pub fn emit_error(error_type: &str, error_message: &str) {
    emit_error_with_context(
        error_type,
        ErrorContext {
            error_message: Some(error_message.to_string()),
            ..Default::default()
        },
    );
}

pub fn emit_error_with_context(error_type: &str, context: ErrorContext) {
    if !is_telemetry_enabled() {
        return;
    }

    // Temporarily disabled - only session_started events are sent
    let _ = (&error_type, &context);
    return;

    #[allow(unreachable_code)]
    let installation = load_or_create_installation();
    let error_type = error_type.to_string();

    tokio::spawn(async move {
        let _ = send_error_event(&installation, &error_type, context).await;
    });
}

pub fn emit_custom_slash_command_used() {
    if !is_telemetry_enabled() {
        return;
    }

    // Temporarily disabled - only session_started events are sent
    return;

    #[allow(unreachable_code)]
    let installation = load_or_create_installation();

    tokio::spawn(async move {
        let _ = send_custom_slash_command_event(&installation).await;
    });
}

async fn send_error_event(
    installation: &InstallationData,
    error_type: &str,
    context: ErrorContext,
) -> Result<(), String> {
    let mut props = HashMap::new();

    insert(&mut props, "error_type", error_type);
    insert(&mut props, "error_category", classify_error(error_type));
    insert(&mut props, "source", "backend");
    insert(&mut props, "version", env!("CARGO_PKG_VERSION"));
    insert(&mut props, "interface", get_session_interface());
    insert(&mut props, "os", std::env::consts::OS);
    insert(&mut props, "arch", std::env::consts::ARCH);

    if let Some(component) = &context.component {
        insert(&mut props, "component", component.as_str());
    }
    if let Some(action) = &context.action {
        insert(&mut props, "action", action.as_str());
    }
    if let Some(error_message) = &context.error_message {
        insert(&mut props, "error_message", sanitize_string(error_message));
    }

    if let Some(platform_version) = get_platform_version() {
        insert(&mut props, "platform_version", platform_version);
    }

    let config = Config::global();
    if let Ok(provider) = config.get_goose_provider() {
        insert(&mut props, "provider", provider);
    }
    if let Ok(model) = config.get_goose_model() {
        insert(&mut props, "model", model);
    }

    posthog_capture("error", &installation.installation_id, props).await
}

async fn send_custom_slash_command_event(installation: &InstallationData) -> Result<(), String> {
    let mut props = HashMap::new();

    insert(&mut props, "source", "backend");
    insert(&mut props, "version", env!("CARGO_PKG_VERSION"));
    insert(&mut props, "interface", get_session_interface());
    insert(&mut props, "os", std::env::consts::OS);
    insert(&mut props, "arch", std::env::consts::ARCH);

    if let Some(platform_version) = get_platform_version() {
        insert(&mut props, "platform_version", platform_version);
    }

    posthog_capture(
        "custom_slash_command_used",
        &installation.installation_id,
        props,
    )
    .await
}

async fn send_session_event(installation: &InstallationData) -> Result<(), String> {
    let mut props = HashMap::new();

    insert(&mut props, "os", std::env::consts::OS);
    insert(&mut props, "arch", std::env::consts::ARCH);
    insert(&mut props, "version", env!("CARGO_PKG_VERSION"));
    insert(&mut props, "is_dev", is_dev_mode());

    if let Some(platform_version) = get_platform_version() {
        insert(&mut props, "platform_version", platform_version);
    }

    insert(&mut props, "install_method", detect_install_method());
    insert(&mut props, "interface", get_session_interface());
    insert(&mut props, "is_resumed", get_session_is_resumed());
    insert(&mut props, "session_number", installation.session_count);

    let days_since_install = (Utc::now() - installation.first_seen).num_days();
    insert(&mut props, "days_since_install", days_since_install);

    let config = Config::global();
    if let Ok(provider) = config.get_goose_provider() {
        insert(&mut props, "provider", provider);
    }
    if let Ok(model) = config.get_goose_model() {
        insert(&mut props, "model", model);
    }

    if let Ok(mode) = config.get_param::<String>("GOOSE_MODE") {
        insert(&mut props, "setting_mode", mode);
    }
    if let Ok(max_turns) = config.get_param::<i64>("GOOSE_MAX_TURNS") {
        insert(&mut props, "setting_max_turns", max_turns);
    }

    let extensions = get_enabled_extensions();
    insert(&mut props, "extensions_count", extensions.len() as u64);
    let extension_names: Vec<String> = extensions.iter().map(|e| e.name()).collect();
    insert(
        &mut props,
        "extensions",
        serde_json::Value::Array(
            extension_names
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );

    insert(
        &mut props,
        "db_schema_version",
        CURRENT_SCHEMA_VERSION as u64,
    );

    let session_manager = SessionManager::instance();
    if let Ok(insights) = session_manager.get_insights().await {
        insert(&mut props, "total_sessions", insights.total_sessions as u64);
        insert(&mut props, "total_tokens", insights.total_tokens as u64);
    }

    posthog_capture("session_started", &installation.installation_id, props).await
}

// ============================================================================
// Error Classification
// ============================================================================
pub fn classify_error(error: &str) -> &'static str {
    let error_lower = error.to_lowercase();

    if error_lower.contains("network") || error_lower.contains("fetch") {
        return "network_error";
    }
    if error_lower.contains("timeout") {
        return "timeout";
    }
    if error_lower.contains("rate") && error_lower.contains("limit") {
        return "rate_limit";
    }
    if error_lower.contains("auth")
        || error_lower.contains("unauthorized")
        || error_lower.contains("401")
    {
        return "auth_error";
    }
    if error_lower.contains("permission") || error_lower.contains("403") {
        return "permission_error";
    }
    if error_lower.contains("not found") || error_lower.contains("404") {
        return "not_found";
    }
    if error_lower.contains("provider") {
        return "provider_error";
    }
    if error_lower.contains("config") {
        return "config_error";
    }
    if error_lower.contains("extension") {
        return "extension_error";
    }
    if error_lower.contains("database") || error_lower.contains("db") || error_lower.contains("sql")
    {
        return "database_error";
    }
    if error_lower.contains("migration") {
        return "migration_error";
    }
    if error_lower.contains("render") || error_lower.contains("react") {
        return "render_error";
    }
    if error_lower.contains("chunk") || error_lower.contains("module") {
        return "module_error";
    }

    "unknown_error"
}

// ============================================================================
// Privacy Sanitization
// ============================================================================

use regex::Regex;
use std::sync::LazyLock;

static SENSITIVE_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"/Users/[^/\s]+").unwrap(),
        Regex::new(r"/home/[^/\s]+").unwrap(),
        Regex::new(r"(?i)C:\\Users\\[^\\\s]+").unwrap(),
        Regex::new(r"sk-[a-zA-Z0-9]{20,}").unwrap(),
        Regex::new(r"pk-[a-zA-Z0-9]{20,}").unwrap(),
        Regex::new(r"(?i)key[_-]?[a-zA-Z0-9]{16,}").unwrap(),
        Regex::new(r"(?i)token[_-]?[a-zA-Z0-9]{16,}").unwrap(),
        Regex::new(r"(?i)bearer\s+[a-zA-Z0-9._-]+").unwrap(),
        Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
        Regex::new(r"https?://[^:]+:[^@]+@").unwrap(),
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap(),
    ]
});

fn sanitize_string(s: &str) -> String {
    let mut result = s.to_string();
    for pattern in SENSITIVE_PATTERNS.iter() {
        result = pattern.replace_all(&result, "[REDACTED]").to_string();
    }
    result
}

fn sanitize_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(sanitize_string(&s)),
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(sanitize_value).collect())
        }
        serde_json::Value::Object(obj) => serde_json::Value::Object(
            obj.into_iter()
                .map(|(k, v)| (k, sanitize_value(v)))
                .collect(),
        ),
        other => other,
    }
}

// ============================================================================
// Generic Event API (for frontend)
// ============================================================================
pub async fn emit_event(
    event_name: &str,
    mut properties: HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    // Only onboarding events are enabled for now. These bypass the telemetry
    // check so we can track the funnel before the user makes their choice.
    let is_onboarding_event =
        event_name.starts_with("onboarding_") || event_name == "telemetry_preference_set";
    if !is_onboarding_event {
        return Ok(());
    }

    let installation = load_or_create_installation();

    insert(&mut properties, "os", std::env::consts::OS);
    insert(&mut properties, "arch", std::env::consts::ARCH);
    insert(&mut properties, "version", env!("CARGO_PKG_VERSION"));
    insert(&mut properties, "interface", "desktop");
    insert(&mut properties, "source", "ui");

    if let Some(platform_version) = get_platform_version() {
        insert(&mut properties, "platform_version", platform_version);
    }

    if event_name == "error_occurred" || event_name == "app_crashed" {
        if let Some(serde_json::Value::String(error_type)) = properties.get("error_type") {
            let classified = classify_error(error_type);
            properties.insert(
                "error_category".to_string(),
                serde_json::Value::String(classified.to_string()),
            );
        }
    }

    let sanitized: HashMap<String, serde_json::Value> = properties
        .into_iter()
        .filter(|(key, _)| {
            let key_lower = key.to_lowercase();
            !key_lower.contains("key")
                && !key_lower.contains("token")
                && !key_lower.contains("secret")
                && !key_lower.contains("password")
                && !key_lower.contains("credential")
        })
        .map(|(k, v)| (k, sanitize_value(v)))
        .collect();

    posthog_capture(event_name, &installation.installation_id, sanitized).await
}
