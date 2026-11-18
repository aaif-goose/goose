//! Convert selected Registry entries without running a launcher or writing credentials.
use crate::agents::extension::{Envs, ExtensionConfig, TransportConfig};
use crate::config::{ExtensionEntry, DEFAULT_EXTENSION_TIMEOUT};
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct ServerDocument {
    name: String,
    description: String,
    #[serde(default)]
    packages: Vec<Package>,
    #[serde(default)]
    remotes: Vec<Transport>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Package {
    registry_type: String,
    identifier: String,
    version: Option<String>,
    runtime_hint: Option<String>,
    registry_base_url: Option<String>,
    file_sha256: Option<String>,
    transport: Transport,
    #[serde(default)]
    runtime_arguments: Vec<Argument>,
    #[serde(default)]
    package_arguments: Vec<Argument>,
    #[serde(default)]
    environment_variables: Vec<NamedInput>,
}

#[derive(Deserialize)]
struct Transport {
    #[serde(rename = "type")]
    kind: String,
    url: Option<String>,
    #[serde(default)]
    headers: Vec<NamedInput>,
    #[serde(default)]
    variables: HashMap<String, serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    value: Option<String>,
    default: Option<String>,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    is_secret: bool,
    #[serde(default)]
    variables: HashMap<String, serde::de::IgnoredAny>,
}

impl Input {
    fn literal(&self) -> Result<Option<String>> {
        ensure!(
            self.variables.is_empty(),
            "input templates are not supported"
        );
        let value = self.value.as_ref().or(self.default.as_ref());
        ensure!(
            !self.is_secret || value.is_none(),
            "embedded secret values are not supported; provision credentials separately"
        );
        Ok(value.cloned())
    }
}

#[derive(Deserialize)]
struct NamedInput {
    name: String,
    #[serde(flatten)]
    input: Input,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Argument {
    Named {
        #[serde(flatten)]
        value: NamedInput,
    },
    Positional {
        #[serde(flatten)]
        value: Input,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerSelection {
    Package(usize),
    Remote(usize),
}

impl ServerDocument {
    pub fn parse(json: &str) -> Result<Self> {
        let document: Self =
            serde_json::from_str(json).context("invalid MCP Registry server.json")?;
        static NAME: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new(r"^[a-zA-Z0-9.-]+/[a-zA-Z0-9._-]+$").expect("valid regex")
        });
        ensure!(
            NAME.is_match(&document.name),
            "expected a namespaced MCP Registry server name"
        );
        Ok(document)
    }

    pub fn choices(&self) -> Vec<(ServerSelection, String)> {
        self.packages
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    ServerSelection::Package(i),
                    format!("Package {i}: {:?} ({:?})", p.identifier, p.registry_type),
                )
            })
            .chain(self.remotes.iter().enumerate().map(|(i, remote)| {
                (
                    ServerSelection::Remote(i),
                    format!(
                        "Remote {i}: {:?}",
                        remote.url.as_deref().unwrap_or("missing URL")
                    ),
                )
            }))
            .collect()
    }

    pub fn extensions(&self, selections: &[ServerSelection]) -> Result<Vec<ExtensionEntry>> {
        selections
            .iter()
            .map(|&selection| self.extension(selection))
            .collect()
    }

    fn extension(&self, selection: ServerSelection) -> Result<ExtensionEntry> {
        let (kind, index) = match selection {
            ServerSelection::Package(i) => ("package", i),
            ServerSelection::Remote(i) => ("remote", i),
        };
        let name = if self.packages.len() + self.remotes.len() == 1 {
            self.name.clone()
        } else {
            format!("{}-{kind}-{index}", self.name)
        };
        let description = self.description.clone();
        let config = match selection {
            ServerSelection::Package(i) => {
                let package = self.packages.get(i).context("package index out of range")?;
                package.config(name, description)?
            }
            ServerSelection::Remote(i) => self
                .remotes
                .get(i)
                .context("remote index out of range")?
                .config(name, description)?,
        };
        Ok(ExtensionEntry {
            enabled: true,
            config: ExtensionConfig::External(config),
        })
    }
}

impl Package {
    fn config(&self, name: String, description: String) -> Result<TransportConfig> {
        if self.transport.kind != "stdio" {
            return self.transport.config(name, description);
        }
        let (default_runtime, separator, public_registry) = match self.registry_type.as_str() {
            "npm" => ("npx", "@", "https://registry.npmjs.org"),
            "pypi" => ("uvx", "==", "https://pypi.org"),
            "oci" => ("docker", ":", "https://docker.io"),
            "nuget" => ("dnx", "@", "https://api.nuget.org/v3/index.json"),
            other => bail!("unsupported package registry type '{other}'"),
        };
        ensure!(
            self.file_sha256.is_none(),
            "package integrity hashes are not supported"
        );
        ensure!(
            self.registry_base_url
                .as_deref()
                .is_none_or(|url| url.trim_end_matches('/') == public_registry),
            "custom package registries are not supported"
        );
        ensure!(
            !self.identifier.is_empty()
                && !self.identifier.starts_with('-')
                && !self.identifier.contains('\0'),
            "invalid package identifier"
        );
        let cmd = self
            .runtime_hint
            .as_deref()
            .unwrap_or(default_runtime)
            .to_owned();
        ensure!(
            !cmd.trim().is_empty() && !cmd.contains('\0'),
            "invalid package runtime"
        );
        let (env, env_keys) = named_inputs(&self.environment_variables, false)?;
        let mut args = Vec::new();
        if self.registry_type == "oci" {
            args.push("run".into());
        }
        args.extend(arguments(&self.runtime_arguments)?);
        match self.registry_type.as_str() {
            "npm" => args.push("-y".into()),
            "oci" => {
                args.extend(["--rm".into(), "-i".into()]);
                let mut keys: Vec<_> = env.keys().chain(&env_keys).collect();
                keys.sort_unstable();
                for key in keys {
                    args.extend(["-e".into(), key.to_owned()]);
                }
            }
            _ => {}
        }
        args.push(match &self.version {
            Some(version) if self.registry_type == "oci" && version.contains(':') => {
                format!("{}@{version}", self.identifier)
            }
            Some(version) => {
                ensure!(
                    !version.is_empty() && !version.contains('\0'),
                    "invalid package version"
                );
                format!("{}{separator}{version}", self.identifier)
            }
            None => self.identifier.clone(),
        });
        let package_args = arguments(&self.package_arguments)?;
        if self.registry_type == "nuget" && !package_args.is_empty() {
            args.push("--".into());
        }
        args.extend(package_args);
        Ok(TransportConfig::Stdio {
            name,
            description,
            cmd,
            args,
            envs: Envs::new(env),
            env_keys,
            timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
            cwd: None,
            bundled: None,
            available_tools: vec![],
        })
    }
}

impl Transport {
    fn config(&self, name: String, description: String) -> Result<TransportConfig> {
        ensure!(
            self.kind == "streamable-http",
            "unsupported transport '{}'",
            self.kind
        );
        let uri = self.url.as_deref().context("remote is missing a URL")?;
        ensure!(
            self.variables.is_empty() && !uri.contains(['{', '}', '$']),
            "URL templates are not supported"
        );
        let url = url::Url::parse(uri).context("invalid remote URL")?;
        ensure!(
            matches!(url.scheme(), "https" | "http")
                && url.username().is_empty()
                && url.password().is_none(),
            "remote URLs must use HTTP(S) without embedded credentials"
        );
        let (headers, env_keys) = named_inputs(&self.headers, true)?;
        Ok(TransportConfig::StreamableHttp {
            name,
            description,
            uri: uri.to_owned(),
            envs: Envs::default(),
            env_keys,
            headers,
            timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        })
    }
}

fn arguments(items: &[Argument]) -> Result<Vec<String>> {
    let mut args = Vec::new();
    for arg in items {
        let (name, input) = match arg {
            Argument::Named { value } => (Some(&value.name), &value.input),
            Argument::Positional { value } => (None, value),
        };
        ensure!(!input.is_secret, "secret arguments are not supported");
        if let Some(value) = input.literal()? {
            ensure!(!value.contains('\0'), "invalid argument value");
            if let Some(name) = name {
                ensure!(
                    name.starts_with('-') && !name.contains('\0'),
                    "invalid named argument"
                );
                args.push(name.clone());
            }
            args.push(value);
        } else {
            ensure!(
                !input.is_required,
                "required arguments must provide a value or default"
            );
        }
    }
    Ok(args)
}

fn named_inputs(
    items: &[NamedInput],
    headers: bool,
) -> Result<(HashMap<String, String>, Vec<String>)> {
    let mut literals = HashMap::new();
    let mut env_keys = Vec::new();
    for NamedInput { name, input } in items {
        let value = input.literal().with_context(|| format!("input {name:?}"))?;
        if headers {
            reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .context("invalid header name")?;
        } else {
            ensure!(
                !name.is_empty() && !name.contains(['=', '\0']),
                "invalid environment variable name"
            );
        }
        if (!headers && value.is_some()) || (value.is_none() && input.is_required) {
            ensure!(
                !Envs::is_disallowed(name),
                "disallowed environment variable '{name}'"
            );
        }
        if let Some(value) = value {
            if headers {
                reqwest::header::HeaderValue::from_str(&value).context("invalid header value")?;
            } else {
                ensure!(!value.contains('\0'), "invalid environment variable value");
            }
            literals.insert(name.clone(), value);
        } else if input.is_required {
            env_keys.push(name.clone());
            if headers {
                literals.insert(name.clone(), format!("${{{name}}}"));
            }
        }
    }
    env_keys.sort_unstable();
    env_keys.dedup();
    Ok((literals, env_keys))
}

pub fn preview(entry: &ExtensionEntry) -> String {
    match &entry.config {
        ExtensionConfig::External(TransportConfig::Stdio { cmd, args, .. }) => {
            shell_words::join(std::iter::once(cmd).chain(args))
        }
        ExtensionConfig::External(TransportConfig::StreamableHttp { uri, .. }) => uri.clone(),
        _ => entry.config.name(),
    }
}

pub async fn install(config: &crate::config::Config, entries: &[ExtensionEntry]) -> Result<()> {
    ensure!(!entries.is_empty(), "no MCP extensions selected");
    if let Some(location) = std::env::var_os("GOOSE_ALLOWLIST") {
        let url = url::Url::parse(location.to_str().context("invalid GOOSE_ALLOWLIST URL")?)?;
        ensure!(url.scheme() == "https", "GOOSE_ALLOWLIST requires HTTPS");
        let response = reqwest::Client::builder()
            .https_only(true)
            .timeout(std::time::Duration::from_secs(30))
            .build()?
            .get(url)
            .send()
            .await?
            .error_for_status()?;
        let policy: Allowlist = serde_yaml::from_str(&response.text().await?)?;
        policy.check(entries)?;
    }
    crate::config::extensions::add_extensions(config, entries)
}

#[derive(Deserialize)]
struct Allowlist {
    extensions: Vec<AllowedExtension>,
}

#[derive(Deserialize)]
struct AllowedExtension {
    command: String,
}

impl Allowlist {
    fn check(&self, entries: &[ExtensionEntry]) -> Result<()> {
        let commands: Vec<_> = self
            .extensions
            .iter()
            .map(|entry| {
                ensure!(
                    !entry.command.trim().is_empty(),
                    "empty command in GOOSE_ALLOWLIST"
                );
                Ok(&entry.command)
            })
            .collect::<Result<_>>()?;
        for entry in entries {
            let allowed = commands.iter().any(|command| match &entry.config {
                ExtensionConfig::External(TransportConfig::Stdio { cmd, args, .. }) => {
                    shell_words::split(command)
                        .is_ok_and(|words| words.iter().eq(std::iter::once(cmd).chain(args)))
                }
                ExtensionConfig::External(TransportConfig::StreamableHttp { uri, .. }) => {
                    *command == uri
                }
                _ => false,
            });
            ensure!(
                allowed,
                "extension '{}' is not permitted by GOOSE_ALLOWLIST",
                entry.config.name()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn server(packages: Vec<Value>, remotes: Vec<Value>) -> ServerDocument {
        ServerDocument::parse(
            &json!({
                "name": "io.example/weather", "description": "test",
                "packages": packages, "remotes": remotes,
            })
            .to_string(),
        )
        .unwrap()
    }

    #[test]
    fn oci_honors_runtime_version_and_only_forwards_resolved_env() {
        let document = server(
            vec![json!({
                "registryType": "oci", "identifier": "example/weather", "version": "1.0.0",
                "runtimeHint": "podman", "transport": { "type": "stdio" },
                "environmentVariables": [
                    { "name": "TOKEN", "isRequired": true, "isSecret": true },
                    { "name": "OPTIONAL", "isSecret": true }
                ],
                "packageArguments": [{ "type": "named", "name": "--optional" }]
            })],
            vec![],
        );
        let entry = document.extension(ServerSelection::Package(0)).unwrap();
        let ExtensionConfig::External(TransportConfig::Stdio {
            cmd,
            args,
            envs,
            env_keys,
            ..
        }) = entry.config
        else {
            panic!("expected stdio")
        };
        assert_eq!(cmd, "podman");
        assert_eq!(
            args,
            ["run", "--rm", "-i", "-e", "TOKEN", "example/weather:1.0.0"]
        );
        assert_eq!(env_keys, ["TOKEN"]);
        assert!(envs.get_env().is_empty());
    }

    #[test]
    fn remote_emits_required_header_references_and_omits_optional_secrets() {
        let document = server(
            vec![],
            vec![json!({
                "type": "streamable-http", "url": "https://example.org/mcp",
                "headers": [
                    { "name": "X.API-Key", "isRequired": true, "isSecret": true },
                    { "name": "X-Optional", "isSecret": true }
                ]
            })],
        );
        let entry = document.extension(ServerSelection::Remote(0)).unwrap();
        assert_eq!(entry.config.name(), "io.example/weather");
        let ExtensionConfig::External(TransportConfig::StreamableHttp {
            headers, env_keys, ..
        }) = entry.config
        else {
            panic!("expected a remote")
        };
        assert_eq!(env_keys, ["X.API-Key"]);
        assert_eq!(
            headers,
            HashMap::from([("X.API-Key".into(), "${X.API-Key}".into())])
        );
    }

    #[test]
    fn rejects_secret_arguments_in_either_position() {
        for field in ["packageArguments", "runtimeArguments"] {
            let document = server(
                vec![json!({
                    "registryType": "npm", "identifier": "weather", "transport": { "type": "stdio" },
                    (field): [{ "type": "named", "name": "--token", "isSecret": true, "value": "not-for-config" }]
                })],
                vec![],
            );
            let error = document.extension(ServerSelection::Package(0)).unwrap_err();
            assert!(error.to_string().contains("secret arguments"));
            assert!(!error.to_string().contains("not-for-config"));
        }
    }

    #[test]
    fn rejects_nested_inputs_and_embedded_secrets() {
        for (input, reason) in [
            (
                json!({ "name": "Authorization", "value": "Bearer {token}", "variables": { "token": { "isSecret": true } } }),
                "input templates",
            ),
            (
                json!({ "name": "Authorization", "default": "not-for-config", "isSecret": true }),
                "embedded secret",
            ),
        ] {
            let document = server(
                vec![],
                vec![
                    json!({ "type": "streamable-http", "url": "https://example.org/mcp", "headers": [input] }),
                ],
            );
            let error = document.extension(ServerSelection::Remote(0)).unwrap_err();
            assert!(format!("{error:#}").contains(reason));
            assert!(!format!("{error:#}").contains("not-for-config"));
        }
    }

    #[test]
    fn import_persists_namespaced_remotes_without_touching_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.yaml");
        let secrets = dir.path().join("secrets.yaml");
        let config = crate::config::Config::new_with_file_secrets(&file, &secrets).unwrap();
        let entries =
            ["io.example/weather", "io.other/weather"].map(|name| {
                ServerDocument::parse(&json!({
                "name": name, "description": "weather",
                "remotes": [{ "type": "streamable-http", "url": "https://example.org/mcp" }],
            }).to_string()).unwrap().extension(ServerSelection::Remote(0)).unwrap()
            });
        crate::config::extensions::add_extensions(&config, &entries).unwrap();
        let saved = crate::config::extensions::get_enabled_extensions_with_config(&config);
        for entry in entries {
            assert!(saved.contains(&entry.config));
        }
        assert!(!secrets.exists());
    }

    #[test]
    fn allowlist_matches_argument_boundaries() {
        let document = server(
            vec![json!({
                "registryType": "npm", "identifier": "weather", "transport": { "type": "stdio" },
                "packageArguments": [{ "type": "positional", "value": "two words" }]
            })],
            vec![],
        );
        let entries = [document.extension(ServerSelection::Package(0)).unwrap()];
        for (command, allowed) in [
            ("npx -y weather 'two words'", true),
            ("npx -y weather two words", false),
            ("npx -y weather", false),
        ] {
            let policy = Allowlist {
                extensions: vec![AllowedExtension {
                    command: command.into(),
                }],
            };
            assert_eq!(policy.check(&entries).is_ok(), allowed);
        }
    }
}
