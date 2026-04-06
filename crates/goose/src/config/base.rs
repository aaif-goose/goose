use crate::config::paths::Paths;
use crate::config::GooseMode;
use fs2::FileExt;
use keyring::Entry;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_yaml::Mapping;
use std::collections::HashMap;
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

fn write_secrets_file(path: &Path, content: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;

        file.write_all(content.as_bytes())
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
    }
}

const KEYRING_SERVICE: &str = "goose";
const KEYRING_USERNAME: &str = "secrets";
pub const CONFIG_YAML_NAME: &str = "config.yaml";

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration value not found: {0}")]
    NotFound(String),
    #[error("Failed to deserialize value: {0}")]
    DeserializeError(String),
    #[error("Failed to read config file: {0}")]
    FileError(#[from] std::io::Error),
    #[error("Failed to create config directory: {0}")]
    DirectoryError(String),
    #[error("Failed to access keyring: {0}")]
    KeyringError(String),
    #[error("Failed to lock config file: {0}")]
    LockError(String),
    #[error("Secret stored using file-based fallback")]
    FallbackToFileStorage,
}

impl From<serde_json::Error> for ConfigError {
    fn from(err: serde_json::Error) -> Self {
        ConfigError::DeserializeError(err.to_string())
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(err: serde_yaml::Error) -> Self {
        ConfigError::DeserializeError(err.to_string())
    }
}

impl From<keyring::Error> for ConfigError {
    fn from(err: keyring::Error) -> Self {
        ConfigError::KeyringError(err.to_string())
    }
}

/// Configuration management for goose.
///
/// This module provides a flexible configuration system that supports:
/// - Dynamic configuration keys
/// - Multiple value types through serde deserialization
/// - Environment variable overrides
/// - YAML-based configuration file storage
/// - Hot reloading of configuration changes
/// - Secure secret storage in system keyring
///
/// Configuration values are loaded with the following precedence:
/// 1. Environment variables (exact key match)
/// 2. Configuration file (~/.config/goose/config.yaml by default)
///
/// Secrets are loaded with the following precedence:
/// 1. Environment variables (exact key match)
/// 2. System keyring (which can be disabled with GOOSE_DISABLE_KEYRING)
/// 3. If the keyring is disabled, secrets are stored in a secrets file
///    (~/.config/goose/secrets.yaml by default)
///
/// # Examples
///
/// ```no_run
/// use goose::config::Config;
/// use serde::Deserialize;
///
/// // Get a string value
/// let config = Config::global();
/// let api_key: String = config.get_param("OPENAI_API_KEY").unwrap();
///
/// // Get a complex type
/// #[derive(Deserialize)]
/// struct ServerConfig {
///     host: String,
///     port: u16,
/// }
///
/// let server_config: ServerConfig = config.get_param("server").unwrap();
/// ```
///
/// # Naming Convention
/// we recommend snake_case for keys, and will convert to UPPERCASE when
/// checking for environment overrides. e.g. openai_api_key will check for an
/// environment variable OPENAI_API_KEY
///
/// For goose-specific configuration, consider prefixing with "goose_" to avoid conflicts.
pub struct Config {
    config_path: PathBuf,
    defaults_path: Option<PathBuf>,
    secrets: SecretStorage,
    guard: Mutex<()>,
    secrets_cache: Arc<Mutex<Option<HashMap<String, Value>>>>,
}

enum SecretStorage {
    Keyring { service: String },
    File { path: PathBuf },
}

// Global instance
static GLOBAL_CONFIG: OnceCell<Config> = OnceCell::new();

impl Default for Config {
    fn default() -> Self {
        let config_dir = Paths::config_dir();

        let config_path = config_dir.join(CONFIG_YAML_NAME);

        let defaults_path = find_workspace_or_exe_root().and_then(|root| {
            let path = root.join("defaults.yaml");
            if path.exists() {
                tracing::info!("Found bundled defaults.yaml at: {:?}", path);
                Some(path)
            } else {
                None
            }
        });

        let secrets = if env::var("GOOSE_DISABLE_KEYRING").is_ok()
            || keyring_disabled_in_config(&config_path)
        {
            SecretStorage::File {
                path: config_dir.join("secrets.yaml"),
            }
        } else {
            SecretStorage::Keyring {
                service: KEYRING_SERVICE.to_string(),
            }
        };
        Config {
            config_path,
            defaults_path,
            secrets,
            guard: Mutex::new(()),
            secrets_cache: Arc::new(Mutex::new(None)),
        }
    }
}

pub trait ConfigValue {
    const KEY: &'static str;
    const DEFAULT: &'static str;
}

macro_rules! config_value {
    ($key:ident, $type:ty) => {
        impl Config {
            pastey::paste! {
                pub fn [<get_ $key:lower>](&self) -> Result<$type, ConfigError> {
                    self.get_param(stringify!($key))
                }
            }
            pastey::paste! {
                pub fn [<set_ $key:lower>](&self, v: impl Into<$type>) -> Result<(), ConfigError> {
                    self.set_param(stringify!($key), &v.into())
                }
            }
        }
    };

    ($key:ident, $inner:ty, $default:expr) => {
        pastey::paste! {
            #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
            #[serde(transparent)]
            pub struct [<$key:camel>]($inner);

            impl ConfigValue for [<$key:camel>] {
                const KEY: &'static str = stringify!($key);
                const DEFAULT: &'static str = $default;
            }

            impl Default for [<$key:camel>] {
                fn default() -> Self {
                    [<$key:camel>]($default.into())
                }
            }

            impl std::ops::Deref for [<$key:camel>] {
                type Target = $inner;

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            impl std::ops::DerefMut for [<$key:camel>] {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self.0
                }
            }

            impl std::fmt::Display for [<$key:camel>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}", self.0)
                }
            }

            impl From<$inner> for [<$key:camel>] {
                fn from(value: $inner) -> Self {
                    [<$key:camel>](value)
                }
            }

            impl From<[<$key:camel>]> for $inner {
                fn from(value: [<$key:camel>]) -> $inner {
                    value.0
                }
            }

            config_value!($key, [<$key:camel>]);
        }
    };
}

fn parse_yaml_content(content: &str) -> Result<Mapping, ConfigError> {
    serde_yaml::from_str(content).map_err(|e| e.into())
}

/// Read the GOOSE_DISABLE_KEYRING flag from the config file.
///
/// Called before Config is fully initialised, so we do a minimal raw read
/// rather than going through `get_param`.  All errors are treated as `false`
/// (keyring stays enabled) so a missing/malformed file is never fatal here.
fn keyring_disabled_in_config(config_path: &Path) -> bool {
    std::fs::read_to_string(config_path)
        .ok()
        .and_then(|s| parse_yaml_content(&s).ok())
        .and_then(|m| {
            m.get("GOOSE_DISABLE_KEYRING")
                .map(|v| v.as_bool().unwrap_or(false) || v.as_str().is_some_and(|s| s == "true"))
        })
        .unwrap_or(false)
}

impl Config {
    /// Get the global configuration instance.
    ///
    /// This will initialize the configuration with the default path (~/.config/goose/config.yaml)
    /// if it hasn't been initialized yet.
    pub fn global() -> &'static Config {
        GLOBAL_CONFIG.get_or_init(Config::default)
    }

    /// Create a new configuration instance with custom paths
    ///
    /// This is primarily useful for testing or for applications that need
    /// to manage multiple configuration files.
    pub fn new<P: AsRef<Path>>(config_path: P, service: &str) -> Result<Self, ConfigError> {
        Ok(Config {
            config_path: config_path.as_ref().to_path_buf(),
            defaults_path: None,
            secrets: SecretStorage::Keyring {
                service: service.to_string(),
            },
            guard: Mutex::new(()),
            secrets_cache: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a new configuration instance with custom paths
    ///
    /// This is primarily useful for testing or for applications that need
    /// to manage multiple configuration files.
    pub fn new_with_file_secrets<P1: AsRef<Path>, P2: AsRef<Path>>(
        config_path: P1,
        secrets_path: P2,
    ) -> Result<Self, ConfigError> {
        Ok(Config {
            config_path: config_path.as_ref().to_path_buf(),
            defaults_path: None,
            secrets: SecretStorage::File {
                path: secrets_path.as_ref().to_path_buf(),
            },
            guard: Mutex::new(()),
            secrets_cache: Arc::new(Mutex::new(None)),
        })
    }

    pub fn new_with_defaults<P1: AsRef<Path>, P2: AsRef<Path>, P3: AsRef<Path>>(
        config_path: P1,
        secrets_path: P2,
        defaults_path: P3,
    ) -> Result<Self, ConfigError> {
        Ok(Config {
            config_path: config_path.as_ref().to_path_buf(),
            defaults_path: Some(defaults_path.as_ref().to_path_buf()),
            secrets: SecretStorage::File {
                path: secrets_path.as_ref().to_path_buf(),
            },
            guard: Mutex::new(()),
            secrets_cache: Arc::new(Mutex::new(None)),
        })
    }

    pub fn exists(&self) -> bool {
        self.config_path.exists()
    }

    pub fn clear(&self) -> Result<(), ConfigError> {
        Ok(std::fs::remove_file(&self.config_path)?)
    }

    pub fn path(&self) -> String {
        self.config_path.to_string_lossy().to_string()
    }

    fn load_raw(&self) -> Result<Mapping, ConfigError> {
        let mut values = if self.config_path.exists() {
            self.load_values_with_recovery()?
        } else {
            // Config file doesn't exist, try to recover from backup first
            tracing::info!("Config file doesn't exist, attempting recovery from backup");

            if let Ok(backup_values) = self.try_restore_from_backup() {
                tracing::info!("Successfully restored config from backup");
                backup_values
            } else {
                // No backup available, create a default config
                tracing::info!("No backup found, creating default configuration");
                let default_config = self.load_init_config_if_exists().unwrap_or_default();
                self.create_and_save_default_config(default_config)?
            }
        };

        // Run migrations on the loaded config
        if crate::config::migrations::run_migrations(&mut values) {
            if let Err(e) = self.save_values(&values) {
                tracing::warn!("Failed to save migrated config: {}", e);
            }
        }

        Ok(values)
    }

    fn load(&self) -> Result<Mapping, ConfigError> {
        let mut values = self.load_raw()?;
        self.merge_missing_defaults(&mut values);
        Ok(values)
    }

    pub fn all_values(&self) -> Result<HashMap<String, Value>, ConfigError> {
        let config_values = self.load()?;
        Ok(HashMap::from_iter(config_values.into_iter().filter_map(
            |(k, v)| {
                k.as_str()
                    .map(|k| k.to_string())
                    .zip(serde_json::to_value(v).ok())
            },
        )))
    }

    // Helper method to create and save default config with consistent logging
    fn create_and_save_default_config(
        &self,
        default_config: Mapping,
    ) -> Result<Mapping, ConfigError> {
        // Try to write the default config to disk
        match self.save_values(&default_config) {
            Ok(_) => {
                if default_config.is_empty() {
                    tracing::info!("Created fresh empty config file");
                } else {
                    tracing::info!(
                        "Created fresh config file from init-config.yaml with {} keys",
                        default_config.len()
                    );
                }
                Ok(default_config)
            }
            Err(write_error) => {
                tracing::error!("Failed to write default config file: {}", write_error);
                // Even if we can't write to disk, return config so app can still run
                Ok(default_config)
            }
        }
    }

    fn load_values_with_recovery(&self) -> Result<Mapping, ConfigError> {
        let file_content = std::fs::read_to_string(&self.config_path)?;

        match parse_yaml_content(&file_content) {
            Ok(values) => Ok(values),
            Err(parse_error) => {
                tracing::warn!(
                    "Config file appears corrupted, attempting recovery: {}",
                    parse_error
                );

                // Try to recover from backup
                if let Ok(backup_values) = self.try_restore_from_backup() {
                    tracing::info!("Successfully restored config from backup");
                    return Ok(backup_values);
                }

                // Last resort: create a fresh default config file
                tracing::error!("Could not recover config file, creating fresh default configuration. Original error: {}", parse_error);
                let default_config = self.load_init_config_if_exists().unwrap_or_default();
                self.create_and_save_default_config(default_config)
            }
        }
    }

    fn try_restore_from_backup(&self) -> Result<Mapping, ConfigError> {
        let backup_paths = self.get_backup_paths();

        for backup_path in backup_paths {
            if backup_path.exists() {
                match std::fs::read_to_string(&backup_path) {
                    Ok(backup_content) => {
                        match parse_yaml_content(&backup_content) {
                            Ok(values) => {
                                // Successfully parsed backup, restore it as the main config
                                if let Err(e) = self.save_values(&values) {
                                    tracing::warn!(
                                        "Failed to restore backup as main config: {}",
                                        e
                                    );
                                } else {
                                    tracing::info!(
                                        "Restored config from backup: {:?}",
                                        backup_path
                                    );
                                }
                                return Ok(values);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Backup file {:?} is also corrupted: {}",
                                    backup_path,
                                    e
                                );
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Could not read backup file {:?}: {}", backup_path, e);
                        continue;
                    }
                }
            }
        }

        Err(ConfigError::NotFound("No valid backup found".to_string()))
    }

    // Get list of backup file paths in order of preference
    fn get_backup_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Primary backup (created by backup_config endpoint)
        if let Some(file_name) = self.config_path.file_name() {
            let mut backup_name = file_name.to_os_string();
            backup_name.push(".bak");
            paths.push(self.config_path.with_file_name(backup_name));
        }

        paths
    }

    fn load_init_config_if_exists(&self) -> Result<Mapping, ConfigError> {
        load_init_config_from_workspace()
    }

    fn config_write_target_path(&self) -> Result<PathBuf, ConfigError> {
        let mut path = self.config_path.clone();

        // Follow symlinks so we update the target file without replacing the link itself.
        const MAX_SYMLINK_HOPS: usize = 1;
        let mut hops = 0usize;
        loop {
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    if hops >= MAX_SYMLINK_HOPS {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!(
                                "Too many symlink levels (or a cycle) while resolving config path: {:?}",
                                self.config_path
                            ),
                        )
                        .into());
                    }
                    hops += 1;

                    let link = std::fs::read_link(&path)?;
                    path = if link.is_absolute() {
                        link
                    } else {
                        path.parent().unwrap_or_else(|| Path::new(".")).join(link)
                    };
                }
                Ok(_) => return Ok(path),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(path),
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn save_values(&self, values: &Mapping) -> Result<(), ConfigError> {
        // Create backup before writing new config
        self.create_backup_if_needed()?;

        let target_path = self.config_write_target_path()?;

        // Convert to YAML for storage
        let yaml_value = serde_yaml::to_string(values)?;

        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ConfigError::DirectoryError(e.to_string()))?;
        }

        // Write to a temporary file first for atomic operation
        let temp_path = target_path.with_extension("tmp");

        {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temp_path)?;

            // Acquire an exclusive lock
            file.lock_exclusive()
                .map_err(|e| ConfigError::LockError(e.to_string()))?;

            // Write the contents using the same file handle
            file.write_all(yaml_value.as_bytes())?;
            file.sync_all()?;

            // Unlock is handled automatically when file is dropped
        }

        // Atomically replace the original file
        std::fs::rename(&temp_path, &target_path)?;

        Ok(())
    }

    pub fn initialize_if_empty(&self, values: Mapping) -> Result<(), ConfigError> {
        let _guard = self.guard.lock().unwrap();
        if !self.exists() {
            self.save_values(&values)
        } else {
            Ok(())
        }
    }

    // Create backup of current config file if it exists and is valid
    fn create_backup_if_needed(&self) -> Result<(), ConfigError> {
        if !self.config_path.exists() {
            return Ok(());
        }

        // Check if current config is valid before backing it up
        let current_content = std::fs::read_to_string(&self.config_path)?;
        if parse_yaml_content(&current_content).is_err() {
            // Don't back up corrupted files
            return Ok(());
        }

        // Create new backup
        if let Some(file_name) = self.config_path.file_name() {
            let mut backup_name = file_name.to_os_string();
            backup_name.push(".bak");
            let backup_path = self.config_path.with_file_name(backup_name);

            if let Err(e) = std::fs::copy(&self.config_path, &backup_path) {
                tracing::warn!("Failed to create config backup: {}", e);
                // Don't fail the entire operation if backup fails
            } else {
                tracing::debug!("Created config backup: {:?}", backup_path);
            }
        }

        Ok(())
    }

    pub fn all_secrets(&self) -> Result<HashMap<String, Value>, ConfigError> {
        let mut cache = self.secrets_cache.lock().unwrap();

        let values = if let Some(ref cached_secrets) = *cache {
            cached_secrets.clone()
        } else {
            tracing::debug!("secrets cache miss, fetching from storage");

            let loaded = match &self.secrets {
                SecretStorage::Keyring { service } => {
                    let result =
                        self.handle_keyring_operation(|entry| entry.get_password(), service, None);

                    match result {
                        Ok(content) => {
                            let values: HashMap<String, Value> = serde_json::from_str(&content)?;
                            values
                        }
                        Err(ConfigError::FallbackToFileStorage) => {
                            self.fallback_to_file_storage()?
                        }
                        Err(ConfigError::KeyringError(msg))
                            if msg.contains("No entry found")
                                || msg.contains("No matching entry found") =>
                        {
                            HashMap::new()
                        }
                        Err(e) => return Err(e),
                    }
                }
                SecretStorage::File { path } => self.read_secrets_from_file(path)?,
            };

            *cache = Some(loaded.clone());
            loaded
        };

        Ok(values)
    }

    /// Parse an environment variable value into a JSON Value.
    ///
    /// This function tries to intelligently parse environment variable values:
    /// 1. First attempts JSON parsing (for structured data)
    /// 2. If that fails, tries primitive type parsing for common cases
    /// 3. Falls back to string if nothing else works
    fn parse_env_value(val: &str) -> Result<Value, ConfigError> {
        // First try JSON parsing - this handles quoted strings, objects, arrays, etc.
        if let Ok(json_value) = serde_json::from_str(val) {
            return Ok(json_value);
        }

        let trimmed = val.trim();

        match trimmed.to_lowercase().as_str() {
            "true" => return Ok(Value::Bool(true)),
            "false" => return Ok(Value::Bool(false)),
            _ => {}
        }

        if let Ok(int_val) = trimmed.parse::<i64>() {
            return Ok(Value::Number(int_val.into()));
        }

        if let Ok(float_val) = trimmed.parse::<f64>() {
            if let Some(num) = serde_json::Number::from_f64(float_val) {
                return Ok(Value::Number(num));
            }
        }

        Ok(Value::String(val.to_string()))
    }

    // check all possible places for a parameter
    pub fn get(&self, key: &str, is_secret: bool) -> Result<Value, ConfigError> {
        if is_secret {
            self.get_secret(key)
        } else {
            self.get_param(key)
        }
    }

    // save a parameter in the appropriate location based on if it's secret or not
    pub fn set<V>(&self, key: &str, value: &V, is_secret: bool) -> Result<(), ConfigError>
    where
        V: Serialize,
    {
        if is_secret {
            self.set_secret(key, value)
        } else {
            self.set_param(key, value)
        }
    }

    /// Get a configuration value (non-secret).
    ///
    /// This will attempt to get the value from (in order):
    /// 1. Environment variable with the uppercase key name
    /// 2. Configuration file (~/.config/goose/config.yaml)
    /// 3. Bundled defaults file (defaults.yaml in workspace root or executable directory)
    ///
    /// The value will be deserialized into the requested type. This works with
    /// both simple types (String, i32, etc.) and complex types that implement
    /// serde::Deserialize.
    ///
    /// # Errors
    ///
    /// Returns a ConfigError if:
    /// - The key doesn't exist in any of the above sources
    /// - The value cannot be deserialized into the requested type
    /// - There is an error reading the config file
    pub fn get_param<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<T, ConfigError> {
        let env_key = key.to_uppercase();
        if let Ok(val) = env::var(&env_key) {
            let value = Self::parse_env_value(&val)?;
            return Ok(serde_json::from_value(value)?);
        }

        let values = self.load()?;
        values
            .get(key)
            .ok_or_else(|| ConfigError::NotFound(key.to_string()))
            .and_then(|v| Ok(serde_yaml::from_value(v.clone())?))
    }

    fn load_defaults(&self) -> Option<Mapping> {
        let path = self.defaults_path.as_ref()?;
        let content = std::fs::read_to_string(path).ok()?;
        parse_yaml_content(&content).ok()
    }

    fn merge_missing_defaults(&self, values: &mut Mapping) {
        let Some(defaults) = self.load_defaults() else {
            return;
        };

        for (key, default_value) in defaults {
            if !values.contains_key(&key) {
                values.insert(key, default_value);
            }
        }
    }

    /// Set a configuration value in the config file (non-secret).
    ///
    /// This will immediately write the value to the config file. The value
    /// can be any type that can be serialized to JSON/YAML.
    ///
    /// Note that this does not affect environment variables - those can only
    /// be set through the system environment.
    ///
    /// # Errors
    ///
    /// Returns a ConfigError if:
    /// - There is an error reading or writing the config file
    /// - There is an error serializing the value
    pub fn set_param<V: Serialize>(&self, key: &str, value: V) -> Result<(), ConfigError> {
        let _guard = self.guard.lock().unwrap();
        let mut values = self.load_raw()?;
        values.insert(serde_yaml::to_value(key)?, serde_yaml::to_value(value)?);
        self.save_values(&values)
    }

    /// Delete a configuration value in the config file.
    ///
    /// This will immediately write the value to the config file. The value
    /// can be any type that can be serialized to JSON/YAML.
    ///
    /// Note that this does not affect environment variables - those can only
    /// be set through the system environment.
    ///
    /// # Errors
    ///
    /// Returns a ConfigError if:
    /// - There is an error reading or writing the config file
    /// - There is an error serializing the value
    pub fn delete(&self, key: &str) -> Result<(), ConfigError> {
        // Lock before reading to prevent race condition.
        let _guard = self.guard.lock().unwrap();

        let mut values = self.load_raw()?;
        values.shift_remove(key);

        self.save_values(&values)
    }

    /// Get a secret value.
    ///
    /// This will attempt to get the value from:
    /// 1. Environment variable with the exact key name
    /// 2. System keyring
    ///
    /// The value will be deserialized into the requested type. This works with
    /// both simple types (String, i32, etc.) and complex types that implement
    /// serde::Deserialize.
    ///
    /// # Errors
    ///
    /// Returns a ConfigError if:
    /// - The key doesn't exist in either environment or keyring
    /// - The value cannot be deserialized into the requested type
    /// - There is an error accessing the keyring
    pub fn get_secret<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<T, ConfigError> {
        // First check environment variables (convert to uppercase)
        let env_key = key.to_uppercase();
        if let Ok(val) = env::var(&env_key) {
            let value = Self::parse_env_value(&val)?;
            return Ok(serde_json::from_value(value)?);
        }

        // Then check keyring
        let values = self.all_secrets()?;
        values
            .get(key)
            .ok_or_else(|| ConfigError::NotFound(key.to_string()))
            .and_then(|v| Ok(serde_json::from_value(v.clone())?))
    }

    /// Get secrets. If primary is in env, use env for all keys. Otherwise, use secret storage.
    pub fn get_secrets(
        &self,
        primary: &str,
        maybe_secret: &[&str],
    ) -> Result<HashMap<String, String>, ConfigError> {
        let use_env = env::var(primary.to_uppercase()).is_ok();
        let get_value = |key: &str| -> Result<String, ConfigError> {
            if use_env {
                env::var(key.to_uppercase()).map_err(|_| ConfigError::NotFound(key.to_string()))
            } else {
                self.get_secret(key)
            }
        };

        let mut result = HashMap::new();
        result.insert(primary.to_string(), get_value(primary)?);
        for &key in maybe_secret {
            if let Ok(v) = get_value(key) {
                result.insert(key.to_string(), v);
            }
        }
        Ok(result)
    }

    /// Set a secret value in the system keyring.
    ///
    /// This will store the value in a single JSON object in the system keyring,
    /// alongside any other secrets. The value can be any type that can be
    /// serialized to JSON.
    ///
    /// Note that this does not affect environment variables - those can only
    /// be set through the system environment.
    ///
    /// # Errors
    ///
    /// Returns a ConfigError if:
    /// - There is an error accessing the keyring
    /// - There is an error serializing the value
    pub fn set_secret<V>(&self, key: &str, value: &V) -> Result<(), ConfigError>
    where
        V: Serialize,
    {
        // Lock before reading to prevent race condition.
        let _guard = self.guard.lock().unwrap();

        let mut values = self.all_secrets()?;
        values.insert(key.to_string(), serde_json::to_value(value)?);

        match &self.secrets {
            SecretStorage::Keyring { service } => {
                let json_value = serde_json::to_string(&values)?;
                match self.handle_keyring_operation(
                    |entry| entry.set_password(&json_value),
                    service,
                    Some(&values),
                ) {
                    Ok(_) => {}
                    Err(ConfigError::FallbackToFileStorage) => {}
                    Err(e) => return Err(e),
                }
            }
            SecretStorage::File { path } => {
                let yaml_value = serde_yaml::to_string(&values)?;
                write_secrets_file(path, &yaml_value)?;
            }
        };

        self.invalidate_secrets_cache();

        Ok(())
    }

    /// Delete a secret from the system keyring.
    ///
    /// This will remove the specified key from the JSON object in the system keyring.
    /// Other secrets will remain unchanged.
    ///
    /// # Errors
    ///
    /// Returns a ConfigError if:
    /// - There is an error accessing the keyring
    /// - There is an error serializing the remaining values
    pub fn delete_secret(&self, key: &str) -> Result<(), ConfigError> {
        // Lock before reading to prevent race condition.
        let _guard = self.guard.lock().unwrap();

        let mut values = self.all_secrets()?;
        values.remove(key);

        match &self.secrets {
            SecretStorage::Keyring { service } => {
                let json_value = serde_json::to_string(&values)?;
                match self.handle_keyring_operation(
                    |entry| entry.set_password(&json_value),
                    service,
                    Some(&values),
                ) {
                    Ok(_) => {}
                    Err(ConfigError::FallbackToFileStorage) => {}
                    Err(e) => return Err(e),
                }
            }
            SecretStorage::File { path } => {
                let yaml_value = serde_yaml::to_string(&values)?;
                write_secrets_file(path, &yaml_value)?;
            }
        };

        self.invalidate_secrets_cache();

        Ok(())
    }

    /// Read secrets from a YAML file
    fn read_secrets_from_file(&self, path: &Path) -> Result<HashMap<String, Value>, ConfigError> {
        if path.exists() {
            let file_content = std::fs::read_to_string(path)?;
            let yaml_value: serde_yaml::Value = serde_yaml::from_str(&file_content)?;
            let json_value: Value = serde_json::to_value(yaml_value)?;
            match json_value {
                Value::Object(map) => Ok(map.into_iter().collect()),
                _ => Ok(HashMap::new()),
            }
        } else {
            Ok(HashMap::new())
        }
    }

    /// Get the path to the secrets storage file
    fn secrets_file_path() -> PathBuf {
        Paths::config_dir().join("secrets.yaml")
    }

    /// Fall back to file storage when keyring is unavailable
    fn fallback_to_file_storage(&self) -> Result<HashMap<String, Value>, ConfigError> {
        let path = Self::secrets_file_path();
        self.read_secrets_from_file(&path)
    }

    /// Write secrets to file storage (used for fallback)
    fn write_secrets_to_file(&self, values: &HashMap<String, Value>) -> Result<(), ConfigError> {
        std::fs::create_dir_all(Paths::config_dir())?;
        let path = Self::secrets_file_path();
        let yaml_value = serde_yaml::to_string(values)?;
        write_secrets_file(&path, &yaml_value)?;
        Ok(())
    }

    pub fn invalidate_secrets_cache(&self) {
        let mut cache = self.secrets_cache.lock().unwrap();
        *cache = None;
    }

    /// Check if an error string indicates a keyring availability issue that should trigger fallback
    fn is_keyring_availability_error(&self, error_str: &str) -> bool {
        let lower = error_str.to_lowercase();
        lower.contains("keyring")
            || lower.contains("dbus")
            || lower.contains("org.freedesktop.secrets")
            || lower.contains("platform secure storage")
            || lower.contains("no secret service")
    }

    /// Get a keyring entry for the specified service
    fn get_keyring_entry(service: &str) -> Result<keyring::Entry, keyring::Error> {
        Entry::new(service, KEYRING_USERNAME)
    }

    /// Handle keyring errors with automatic fallback to file storage
    fn handle_keyring_fallback_error<T>(
        &self,
        keyring_err: &keyring::Error,
        fallback_values: Option<&HashMap<String, Value>>,
    ) -> Result<T, ConfigError> {
        if self.is_keyring_availability_error(&keyring_err.to_string()) {
            std::env::set_var("GOOSE_DISABLE_KEYRING", "1");
            tracing::warn!("Keyring unavailable. Using file storage for secrets.");

            if let Some(values) = fallback_values {
                self.write_secrets_to_file(values)?;
                Err(ConfigError::FallbackToFileStorage)
            } else {
                Err(ConfigError::FallbackToFileStorage)
            }
        } else {
            Err(ConfigError::KeyringError(keyring_err.to_string()))
        }
    }

    /// Handle keyring operation with automatic fallback to file storage
    fn handle_keyring_operation<T>(
        &self,
        operation: impl FnOnce(keyring::Entry) -> Result<T, keyring::Error>,
        service: &str,
        fallback_values: Option<&HashMap<String, Value>>,
    ) -> Result<T, ConfigError> {
        // Try to get the keyring entry and perform the operation
        let entry = match Self::get_keyring_entry(service) {
            Ok(entry) => entry,
            Err(keyring_err) => {
                return self.handle_keyring_fallback_error(&keyring_err, fallback_values);
            }
        };

        // Perform the operation
        match operation(entry) {
            Ok(result) => Ok(result),
            Err(keyring_err) => self.handle_keyring_fallback_error(&keyring_err, fallback_values),
        }
    }
}

config_value!(CLAUDE_CODE_COMMAND, String, "claude");
config_value!(GEMINI_CLI_COMMAND, String, "gemini");
config_value!(CURSOR_AGENT_COMMAND, String, "cursor-agent");
config_value!(CODEX_COMMAND, String, "codex");
config_value!(CODEX_REASONING_EFFORT, String, "high");
config_value!(CODEX_ENABLE_SKILLS, String, "true");
config_value!(CODEX_SKIP_GIT_CHECK, String, "false");
config_value!(CHATGPT_CODEX_REASONING_EFFORT, String, "medium");

config_value!(GOOSE_SEARCH_PATHS, Vec<String>);
config_value!(GOOSE_MODE, GooseMode);
config_value!(GOOSE_PROVIDER, String);
config_value!(GOOSE_MODEL, String);
config_value!(GOOSE_PROMPT_EDITOR, Option<String>);
config_value!(GOOSE_MAX_ACTIVE_AGENTS, usize);
config_value!(GOOSE_DISABLE_SESSION_NAMING, bool);
config_value!(GEMINI3_THINKING_LEVEL, String);
config_value!(CLAUDE_THINKING_TYPE, String);
config_value!(CLAUDE_THINKING_EFFORT, String);
config_value!(CLAUDE_THINKING_BUDGET, i32);
config_value!(GOOSE_DEFAULT_EXTENSION_TIMEOUT, u64);

fn find_workspace_or_exe_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?.to_path_buf();

    let mut path = exe;
    while let Some(parent) = path.parent() {
        let cargo_toml = parent.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return Some(parent.to_path_buf());
                }
            }
        }
        path = parent.to_path_buf();
    }

    Some(exe_dir)
}

pub fn load_init_config_from_workspace() -> Result<Mapping, ConfigError> {
    let root = find_workspace_or_exe_root().ok_or_else(|| {
        ConfigError::FileError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine executable path",
        ))
    })?;

    let init_config_path = root.join("init-config.yaml");
    if !init_config_path.exists() {
        return Err(ConfigError::NotFound(
            "init-config.yaml not found".to_string(),
        ));
    }

    let init_content = std::fs::read_to_string(&init_config_path)?;
    parse_yaml_content(&init_content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::{NamedTempFile, TempDir};
    #[test]
    fn test_basic_config() -> Result<(), ConfigError> {
        let config = new_test_config();

        // Set a simple string value
        config.set_param("test_key", "test_value")?;

        // Test simple string retrieval
        let value: String = config.get_param("test_key")?;
        assert_eq!(value, "test_value");

        // Test with environment variable override
        std::env::set_var("TEST_KEY", "env_value");
        let value: String = config.get_param("test_key")?;
        assert_eq!(value, "env_value");

        Ok(())
    }

    #[test]
    fn test_complex_type() -> Result<(), ConfigError> {
        #[derive(Deserialize, Debug, PartialEq)]
        struct TestStruct {
            field1: String,
            field2: i32,
        }

        let config = new_test_config();

        // Set a complex value
        config.set_param(
            "complex_key",
            serde_json::json!({
                "field1": "hello",
                "field2": 42
            }),
        )?;

        let value: TestStruct = config.get_param("complex_key")?;
        assert_eq!(value.field1, "hello");
        assert_eq!(value.field2, 42);

        Ok(())
    }

    #[test]
    fn test_missing_value() {
        let config = new_test_config();

        let result: Result<String, ConfigError> = config.get_param("nonexistent_key");
        assert!(matches!(result, Err(ConfigError::NotFound(_))));
    }

    #[test]
    fn test_yaml_formatting() -> Result<(), ConfigError> {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        let config = Config::new_with_file_secrets(config_file.path(), secrets_file.path())?;

        config.set_param("key1", "value1")?;
        config.set_param("key2", 42)?;

        // Read the file directly to check YAML formatting
        let content = std::fs::read_to_string(config_file.path())?;
        assert!(content.contains("key1: value1"));
        assert!(content.contains("key2: 42"));

        Ok(())
    }

    #[test]
    fn test_value_management() -> Result<(), ConfigError> {
        let config = new_test_config();

        config.set_param("test_key", "test_value")?;
        config.set_param("another_key", 42)?;
        config.set_param("third_key", true)?;

        let _values = config.load()?;

        let result: Result<String, ConfigError> = config.get_param("key");
        assert!(matches!(result, Err(ConfigError::NotFound(_))));

        Ok(())
    }

    #[test]
    fn test_file_based_secrets_management() -> Result<(), ConfigError> {
        let config = new_test_config();

        config.set_secret("key", &"value")?;

        let value: String = config.get_secret("key")?;
        assert_eq!(value, "value");

        config.delete_secret("key")?;

        let result: Result<String, ConfigError> = config.get_secret("key");
        assert!(matches!(result, Err(ConfigError::NotFound(_))));

        Ok(())
    }

    #[test]
    #[serial]
    fn test_secret_management() -> Result<(), ConfigError> {
        let config = new_test_config();

        // Test setting and getting a simple secret
        config.set_secret("api_key", &Value::String("secret123".to_string()))?;
        let value: String = config.get_secret("api_key")?;
        assert_eq!(value, "secret123");

        // Test environment variable override
        std::env::set_var("API_KEY", "env_secret");
        let value: String = config.get_secret("api_key")?;
        assert_eq!(value, "env_secret");
        std::env::remove_var("API_KEY");

        // Test deleting a secret
        config.delete_secret("api_key")?;
        let result: Result<String, ConfigError> = config.get_secret("api_key");
        assert!(matches!(result, Err(ConfigError::NotFound(_))));

        Ok(())
    }

    #[test]
    fn test_multiple_secrets() -> Result<(), ConfigError> {
        let config = new_test_config();

        // Set multiple secrets
        config.set_secret("key1", &Value::String("secret1".to_string()))?;
        config.set_secret("key2", &Value::String("secret2".to_string()))?;

        // Verify both exist
        let value1: String = config.get_secret("key1")?;
        let value2: String = config.get_secret("key2")?;
        assert_eq!(value1, "secret1");
        assert_eq!(value2, "secret2");

        // Delete one secret
        config.delete_secret("key1")?;

        // Verify key1 is gone but key2 remains
        let result1: Result<String, ConfigError> = config.get_secret("key1");
        let value2: String = config.get_secret("key2")?;
        assert!(matches!(result1, Err(ConfigError::NotFound(_))));
        assert_eq!(value2, "secret2");

        Ok(())
    }

    #[test]
    fn test_concurrent_writes() -> Result<(), ConfigError> {
        use std::sync::{Arc, Barrier, Mutex};
        use std::thread;

        let config = Arc::new(new_test_config());
        let barrier = Arc::new(Barrier::new(3)); // For 3 concurrent threads
        let values = Arc::new(Mutex::new(Mapping::new()));
        let mut handles = vec![];

        // Initialize with empty values
        config.save_values(&Default::default())?;

        // Spawn 3 threads that will try to write simultaneously
        for i in 0..3 {
            let config = Arc::clone(&config);
            let barrier = Arc::clone(&barrier);
            let values = Arc::clone(&values);
            let handle = thread::spawn(move || -> Result<(), ConfigError> {
                // Wait for all threads to reach this point
                barrier.wait();

                // Get the lock and update values
                let mut values = values.lock().unwrap();
                values.insert(
                    serde_yaml::to_value(format!("key{}", i)).unwrap(),
                    serde_yaml::to_value(format!("value{}", i)).unwrap(),
                );

                // Write all values
                config.save_values(&values)?;
                Ok(())
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap()?;
        }

        // Verify all values were written correctly
        let final_values = config.all_values()?;

        // Print the final values for debugging
        println!("Final values: {:?}", final_values);

        // Check that our 3 keys are present (migrations may add additional keys like "extensions")
        for i in 0..3 {
            let key = format!("key{}", i);
            let value = format!("value{}", i);
            assert!(
                final_values.contains_key(&key),
                "Missing key {} in final values",
                key
            );
            assert_eq!(
                final_values.get(&key).unwrap(),
                &Value::String(value),
                "Incorrect value for key {}",
                key
            );
        }

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn test_write_follows_symlink() -> Result<(), ConfigError> {
        use std::os::unix::fs as unix_fs;

        let dir = TempDir::new().unwrap();
        let target_path = dir.path().join("real_config.yaml");
        let symlink_path = dir.path().join("config.yaml");

        std::fs::write(&target_path, "{}\n")?;
        unix_fs::symlink(&target_path, &symlink_path)?;

        let secrets_file = NamedTempFile::new().unwrap();
        let config = Config::new_with_file_secrets(&symlink_path, secrets_file.path())?;

        config.set_param("key1", "value1")?;

        let meta = std::fs::symlink_metadata(&symlink_path)?;
        assert!(
            meta.file_type().is_symlink(),
            "config path should remain a symlink"
        );

        let content = std::fs::read_to_string(&symlink_path)?;
        assert!(content.contains("key1: value1"));

        let content = std::fs::read_to_string(&target_path)?;
        assert!(content.contains("key1: value1"));

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn test_write_fails_on_long_symlink_chain() -> Result<(), ConfigError> {
        use std::os::unix::fs as unix_fs;

        let dir = TempDir::new().unwrap();
        let target_path = dir.path().join("real_config.yaml");
        std::fs::write(&target_path, "{}\n")?;

        // config.yaml -> link1.yaml -> real_config.yaml
        // We only allow following one symlink hop. If there's another symlink, we should fail
        // rather than overwrite the intermediate symlink.
        let config_symlink = dir.path().join("config.yaml");
        let link1 = dir.path().join("link1.yaml");
        unix_fs::symlink(&target_path, &link1)?;
        unix_fs::symlink(&link1, &config_symlink)?;

        let secrets_file = NamedTempFile::new().unwrap();
        let config = Config::new_with_file_secrets(&config_symlink, secrets_file.path())?;

        let err = config.set_param("key1", "value1").unwrap_err();
        assert!(
            err.to_string().contains("Too many symlink levels"),
            "unexpected error: {err}"
        );

        let meta = std::fs::symlink_metadata(&config_symlink)?;
        assert!(
            meta.file_type().is_symlink(),
            "config path should remain a symlink"
        );
        let meta = std::fs::symlink_metadata(&link1)?;
        assert!(
            meta.file_type().is_symlink(),
            "intermediate link should remain a symlink"
        );

        Ok(())
    }

    #[test]
    fn test_config_recovery_from_backup() -> Result<(), ConfigError> {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        let config = Config::new_with_file_secrets(config_file.path(), secrets_file.path())?;

        // Create a valid config first
        config.set_param("key1", "value1")?;

        // Verify the backup was created by the first write
        let backup_paths = config.get_backup_paths();
        println!("Backup paths: {:?}", backup_paths);
        for (i, path) in backup_paths.iter().enumerate() {
            println!("Backup {} exists: {}", i, path.exists());
        }

        // Make another write to ensure backup is created
        config.set_param("key2", 42)?;

        // Check again
        for (i, path) in backup_paths.iter().enumerate() {
            println!(
                "After second write - Backup {} exists: {}",
                i,
                path.exists()
            );
        }

        // Corrupt the main config file
        std::fs::write(config_file.path(), "invalid: yaml: content: [unclosed")?;

        // Try to load values - should recover from backup
        let recovered_values = config.all_values()?;
        println!("Recovered values: {:?}", recovered_values);

        // Should have recovered the data
        assert!(
            !recovered_values.is_empty(),
            "Should have recovered at least one key"
        );

        Ok(())
    }

    #[test]
    fn test_config_recovery_creates_fresh_file() -> Result<(), ConfigError> {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        let config = Config::new_with_file_secrets(config_file.path(), secrets_file.path())?;

        // Create a corrupted config file with no backup
        std::fs::write(config_file.path(), "invalid: yaml: content: [unclosed")?;

        // Try to load values - should create a fresh default config
        let recovered_values = config.all_values()?;

        // Note: migrations may add keys like "extensions", so we just verify
        // that no user-defined keys exist (the config was reset)
        assert!(
            !recovered_values.contains_key("key1"),
            "Should not have user keys after recovery"
        );

        // Verify that a clean config file was written to disk
        let file_content = std::fs::read_to_string(config_file.path())?;

        // Should be valid YAML
        let parsed: serde_yaml::Value = serde_yaml::from_str(&file_content)?;
        assert!(parsed.is_mapping());

        // Should be able to load it again without issues
        let _reloaded_values = config.all_values()?;

        Ok(())
    }

    #[test]
    fn test_config_file_creation_when_missing() -> Result<(), ConfigError> {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        let config_path = config_file.path().to_path_buf();
        let config = Config::new_with_file_secrets(&config_path, secrets_file.path())?;

        // Delete the file to simulate it not existing
        std::fs::remove_file(&config_path)?;
        assert!(!config_path.exists());

        // Try to load values - should create a fresh default config file
        let values = config.all_values()?;

        // Note: migrations may add keys like "extensions", so we just verify
        // that no user-defined keys exist (the config was freshly created)
        assert!(
            !values.contains_key("key1"),
            "Should not have user keys in fresh config"
        );

        // Verify that the config file was created
        assert!(config_path.exists());

        // Verify that it's valid YAML
        let file_content = std::fs::read_to_string(&config_path)?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&file_content)?;
        assert!(parsed.is_mapping());

        // Should be able to load it again without issues
        let _reloaded_values = config.all_values()?;

        Ok(())
    }

    #[test]
    fn test_config_recovery_from_backup_when_missing() -> Result<(), ConfigError> {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        let config_path = config_file.path().to_path_buf();
        let config = Config::new_with_file_secrets(&config_path, secrets_file.path())?;

        // First, create a config with some data
        config.set_param("test_key_backup", "backup_value")?;
        config.set_param("another_key", 42)?;

        // Verify the backup was created
        let backup_paths = config.get_backup_paths();
        let primary_backup = &backup_paths[0]; // .bak file

        // Make sure we have a backup by doing another write
        config.set_param("third_key", true)?;
        assert!(primary_backup.exists(), "Backup should exist after writes");

        // Now delete the main config file to simulate it being lost
        std::fs::remove_file(&config_path)?;
        assert!(!config_path.exists());

        // Try to load values - should recover from backup
        let recovered_values = config.all_values()?;

        // Should have recovered the data from backup
        assert!(
            !recovered_values.is_empty(),
            "Should have recovered data from backup"
        );

        // Verify the main config file was restored
        assert!(config_path.exists(), "Main config file should be restored");

        // Verify we can load the data (using a key that won't conflict with env vars)
        if let Ok(backup_value) = config.get_param::<String>("test_key_backup") {
            // If we recovered the key, great!
            assert_eq!(backup_value, "backup_value");
        }
        // Note: Due to back up rotation, we might not get the exact same data,
        // but we should get some data back

        Ok(())
    }

    #[test]
    fn test_atomic_write_prevents_corruption() -> Result<(), ConfigError> {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        let config = Config::new_with_file_secrets(config_file.path(), secrets_file.path())?;

        // Set initial values
        config.set_param("key1", "value1")?;

        // Verify the config file exists and is valid
        assert!(config_file.path().exists());
        let content = std::fs::read_to_string(config_file.path())?;
        assert!(serde_yaml::from_str::<serde_yaml::Value>(&content).is_ok());

        // The temp file should not exist after successful write
        let temp_path = config_file.path().with_extension("tmp");
        assert!(!temp_path.exists(), "Temporary file should be cleaned up");

        Ok(())
    }

    #[test]
    fn test_backup_rotation() -> Result<(), ConfigError> {
        let config = new_test_config();

        // Create multiple versions to test rotation
        for i in 1..=7 {
            config.set_param("version", i)?;
        }

        let backup_paths = config.get_backup_paths();

        // Should have backups but not more than our limit
        let existing_backups: Vec<_> = backup_paths.iter().filter(|p| p.exists()).collect();
        assert!(
            existing_backups.len() == 1,
            "Should not exceed backup limit"
        ); // .bak + .bak.1 through .bak.5

        Ok(())
    }

    #[test]
    fn test_env_var_parsing_strings() -> Result<(), ConfigError> {
        // Test unquoted strings
        let value = Config::parse_env_value("ANTHROPIC")?;
        assert_eq!(value, Value::String("ANTHROPIC".to_string()));

        // Test strings with spaces
        let value = Config::parse_env_value("hello world")?;
        assert_eq!(value, Value::String("hello world".to_string()));

        // Test JSON quoted strings
        let value = Config::parse_env_value("\"ANTHROPIC\"")?;
        assert_eq!(value, Value::String("ANTHROPIC".to_string()));

        // Test empty string
        let value = Config::parse_env_value("")?;
        assert_eq!(value, Value::String("".to_string()));

        Ok(())
    }

    #[test]
    fn test_env_var_parsing_numbers() -> Result<(), ConfigError> {
        // Test integers
        let value = Config::parse_env_value("42")?;
        assert_eq!(value, Value::Number(42.into()));

        let value = Config::parse_env_value("-123")?;
        assert_eq!(value, Value::Number((-123).into()));

        // Test floats
        let value = Config::parse_env_value("3.41")?;
        assert!(matches!(value, Value::Number(_)));
        if let Value::Number(n) = value {
            assert_eq!(n.as_f64().unwrap(), 3.41);
        }

        let value = Config::parse_env_value("0.01")?;
        assert!(matches!(value, Value::Number(_)));
        if let Value::Number(n) = value {
            assert_eq!(n.as_f64().unwrap(), 0.01);
        }

        // Test zero
        let value = Config::parse_env_value("0")?;
        assert_eq!(value, Value::Number(0.into()));

        let value = Config::parse_env_value("0.0")?;
        assert!(matches!(value, Value::Number(_)));
        if let Value::Number(n) = value {
            assert_eq!(n.as_f64().unwrap(), 0.0);
        }

        // Test numbers starting with decimal point
        let value = Config::parse_env_value(".5")?;
        assert!(matches!(value, Value::Number(_)));
        if let Value::Number(n) = value {
            assert_eq!(n.as_f64().unwrap(), 0.5);
        }

        let value = Config::parse_env_value(".00001")?;
        assert!(matches!(value, Value::Number(_)));
        if let Value::Number(n) = value {
            assert_eq!(n.as_f64().unwrap(), 0.00001);
        }

        Ok(())
    }

    #[test]
    fn test_env_var_parsing_booleans() -> Result<(), ConfigError> {
        // Test true variants
        let value = Config::parse_env_value("true")?;
        assert_eq!(value, Value::Bool(true));

        let value = Config::parse_env_value("True")?;
        assert_eq!(value, Value::Bool(true));

        let value = Config::parse_env_value("TRUE")?;
        assert_eq!(value, Value::Bool(true));

        // Test false variants
        let value = Config::parse_env_value("false")?;
        assert_eq!(value, Value::Bool(false));

        let value = Config::parse_env_value("False")?;
        assert_eq!(value, Value::Bool(false));

        let value = Config::parse_env_value("FALSE")?;
        assert_eq!(value, Value::Bool(false));

        Ok(())
    }

    #[test]
    fn test_env_var_parsing_json() -> Result<(), ConfigError> {
        // Test JSON objects
        let value = Config::parse_env_value("{\"host\": \"localhost\", \"port\": 8080}")?;
        assert!(matches!(value, Value::Object(_)));
        if let Value::Object(obj) = value {
            assert_eq!(
                obj.get("host"),
                Some(&Value::String("localhost".to_string()))
            );
            assert_eq!(obj.get("port"), Some(&Value::Number(8080.into())));
        }

        // Test JSON arrays
        let value = Config::parse_env_value("[1, 2, 3]")?;
        assert!(matches!(value, Value::Array(_)));
        if let Value::Array(arr) = value {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], Value::Number(1.into()));
            assert_eq!(arr[1], Value::Number(2.into()));
            assert_eq!(arr[2], Value::Number(3.into()));
        }

        // Test JSON null
        let value = Config::parse_env_value("null")?;
        assert_eq!(value, Value::Null);

        Ok(())
    }

    #[test]
    fn test_env_var_parsing_edge_cases() -> Result<(), ConfigError> {
        // Test whitespace handling
        let value = Config::parse_env_value(" 42 ")?;
        assert_eq!(value, Value::Number(42.into()));

        let value = Config::parse_env_value(" true ")?;
        assert_eq!(value, Value::Bool(true));

        // Test strings that look like numbers but aren't
        let value = Config::parse_env_value("123abc")?;
        assert_eq!(value, Value::String("123abc".to_string()));

        let value = Config::parse_env_value("abc123")?;
        assert_eq!(value, Value::String("abc123".to_string()));

        // Test strings that look like booleans but aren't
        let value = Config::parse_env_value("truthy")?;
        assert_eq!(value, Value::String("truthy".to_string()));

        let value = Config::parse_env_value("falsy")?;
        assert_eq!(value, Value::String("falsy".to_string()));

        Ok(())
    }

    #[test]
    fn test_env_var_parsing_numeric_edge_cases() -> Result<(), ConfigError> {
        // Test leading zeros (should be treated as integers, not octal)
        let value = Config::parse_env_value("007")?;
        assert_eq!(value, Value::Number(7.into()));

        // Test large numbers
        let value = Config::parse_env_value("9223372036854775807")?; // i64::MAX
        assert_eq!(value, Value::Number(9223372036854775807i64.into()));

        // Test scientific notation (JSON parsing should handle this correctly)
        let value = Config::parse_env_value("1e10")?;
        assert!(matches!(value, Value::Number(_)));
        if let Value::Number(n) = value {
            assert_eq!(n.as_f64().unwrap(), 1e10);
        }

        // Test infinity (should be treated as string)
        let value = Config::parse_env_value("inf")?;
        assert_eq!(value, Value::String("inf".to_string()));

        Ok(())
    }

    #[test]
    fn test_env_var_with_config_integration() -> Result<(), ConfigError> {
        let config = new_test_config();

        // Test string environment variable (the original issue case)
        std::env::set_var("PROVIDER", "ANTHROPIC");
        let value: String = config.get_param("provider")?;
        assert_eq!(value, "ANTHROPIC");

        // Test number environment variable
        std::env::set_var("PORT", "8080");
        let value: i32 = config.get_param("port")?;
        assert_eq!(value, 8080);

        // Test boolean environment variable
        std::env::set_var("ENABLED", "true");
        let value: bool = config.get_param("enabled")?;
        assert!(value);

        // Test JSON object environment variable
        std::env::set_var("CONFIG", "{\"debug\": true, \"level\": 5}");
        #[derive(Deserialize, Debug, PartialEq)]
        struct TestConfig {
            debug: bool,
            level: i32,
        }
        let value: TestConfig = config.get_param("config")?;
        assert!(value.debug);
        assert_eq!(value.level, 5);

        // Clean up
        std::env::remove_var("PROVIDER");
        std::env::remove_var("PORT");
        std::env::remove_var("ENABLED");
        std::env::remove_var("CONFIG");

        Ok(())
    }

    #[test]
    fn test_env_var_precedence_over_config_file() -> Result<(), ConfigError> {
        let config = new_test_config();

        // Set value in config file
        config.set_param("test_precedence", "file_value")?;

        // Verify file value is returned when no env var
        let value: String = config.get_param("test_precedence")?;
        assert_eq!(value, "file_value");

        // Set environment variable
        std::env::set_var("TEST_PRECEDENCE", "env_value");

        // Environment variable should take precedence
        let value: String = config.get_param("test_precedence")?;
        assert_eq!(value, "env_value");

        // Clean up
        std::env::remove_var("TEST_PRECEDENCE");

        Ok(())
    }

    #[test]
    fn get_secrets_primary_from_env_uses_env_for_secondary() {
        let _guard = env_lock::lock_env([
            ("TEST_PRIMARY", Some("primary_env")),
            ("TEST_SECONDARY", Some("secondary_env")),
        ]);
        let config = new_test_config();
        let secrets = config
            .get_secrets("TEST_PRIMARY", &["TEST_SECONDARY"])
            .unwrap();

        assert_eq!(secrets["TEST_PRIMARY"], "primary_env");
        assert_eq!(secrets["TEST_SECONDARY"], "secondary_env");
    }

    #[test]
    fn get_secrets_primary_from_secret_uses_secret_for_secondary() {
        let _guard = env_lock::lock_env([("TEST_PRIMARY", None::<&str>), ("TEST_SECONDARY", None)]);
        let config = new_test_config();
        config
            .set_secret("TEST_PRIMARY", &"primary_secret")
            .unwrap();
        config
            .set_secret("TEST_SECONDARY", &"secondary_secret")
            .unwrap();

        let secrets = config
            .get_secrets("TEST_PRIMARY", &["TEST_SECONDARY"])
            .unwrap();

        assert_eq!(secrets["TEST_PRIMARY"], "primary_secret");
        assert_eq!(secrets["TEST_SECONDARY"], "secondary_secret");
    }

    #[test]
    fn get_secrets_primary_missing_returns_error() {
        let _guard = env_lock::lock_env([("TEST_PRIMARY", None::<&str>)]);
        let config = new_test_config();

        let result = config.get_secrets("TEST_PRIMARY", &[]);

        assert!(matches!(result, Err(ConfigError::NotFound(_))));
    }

    fn new_test_config() -> Config {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap()
    }

    fn new_test_config_with_defaults(defaults_content: &str) -> (Config, NamedTempFile) {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        let defaults_file = NamedTempFile::new().unwrap();
        std::fs::write(defaults_file.path(), defaults_content).unwrap();
        let config = Config::new_with_defaults(
            config_file.path(),
            secrets_file.path(),
            defaults_file.path(),
        )
        .unwrap();
        (config, defaults_file)
    }

    #[test]
    fn test_defaults_fallback_when_key_not_in_config() -> Result<(), ConfigError> {
        let (config, _defaults) =
            new_test_config_with_defaults("SECURITY_PROMPT_ENABLED: true\nsome_key: default_val");

        // Key only in defaults → returns defaults value
        let value: bool = config.get_param("SECURITY_PROMPT_ENABLED")?;
        assert!(value);

        let value: String = config.get_param("some_key")?;
        assert_eq!(value, "default_val");

        Ok(())
    }

    #[test]
    #[serial]
    fn test_full_precedence_env_over_config_over_defaults() -> Result<(), ConfigError> {
        let (config, _defaults) = new_test_config_with_defaults("my_key: from_defaults");

        // Only defaults → returns defaults
        let value: String = config.get_param("my_key")?;
        assert_eq!(value, "from_defaults");

        // Config file overrides defaults
        config.set_param("my_key", "from_config")?;
        let value: String = config.get_param("my_key")?;
        assert_eq!(value, "from_config");

        // Env var overrides config file (and defaults)
        std::env::set_var("MY_KEY", "from_env");
        let value: String = config.get_param("my_key")?;
        assert_eq!(value, "from_env");
        std::env::remove_var("MY_KEY");

        // After removing env var, config file value is back
        let value: String = config.get_param("my_key")?;
        assert_eq!(value, "from_config");

        Ok(())
    }

    #[test]
    fn test_no_defaults_file_behaves_as_before() {
        // Config without defaults (the normal open-source case)
        let config = new_test_config();

        let result: Result<String, ConfigError> = config.get_param("nonexistent_key");
        assert!(matches!(result, Err(ConfigError::NotFound(_))));
    }

    #[test]
    fn test_defaults_not_persisted_on_write() -> Result<(), ConfigError> {
        let (config, _defaults) = new_test_config_with_defaults("default_key: default_value");

        // Read a default value (should work)
        let value: String = config.get_param("default_key")?;
        assert_eq!(value, "default_value");

        // Write a different key
        config.set_param("user_key", "user_value")?;

        // Read config file directly - should NOT contain default_key
        let config_path = PathBuf::from(config.path());
        let file_content = std::fs::read_to_string(&config_path)?;
        assert!(
            !file_content.contains("default_key"),
            "Defaults should not be persisted to config file on write"
        );
        assert!(
            file_content.contains("user_key"),
            "User's key should be in config file"
        );

        // But reading via get_param should still return the default
        let value: String = config.get_param("default_key")?;
        assert_eq!(value, "default_value");

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn test_secrets_file_created_with_restricted_permissions() -> Result<(), ConfigError> {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let config_file = NamedTempFile::new().unwrap();
        let secrets_path = dir.path().join("secrets.yaml");

        let config = Config::new_with_file_secrets(config_file.path(), &secrets_path)?;
        config.set_secret("key", &"value")?;

        let mode = std::fs::metadata(&secrets_path)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        Ok(())
    }
}
