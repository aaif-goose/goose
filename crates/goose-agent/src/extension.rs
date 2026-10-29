use std::collections::HashMap;

use once_cell::sync::Lazy;
use rmcp::service::ClientInitializeError;
use rmcp::ServiceError as ClientError;
use serde::Deserializer;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

pub const DEFAULT_EXTENSION: &str = "developer";
pub const DEFAULT_DISPLAY_NAME: &str = "Developer";
pub const DEFAULT_EXTENSION_TIMEOUT: u64 = 300;

pub trait ExtensionConfigResolver: Send + Sync {
    fn get_secret(&self, key: &str) -> anyhow::Result<String>;
}

pub fn name_to_key(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    for c in name.chars() {
        result.push(match c {
            c if c.is_ascii_alphanumeric() || c == '_' || c == '-' => c,
            c if c.is_whitespace() => continue,
            _ => '_',
        });
    }
    result.to_lowercase()
}

#[derive(Error, Debug)]
#[error("process quit before initialization: stderr = {stderr}")]
pub struct ProcessExit {
    stderr: String,
    #[source]
    source: Box<ClientInitializeError>,
}

impl ProcessExit {
    pub fn new<T>(stderr: T, source: Box<ClientInitializeError>) -> Self
    where
        T: Into<String>,
    {
        ProcessExit {
            stderr: stderr.into(),
            source,
        }
    }
}

#[derive(Error, Debug)]
pub enum ExtensionError {
    #[error("failed a client call to an MCP server: {0}")]
    Client(#[from] ClientError),
    #[error("invalid config: {0}")]
    ConfigError(String),
    #[error("error during extension setup: {0}")]
    SetupError(String),
    #[error("join error occurred during task execution: {0}")]
    TaskJoinError(#[from] tokio::task::JoinError),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("failed to initialize MCP client: {0}")]
    InitializeError(#[from] Box<ClientInitializeError>),
    #[error("{0}")]
    ProcessExit(#[source] Box<ProcessExit>),
    #[error("{0}")]
    StartFailed(String),
}

impl From<ClientInitializeError> for ExtensionError {
    fn from(error: ClientInitializeError) -> Self {
        Self::InitializeError(Box::new(error))
    }
}

impl From<ProcessExit> for ExtensionError {
    fn from(error: ProcessExit) -> Self {
        Self::ProcessExit(Box::new(error))
    }
}

pub type ExtensionResult<T> = Result<T, ExtensionError>;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Envs {
    #[serde(default)]
    #[serde(flatten)]
    map: HashMap<String, String>,
}

impl<'de> Deserialize<'de> for Envs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map = HashMap::<String, String>::deserialize(deserializer)?;
        Ok(Self::new(map))
    }
}

impl Envs {
    const DISALLOWED_KEYS: [&'static str; 31] = [
        // 🔧 Binary path manipulation
        "PATH",       // Controls executable lookup paths — critical for command hijacking
        "PATHEXT",    // Windows: Determines recognized executable extensions (e.g., .exe, .bat)
        "SystemRoot", // Windows: Can affect system DLL resolution (e.g., `kernel32.dll`)
        "windir",     // Windows: Alternative to SystemRoot (used in legacy apps)
        // 🧬 Dynamic linker hijacking (Linux/macOS)
        "LD_LIBRARY_PATH",  // Alters shared library resolution
        "LD_PRELOAD",       // Forces preloading of shared libraries — common attack vector
        "LD_AUDIT",         // Loads a monitoring library that can intercept execution
        "LD_DEBUG",         // Enables verbose linker logging (information disclosure risk)
        "LD_BIND_NOW",      // Forces immediate symbol resolution, affecting ASLR
        "LD_ASSUME_KERNEL", // Tricks linker into thinking it's running on an older kernel
        // 🍎 macOS dynamic linker variables
        "DYLD_LIBRARY_PATH",     // Same as LD_LIBRARY_PATH but for macOS
        "DYLD_INSERT_LIBRARIES", // macOS equivalent of LD_PRELOAD
        "DYLD_FRAMEWORK_PATH",   // Overrides framework lookup paths
        // 🐍 Python / Node / Ruby / Java / Golang hijacking
        "PYTHONPATH",   // Overrides Python module resolution
        "PYTHONHOME",   // Overrides Python root directory
        "NODE_OPTIONS", // Injects options/scripts into every Node.js process
        "RUBYOPT",      // Injects Ruby execution flags
        "GEM_PATH",     // Alters where RubyGems looks for installed packages
        "GEM_HOME",     // Changes RubyGems default install location
        "CLASSPATH",    // Java: Controls where classes are loaded from — critical for RCE attacks
        "GO111MODULE",  // Go: Forces use of module proxy or disables it
        "GOROOT", // Go: Changes root installation directory (could lead to execution hijacking)
        // 🖥️ Windows-specific process & DLL hijacking
        "APPINIT_DLLS", // Forces Windows to load a DLL into every process
        "SESSIONNAME",  // Affects Windows session configuration
        "ComSpec",      // Determines default command interpreter (can replace `cmd.exe`)
        "TEMP",
        "TMP",          // Redirects temporary file storage (useful for injection attacks)
        "LOCALAPPDATA", // Controls application data paths (can be abused for persistence)
        "USERPROFILE",  // Windows user directory (can affect profile-based execution paths)
        "HOMEDRIVE",
        "HOMEPATH", // Changes where the user's home directory is located
    ];

    /// Constructs a new Envs, skipping disallowed env vars with a warning
    pub fn new(map: HashMap<String, String>) -> Self {
        let mut validated = HashMap::new();

        for (key, value) in map {
            if Self::is_disallowed(&key) {
                warn!("Skipping disallowed env var: {}", key);
                continue;
            }
            validated.insert(key, value);
        }

        Self { map: validated }
    }

    pub fn get_env(&self) -> HashMap<String, String> {
        self.map.clone()
    }

    fn is_disallowed(key: &str) -> bool {
        Self::DISALLOWED_KEYS
            .iter()
            .any(|disallowed| disallowed.eq_ignore_ascii_case(key))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum ExtensionConfig {
    #[serde(rename = "stdio")]
    Stdio {
        name: String,
        #[serde(default)]
        #[serde(deserialize_with = "deserialize_null_with_default")]
        description: String,
        cmd: String,
        args: Vec<String>,
        #[serde(default, alias = "env")]
        envs: Envs,
        #[serde(default)]
        env_keys: Vec<String>,
        timeout: Option<u64>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Vec::is_empty")]
        available_tools: Vec<String>,
    },
    /// Built-in extension that is part of the bundled goose MCP server
    #[serde(rename = "builtin")]
    Builtin {
        name: String,
        #[serde(default)]
        #[serde(deserialize_with = "deserialize_null_with_default")]
        description: String,
        display_name: Option<String>, // needed for the UI
        timeout: Option<u64>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Vec::is_empty")]
        available_tools: Vec<String>,
    },
    /// Platform extensions that have direct access to the agent etc and run in the agent process
    #[serde(rename = "platform")]
    Platform {
        name: String,
        #[serde(default)]
        #[serde(deserialize_with = "deserialize_null_with_default")]
        description: String,
        display_name: Option<String>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Vec::is_empty")]
        available_tools: Vec<String>,
    },
    #[serde(rename = "streamable_http")]
    StreamableHttp {
        name: String,
        #[serde(default)]
        #[serde(deserialize_with = "deserialize_null_with_default")]
        description: String,
        uri: String,
        #[serde(default)]
        envs: Envs,
        #[serde(default)]
        env_keys: Vec<String>,
        #[serde(default)]
        headers: HashMap<String, String>,
        // NOTE: set timeout to be optional for compatibility.
        // However, new configurations should include this field.
        timeout: Option<u64>,
        /// Optional Unix domain socket path for HTTP-over-UDS transport.
        /// When set, the HTTP connection is routed through this socket while
        /// `uri` is used for the Host header and path.
        /// Use `@name` for Linux abstract sockets.
        #[serde(default)]
        socket: Option<String>,
        /// OAuth client ID pre-registered with the server's authorization
        /// server. When set, it is used directly for the authorization flow
        /// instead of Client ID Metadata Documents or Dynamic Client
        /// Registration — required for authorization servers that support
        /// neither. Supports `$VAR`/`${VAR}` substitution.
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        /// Name of the env/secret key holding the OAuth client secret paired
        /// with `client_id`. The value is resolved from `envs`/`env_keys` or
        /// the config secret store — never stored inline. Optional: public
        /// clients using PKCE have no secret.
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        client_secret_key: Option<String>,
        /// OAuth scopes to request with `client_id`. When empty, scopes are
        /// selected from server metadata, which may be broader than needed.
        #[serde(default)]
        #[serde(skip_serializing_if = "Vec::is_empty")]
        scopes: Vec<String>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Vec::is_empty")]
        available_tools: Vec<String>,
    },
}

impl Default for ExtensionConfig {
    fn default() -> Self {
        Self::Builtin {
            name: DEFAULT_EXTENSION.to_string(),
            display_name: Some(DEFAULT_DISPLAY_NAME.to_string()),
            description: "default".to_string(),
            timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
            bundled: Some(true),
            available_tools: Vec::new(),
        }
    }
}

impl ExtensionConfig {
    pub fn streamable_http<S: Into<String>, T: Into<u64>>(
        name: S,
        uri: S,
        description: S,
        timeout: T,
    ) -> Self {
        Self::StreamableHttp {
            name: name.into(),
            uri: uri.into(),
            envs: Envs::default(),
            env_keys: Vec::new(),
            headers: HashMap::new(),
            description: description.into(),
            timeout: Some(timeout.into()),
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: Vec::new(),
            bundled: None,
            available_tools: Vec::new(),
        }
    }

    pub fn stdio<S: Into<String>, T: Into<u64>>(
        name: S,
        cmd: S,
        description: S,
        timeout: T,
    ) -> Self {
        Self::Stdio {
            name: name.into(),
            cmd: cmd.into(),
            args: vec![],
            envs: Envs::default(),
            env_keys: Vec::new(),
            description: description.into(),
            timeout: Some(timeout.into()),
            cwd: None,
            bundled: None,
            available_tools: Vec::new(),
        }
    }

    pub fn with_args<I, S>(self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        match self {
            Self::Stdio {
                name,
                cmd,
                envs,
                env_keys,
                timeout,
                cwd,
                description,
                bundled,
                available_tools,
                ..
            } => Self::Stdio {
                name,
                cmd,
                envs,
                env_keys,
                args: args.into_iter().map(Into::into).collect(),
                description,
                timeout,
                cwd,
                bundled,
                available_tools,
            },
            other => other,
        }
    }

    pub fn key(&self) -> String {
        name_to_key(&self.name())
    }

    pub fn name(&self) -> String {
        match self {
            Self::StreamableHttp { name, .. } => name,
            Self::Stdio { name, .. } => name,
            Self::Builtin { name, .. } => name,
            Self::Platform { name, .. } => name,
        }
        .to_string()
    }

    pub fn is_tool_available(&self, tool_name: &str) -> bool {
        let available_tools = match self {
            Self::StreamableHttp {
                available_tools, ..
            }
            | Self::Stdio {
                available_tools, ..
            }
            | Self::Builtin {
                available_tools, ..
            }
            | Self::Platform {
                available_tools, ..
            } => available_tools,
        };

        available_tools.is_empty() || available_tools.contains(&tool_name.to_string())
    }

    pub async fn resolve(self, config: &impl ExtensionConfigResolver) -> ExtensionResult<Self> {
        match self {
            Self::Stdio {
                name,
                description,
                cmd,
                args,
                envs,
                env_keys,
                timeout,
                cwd,
                bundled,
                available_tools,
            } => {
                let merged = merge_environments(&envs, env_keys.iter(), config).await?;
                Ok(Self::Stdio {
                    name,
                    description,
                    cmd,
                    args,
                    envs: Envs::new(merged.clone()),
                    env_keys: vec![],
                    timeout,
                    cwd: cwd.map(|s| substitute_env_vars(&s, &merged)),
                    bundled,
                    available_tools,
                })
            }
            Self::StreamableHttp {
                name,
                description,
                uri,
                envs,
                env_keys,
                headers,
                timeout,
                socket,
                client_id,
                client_secret_key,
                scopes,
                bundled,
                available_tools,
            } => {
                // Resolve the OAuth client secret alongside env_keys so that
                // rotating it changes the resolved config, which is what
                // add_extension compares to decide whether to restart.
                let merged =
                    merge_environments(&envs, env_keys.iter().chain(&client_secret_key), config)
                        .await?;
                let headers = headers
                    .into_iter()
                    .map(|(k, v)| {
                        let v = substitute_env_vars(&v, &merged);
                        (k, v)
                    })
                    .collect();
                let socket = socket.map(|s| substitute_env_vars(&s, &merged));
                let client_id = client_id.map(|c| substitute_env_vars(&c, &merged));
                Ok(Self::StreamableHttp {
                    name,
                    description,
                    uri: substitute_env_vars(&uri, &merged),
                    envs: Envs::new(merged),
                    env_keys: vec![],
                    headers,
                    timeout,
                    socket,
                    client_id,
                    client_secret_key,
                    scopes,
                    bundled,
                    available_tools,
                })
            }
            other => Ok(other),
        }
    }
}

static RE_ENV_BRACES: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"\$\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}").expect("valid regex"));

static RE_ENV_SIMPLE: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").expect("valid regex"));

async fn merge_environments(
    envs: &Envs,
    env_keys: impl Iterator<Item = &String>,
    config: &impl ExtensionConfigResolver,
) -> ExtensionResult<HashMap<String, String>> {
    let mut all_envs = envs.get_env();
    for key in env_keys {
        // inline values shadow the secret store
        if all_envs.contains_key(key) {
            continue;
        }
        // Config::get_secret parses env values as JSON, so PORT=3000 would come
        // back as a number. An env override is a string by definition; only
        // the secret store gets the type check.
        let value = match std::env::var(key.to_uppercase()) {
            Ok(value) => value,
            Err(_) => config.get_secret(key).map_err(|e| {
                ExtensionError::ConfigError(format!(
                    "Failed to fetch secret '{}' from config: {}",
                    key, e
                ))
            })?,
        };
        all_envs.insert(key.clone(), value);
    }
    Ok(Envs::new(all_envs).get_env())
}

fn substitute_env_vars(value: &str, env_map: &HashMap<String, String>) -> String {
    let mut result = value.to_string();

    for cap in RE_ENV_BRACES.captures_iter(value) {
        if let Some(var_name) = cap.get(1) {
            if let Some(env_value) = env_map.get(var_name.as_str()) {
                result = result.replace(&cap[0], env_value);
            }
        }
    }

    // Scan the original input for $VAR patterns (not the post-substitution result)
    // to avoid recursive expansion when a substituted value contains $OTHER_VAR.
    for cap in RE_ENV_SIMPLE.captures_iter(value) {
        if let Some(var_name) = cap.get(1) {
            if !value.contains(&format!("${{{}}}", var_name.as_str())) {
                if let Some(env_value) = env_map.get(var_name.as_str()) {
                    result = result.replace(&cap[0], env_value);
                }
            }
        }
    }

    result
}

impl std::fmt::Display for ExtensionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtensionConfig::StreamableHttp {
                name, uri, socket, ..
            } => {
                if let Some(socket) = socket {
                    write!(f, "StreamableHttp({}: {} via {})", name, uri, socket)
                } else {
                    write!(f, "StreamableHttp({}: {})", name, uri)
                }
            }
            ExtensionConfig::Stdio {
                name, cmd, args, ..
            } => {
                write!(f, "Stdio({}: {} {})", name, cmd, args.join(" "))
            }
            ExtensionConfig::Builtin { name, .. } => write!(f, "Builtin({})", name),
            ExtensionConfig::Platform { name, .. } => write!(f, "Platform({})", name),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExtensionInfo {
    pub name: String,
    pub instructions: String,
    pub has_resources: bool,
}

impl ExtensionInfo {
    pub fn new(name: &str, instructions: &str, has_resources: bool) -> Self {
        Self {
            name: name.to_string(),
            instructions: instructions.to_string(),
            has_resources,
        }
    }
}

fn deserialize_null_with_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

pub fn check_unique_keys(extensions: &[ExtensionConfig]) -> anyhow::Result<()> {
    let mut keys = std::collections::HashSet::new();
    match extensions.iter().find(|config| !keys.insert(config.key())) {
        Some(config) => Err(anyhow::anyhow!(
            "extension '{}' appears twice in the selection",
            config.name()
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    #[test_case("Bearer ${ AUTH_TOKEN }", "Bearer secret123"; "braces_with_spaces")]
    #[test_case("Bearer ${AUTH_TOKEN}", "Bearer secret123"; "braces")]
    #[test_case("Bearer $AUTH_TOKEN", "Bearer secret123"; "bare")]
    #[test_case("Key: $API_KEY, Token: ${AUTH_TOKEN}", "Key: key456, Token: secret123"; "multiple")]
    #[test_case("Bearer ${UNKNOWN_VAR}", "Bearer ${UNKNOWN_VAR}"; "unknown_left_alone")]
    #[test_case("${TOKEN}", "abc$KEY"; "substituted_value_not_expanded_again_braces")]
    #[test_case("$TOKEN", "abc$KEY"; "substituted_value_not_expanded_again_bare")]
    fn test_substitute_env_vars(input: &str, expected: &str) {
        let env_map = HashMap::from(
            [
                ("AUTH_TOKEN", "secret123"),
                ("API_KEY", "key456"),
                ("TOKEN", "abc$KEY"),
                ("KEY", "xyz"),
            ]
            .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        assert_eq!(substitute_env_vars(input, &env_map), expected);
    }
}
