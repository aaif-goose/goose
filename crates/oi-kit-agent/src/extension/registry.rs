//! Convert MCP Registry `server.json` entries to external extension configs.
//! Conversion does not run launchers, resolve credentials, or persist config.
use super::{Envs, ExtensionConfig, DEFAULT_EXTENSION_TIMEOUT};
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
    format: Option<String>,
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
    /// Parse the Registry document without selecting or validating launch configurations.
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

    /// List package and remote selections in document order.
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

    /// Convert selected entries without resolving credentials or starting servers.
    pub fn extensions(&self, selections: &[ServerSelection]) -> Result<Vec<ExtensionConfig>> {
        selections
            .iter()
            .map(|&selection| self.extension(selection))
            .collect()
    }

    fn extension(&self, selection: ServerSelection) -> Result<ExtensionConfig> {
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
        match selection {
            ServerSelection::Package(i) => {
                let package = self.packages.get(i).context("package index out of range")?;
                package.config(name, description)
            }
            ServerSelection::Remote(i) => self
                .remotes
                .get(i)
                .context("remote index out of range")?
                .config(name, description),
        }
    }
}

impl Package {
    fn config(&self, name: String, description: String) -> Result<ExtensionConfig> {
        ensure!(
            self.transport.kind == "stdio",
            "unsupported package transport '{}'; packages must use stdio",
            self.transport.kind
        );
        ensure!(
            self.file_sha256.is_none(),
            "package integrity hashes are not supported"
        );
        ensure!(
            !self.identifier.is_empty()
                && !self.identifier.starts_with('-')
                && !self.identifier.contains('\0'),
            "invalid package identifier"
        );
        ensure!(
            self.version
                .as_deref()
                .is_none_or(|version| !version.is_empty() && !version.contains('\0')),
            "invalid package version"
        );
        let (env, env_keys) = named_inputs(&self.environment_variables, false)?;
        let mut args = arguments(&self.runtime_arguments)?;
        let (cmd, package) = match self.registry_type.as_str() {
            "npm" => {
                let cmd = self.runtime("https://registry.npmjs.org", "npx", &[])?;
                args.push("-y".into());
                (cmd, self.versioned_identifier("@"))
            }
            "pypi" => (
                self.runtime("https://pypi.org", "uvx", &[])?,
                self.versioned_identifier("=="),
            ),
            "oci" => {
                let cmd = self.runtime("https://docker.io", "docker", &["podman"])?;
                args.insert(0, "run".into());
                args.extend(["--rm".into(), "-i".into()]);
                let mut keys: Vec<_> = env.keys().chain(&env_keys).collect();
                keys.sort_unstable();
                for key in keys {
                    args.extend(["-e".into(), key.to_owned()]);
                }
                let separator = if self
                    .version
                    .as_deref()
                    .is_some_and(|version| version.contains(':'))
                {
                    "@"
                } else {
                    ":"
                };
                (cmd, self.versioned_identifier(separator))
            }
            other => bail!("unsupported package registry type '{other}'"),
        };
        args.push(package);
        args.extend(arguments(&self.package_arguments)?);
        Ok(ExtensionConfig::Stdio {
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

    fn runtime(
        &self,
        public_registry: &str,
        default: &str,
        alternatives: &[&str],
    ) -> Result<String> {
        ensure!(
            self.registry_base_url
                .as_deref()
                .is_none_or(|url| url.trim_end_matches('/') == public_registry),
            "custom package registries are not supported"
        );
        let cmd = self.runtime_hint.as_deref().unwrap_or(default);
        ensure!(
            cmd == default || alternatives.contains(&cmd),
            "runtime hint {cmd:?} is incompatible with {} packages",
            self.registry_type
        );
        Ok(cmd.to_owned())
    }

    fn versioned_identifier(&self, separator: &str) -> String {
        match &self.version {
            Some(version) => format!("{}{separator}{version}", self.identifier),
            None => self.identifier.clone(),
        }
    }
}

impl Transport {
    fn config(&self, name: String, description: String) -> Result<ExtensionConfig> {
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
        Ok(ExtensionConfig::StreamableHttp {
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
                if input.format.as_deref() == Some("boolean") {
                    ensure!(
                        matches!(value.as_str(), "true" | "false"),
                        "invalid boolean argument value"
                    );
                    args.push(format!("{name}={value}"));
                    continue;
                }
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
                "runtimeArguments": [
                    { "type": "named", "name": "--read-only", "format": "boolean", "value": "true" },
                    { "type": "named", "name": "--init", "format": "boolean", "default": "false" }
                ],
                "packageArguments": [{ "type": "named", "name": "--optional" }]
            })],
            vec![],
        );
        let extension = document.extension(ServerSelection::Package(0)).unwrap();
        let ExtensionConfig::Stdio {
            cmd,
            args,
            envs,
            env_keys,
            ..
        } = extension
        else {
            panic!("expected stdio")
        };
        assert_eq!(cmd, "podman");
        assert_eq!(
            args,
            [
                "run",
                "--read-only=true",
                "--init=false",
                "--rm",
                "-i",
                "-e",
                "TOKEN",
                "example/weather:1.0.0"
            ]
        );
        assert_eq!(env_keys, ["TOKEN"]);
        assert!(envs.get_env().is_empty());
    }

    #[test]
    fn http_transport_is_supported_for_remotes_but_not_packages() {
        let http = json!({ "type": "streamable-http", "url": "http://localhost:3000/mcp" });
        let document = server(
            vec![json!({
                "registryType": "npm", "identifier": "weather", "transport": http.clone(),
            })],
            vec![http],
        );
        let error = document.extension(ServerSelection::Package(0)).unwrap_err();
        assert!(error.to_string().contains("packages must use stdio"));
        assert!(matches!(
            document.extension(ServerSelection::Remote(0)),
            Ok(ExtensionConfig::StreamableHttp { .. })
        ));
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
        let extension = document.extension(ServerSelection::Remote(0)).unwrap();
        assert_eq!(extension.name(), "io.example/weather");
        let ExtensionConfig::StreamableHttp {
            headers, env_keys, ..
        } = extension
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
    fn package_launchers_preserve_argument_order() {
        for (registry, hint, identifier, command, install_args) in [
            (
                "npm",
                None,
                "@example/weather",
                "npx",
                vec!["-y", "@example/weather@1.2.3"],
            ),
            (
                "pypi",
                Some("uvx"),
                "weather",
                "uvx",
                vec!["weather==1.2.3"],
            ),
            (
                "oci",
                None,
                "example/weather",
                "docker",
                vec!["--rm", "-i", "example/weather:1.2.3"],
            ),
        ] {
            let document = server(
                vec![json!({
                    "registryType": registry, "identifier": identifier, "version": "1.2.3",
                    "runtimeHint": hint, "transport": { "type": "stdio" },
                    "runtimeArguments": [{ "type": "named", "name": "--runtime-option", "value": "one value" }],
                    "packageArguments": [{ "type": "named", "name": "--package-option", "value": "another value" }],
                })],
                vec![],
            );
            let ExtensionConfig::Stdio { cmd, args, .. } =
                document.extension(ServerSelection::Package(0)).unwrap()
            else {
                panic!("expected stdio");
            };
            let mut expected = if registry == "oci" {
                vec!["run"]
            } else {
                vec![]
            };
            expected.extend(["--runtime-option", "one value"]);
            expected.extend(install_args);
            expected.extend(["--package-option", "another value"]);
            assert_eq!(cmd, command);
            assert_eq!(args, expected);
        }
    }

    #[test]
    fn rejects_incompatible_runtime_hints() {
        for (registry, hint) in [("npm", "uvx"), ("pypi", "npx"), ("oci", "npx"), ("npm", "")] {
            let document = server(
                vec![json!({
                    "registryType": registry, "identifier": "weather", "runtimeHint": hint,
                    "transport": { "type": "stdio" },
                })],
                vec![],
            );
            let error = document.extension(ServerSelection::Package(0)).unwrap_err();
            assert!(error.to_string().contains("incompatible"));
        }
    }

    #[test]
    fn rejects_unsupported_launchers() {
        for registry in ["nuget", "cargo", "mcpb"] {
            let document = server(
                vec![json!({
                    "registryType": registry, "identifier": "weather", "transport": { "type": "stdio" },
                })],
                vec![],
            );
            let error = document.extension(ServerSelection::Package(0)).unwrap_err();
            assert!(error
                .to_string()
                .contains("unsupported package registry type"));
        }
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
    fn registry_namespace_is_preserved_in_extension_name() {
        let names = ["io.example/weather", "io.other/weather"];
        let configs =
            names.map(|name| {
                ServerDocument::parse(&json!({
                "name": name, "description": "weather",
                "remotes": [{ "type": "streamable-http", "url": "https://example.org/mcp" }],
            }).to_string()).unwrap().extension(ServerSelection::Remote(0)).unwrap()
            });
        assert_eq!(configs.each_ref().map(ExtensionConfig::name), names);
        assert_ne!(configs[0].key(), configs[1].key());
    }
}
