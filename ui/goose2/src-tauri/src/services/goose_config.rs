use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};
use serde::Serialize;
use serde_json::Value;

use super::provider_defs::{find_provider_def, PROVIDER_CONFIG_DEFS};

const KEYRING_SERVICE: &str = "goose";
const KEYRING_USERNAME: &str = "secrets";
const CONFIG_YAML_NAME: &str = "config.yaml";
const SECRETS_YAML_NAME: &str = "secrets.yaml";
const SECRET_MASK_PREFIX_LEN: usize = 4;
const SECRET_MASK_SUFFIX_LEN: usize = 3;
const SECRET_MASK_FALLBACK: &str = "***";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub provider_id: String,
    pub is_configured: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldValue {
    pub key: String,
    pub value: Option<String>,
    pub is_set: bool,
    pub is_secret: bool,
    pub required: bool,
}

pub struct GooseConfig {
    config_dir: PathBuf,
    guard: Mutex<()>,
}

impl GooseConfig {
    pub fn new() -> Self {
        let config_dir = Self::resolve_config_dir();
        log::info!("GooseConfig using config dir: {}", config_dir.display());
        Self {
            config_dir,
            guard: Mutex::new(()),
        }
    }

    fn resolve_config_dir() -> PathBuf {
        if let Ok(root) = std::env::var("GOOSE_PATH_ROOT") {
            return PathBuf::from(root).join("config");
        }

        let strategy = choose_app_strategy(AppStrategyArgs {
            top_level_domain: "Block".to_string(),
            author: "Block".to_string(),
            app_name: "goose".to_string(),
        })
        .expect("goose requires a home dir");

        strategy.config_dir()
    }

    fn read_config_map(&self) -> serde_yaml::Mapping {
        let config_path = self.config_dir.join(CONFIG_YAML_NAME);
        match std::fs::read_to_string(&config_path) {
            Ok(contents) => {
                serde_yaml::from_str::<serde_yaml::Mapping>(&contents).unwrap_or_default()
            }
            Err(_) => serde_yaml::Mapping::new(),
        }
    }

    fn write_config_map(&self, config: &serde_yaml::Mapping) -> Result<(), String> {
        let path = self.config_dir.join(CONFIG_YAML_NAME);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let yaml = serde_yaml::to_string(config)
            .map_err(|e| format!("Failed to serialize config: {e}"))?;

        std::fs::write(&path, yaml).map_err(|e| format!("Failed to write config file: {e}"))
    }

    fn get_secret(&self, key: &str) -> Option<String> {
        let env_key = key.to_uppercase();
        if let Ok(value) = std::env::var(&env_key) {
            return Some(value);
        }

        let secrets = self.all_secrets();
        secrets
            .get(key)
            .and_then(|value| value.as_str().map(|secret| secret.to_string()))
    }

    fn mask_secret_value(value: &str) -> String {
        let prefix: String = value.chars().take(SECRET_MASK_PREFIX_LEN).collect();
        let suffix_chars: Vec<char> = value.chars().rev().take(SECRET_MASK_SUFFIX_LEN).collect();
        let suffix: String = suffix_chars.into_iter().rev().collect();

        if prefix.is_empty()
            || suffix.is_empty()
            || value.chars().count() <= SECRET_MASK_PREFIX_LEN + SECRET_MASK_SUFFIX_LEN
        {
            return SECRET_MASK_FALLBACK.to_string();
        }

        format!("{prefix}...{suffix}")
    }

    fn has_param(&self, key: &str) -> bool {
        self.get_param(key).is_some()
    }

    fn has_secret(&self, key: &str) -> bool {
        self.get_secret(key).is_some()
    }

    pub fn get_param(&self, key: &str) -> Option<String> {
        let env_key = key.to_uppercase();
        if let Ok(value) = std::env::var(&env_key) {
            return Some(value);
        }

        let config = self.read_config_map();
        let yaml_key = serde_yaml::Value::String(key.to_string());
        config
            .get(&yaml_key)
            .and_then(|value| value.as_str().map(|param| param.to_string()))
    }

    pub fn get_secret_masked(&self, key: &str) -> Option<String> {
        self.get_secret(key)
            .map(|value| Self::mask_secret_value(&value))
    }

    pub fn set_param(&self, key: &str, value: &str) -> Result<(), String> {
        let _guard = self.guard.lock().unwrap();

        let mut config = self.read_config_map();
        config.insert(
            serde_yaml::Value::String(key.to_string()),
            serde_yaml::Value::String(value.to_string()),
        );

        self.write_config_map(&config)
    }

    pub fn delete_param(&self, key: &str) -> Result<bool, String> {
        let _guard = self.guard.lock().unwrap();

        let mut config = self.read_config_map();
        let yaml_key = serde_yaml::Value::String(key.to_string());
        let removed = config.remove(&yaml_key).is_some();

        if removed {
            self.write_config_map(&config)?;
        }

        Ok(removed)
    }

    pub fn set_secret(&self, key: &str, value: &str) -> Result<(), String> {
        let _guard = self.guard.lock().unwrap();

        let mut secrets = self.all_secrets();
        secrets.insert(key.to_string(), Value::String(value.to_string()));

        if self.is_keyring_disabled() {
            self.write_secrets_to_file(&secrets)
        } else {
            self.write_secrets_to_keyring(&secrets)
        }
    }

    pub fn delete_secret(&self, key: &str) -> Result<bool, String> {
        let _guard = self.guard.lock().unwrap();

        let mut secrets = self.all_secrets();
        let removed = secrets.remove(key).is_some();

        if removed {
            if self.is_keyring_disabled() {
                self.write_secrets_to_file(&secrets)?;
            } else {
                self.write_secrets_to_keyring(&secrets)?;
            }
        }

        Ok(removed)
    }

    fn has_oauth_cache(&self, cache_path: &str) -> bool {
        let full_path = self.config_dir.join(cache_path);
        if full_path.is_dir() {
            std::fs::read_dir(&full_path)
                .map(|mut entries| entries.any(|_| true))
                .unwrap_or(false)
        } else {
            full_path.exists()
        }
    }

    fn delete_oauth_cache(&self, cache_path: &str) -> Result<bool, String> {
        let _guard = self.guard.lock().unwrap();
        let full_path = self.config_dir.join(cache_path);

        if full_path.is_dir() {
            std::fs::remove_dir_all(&full_path)
                .map_err(|e| format!("Failed to remove OAuth cache directory: {e}"))?;
            return Ok(true);
        }

        if full_path.exists() {
            std::fs::remove_file(&full_path)
                .map_err(|e| format!("Failed to remove OAuth cache file: {e}"))?;
            return Ok(true);
        }

        Ok(false)
    }

    pub fn check_provider_status(&self, provider_id: &str) -> ProviderStatus {
        if provider_id == "databricks" {
            let has_host = self.has_param("DATABRICKS_HOST");
            let has_token = self.has_secret("DATABRICKS_TOKEN");
            let has_oauth = self.has_oauth_cache("databricks/oauth");

            return ProviderStatus {
                provider_id: provider_id.to_string(),
                is_configured: has_host && (has_token || has_oauth),
            };
        }

        let def = match find_provider_def(provider_id) {
            Some(def) => def,
            None => {
                return ProviderStatus {
                    provider_id: provider_id.to_string(),
                    is_configured: false,
                }
            }
        };

        let has_oauth = def
            .oauth_cache_path
            .map(|p| self.has_oauth_cache(p))
            .unwrap_or(false);

        if has_oauth {
            return ProviderStatus {
                provider_id: provider_id.to_string(),
                is_configured: true,
            };
        }

        let all_required_present = def.keys.iter().all(|k| {
            if !k.required {
                return true;
            }
            if k.is_secret {
                self.has_secret(k.name)
            } else {
                self.has_param(k.name)
            }
        });

        let has_any_key = def.keys.iter().any(|k| {
            if k.is_secret {
                self.has_secret(k.name)
            } else {
                self.has_param(k.name)
            }
        });

        let is_configured = if def.keys.is_empty() {
            false
        } else {
            all_required_present && has_any_key
        };

        ProviderStatus {
            provider_id: provider_id.to_string(),
            is_configured,
        }
    }

    pub fn check_all_provider_status(&self) -> Vec<ProviderStatus> {
        PROVIDER_CONFIG_DEFS
            .iter()
            .map(|def| self.check_provider_status(def.id))
            .collect()
    }

    pub fn get_provider_field_values(&self, provider_id: &str) -> Result<Vec<FieldValue>, String> {
        let def = find_provider_def(provider_id)
            .ok_or_else(|| format!("Unknown provider '{provider_id}'"))?;

        Ok(def
            .keys
            .iter()
            .map(|config_key| {
                let value = if config_key.is_secret {
                    self.get_secret_masked(config_key.name)
                } else {
                    self.get_param(config_key.name)
                };

                FieldValue {
                    key: config_key.name.to_string(),
                    is_set: value.is_some(),
                    value,
                    is_secret: config_key.is_secret,
                    required: config_key.required,
                }
            })
            .collect())
    }

    pub fn delete_all_provider_fields(&self, provider_id: &str) -> Result<(), String> {
        let def = find_provider_def(provider_id)
            .ok_or_else(|| format!("Unknown provider '{provider_id}'"))?;

        for config_key in def.keys {
            if config_key.is_secret {
                self.delete_secret(config_key.name)?;
            } else {
                self.delete_param(config_key.name)?;
            }
        }

        if let Some(oauth_cache_path) = def.oauth_cache_path {
            self.delete_oauth_cache(oauth_cache_path)?;
        }

        Ok(())
    }

    fn is_keyring_disabled(&self) -> bool {
        if std::env::var("GOOSE_DISABLE_KEYRING").is_ok() {
            return true;
        }

        let config_path = self.config_dir.join(CONFIG_YAML_NAME);
        if let Ok(contents) = std::fs::read_to_string(&config_path) {
            if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Mapping>(&contents) {
                let key = serde_yaml::Value::String("GOOSE_DISABLE_KEYRING".to_string());
                if let Some(val) = yaml.get(&key) {
                    return val.as_bool().unwrap_or(false)
                        || val
                            .as_str()
                            .map(|s| s == "true" || s == "1")
                            .unwrap_or(false);
                }
            }
        }

        false
    }

    fn all_secrets(&self) -> HashMap<String, Value> {
        if self.is_keyring_disabled() {
            return self.read_secrets_from_file();
        }

        match self.read_secrets_from_keyring() {
            Ok(secrets) => secrets,
            Err(e) => {
                log::warn!("Keyring read failed, falling back to secrets file: {e}");
                self.read_secrets_from_file()
            }
        }
    }

    fn read_secrets_from_keyring(&self) -> Result<HashMap<String, Value>, String> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME)
            .map_err(|e| format!("Failed to access keyring: {e}"))?;

        match entry.get_password() {
            Ok(json_str) => serde_json::from_str(&json_str)
                .map_err(|e| format!("Failed to parse keyring JSON: {e}")),
            Err(keyring::Error::NoEntry) => Ok(HashMap::new()),
            Err(e) => Err(format!("Failed to read keyring: {e}")),
        }
    }

    fn read_secrets_from_file(&self) -> HashMap<String, Value> {
        let path = self.config_dir.join(SECRETS_YAML_NAME);
        match std::fs::read_to_string(&path) {
            Ok(contents) => serde_yaml::from_str::<HashMap<String, String>>(&contents)
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, Value::String(v)))
                .collect(),
            Err(_) => HashMap::new(),
        }
    }

    fn write_secrets_to_keyring(&self, secrets: &HashMap<String, Value>) -> Result<(), String> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME)
            .map_err(|e| format!("Failed to access keyring: {e}"))?;

        let json_str = serde_json::to_string(secrets)
            .map_err(|e| format!("Failed to serialize secrets: {e}"))?;

        entry
            .set_password(&json_str)
            .map_err(|e| format!("Failed to write to keyring: {e}"))
    }

    fn write_secrets_to_file(&self, secrets: &HashMap<String, Value>) -> Result<(), String> {
        let path = self.config_dir.join(SECRETS_YAML_NAME);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let yaml_map: HashMap<String, String> = secrets
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect();

        let yaml = serde_yaml::to_string(&yaml_map)
            .map_err(|e| format!("Failed to serialize secrets: {e}"))?;

        #[cfg(unix)]
        {
            use std::fs::OpenOptions;
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)
                .map_err(|e| format!("Failed to open secrets file: {e}"))?;

            file.write_all(yaml.as_bytes())
                .map_err(|e| format!("Failed to write secrets file: {e}"))
        }

        #[cfg(not(unix))]
        {
            std::fs::write(&path, yaml).map_err(|e| format!("Failed to write secrets file: {e}"))
        }
    }
}
