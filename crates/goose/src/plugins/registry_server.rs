use crate::agents::extension::{Envs, ExtensionConfig};
use crate::config::{ExtensionEntry, DEFAULT_EXTENSION_TIMEOUT};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerDocument {
    name: String,
    description: String,
    #[serde(default)]
    packages: Vec<Package>,
    #[serde(default)]
    remotes: Vec<Transport>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Package {
    registry_type: String,
    identifier: String,
    version: Option<String>,
    runtime_hint: Option<String>,
    transport: Transport,
    #[serde(default)]
    runtime_arguments: Vec<Argument>,
    #[serde(default)]
    package_arguments: Vec<Argument>,
    #[serde(default)]
    environment_variables: Vec<Input>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Transport {
    #[serde(rename = "type")]
    kind: String,
    url: Option<String>,
    #[serde(default)]
    headers: Vec<Input>,
    #[serde(default)]
    variables: HashMap<String, Input>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    name: Option<String>,
    value: Option<String>,
    default: Option<String>,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    is_secret: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Argument {
    #[serde(rename = "type")]
    kind: String,
    name: Option<String>,
    value: Option<String>,
    default: Option<String>,
    value_hint: Option<String>,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    variables: HashMap<String, Input>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerSelection {
    Package(usize),
    Remote(usize),
}

pub fn server_json_choices(json: &str) -> Result<Vec<(ServerSelection, String)>> {
    let document: ServerDocument =
        serde_json::from_str(json).context("invalid MCP registry server.json")?;
    let packages = document
        .packages
        .iter()
        .enumerate()
        .map(|(index, package)| {
            (
                ServerSelection::Package(index),
                format!(
                    "Package {index}: {} {} ({})",
                    package.identifier,
                    package.version.as_deref().unwrap_or(""),
                    package.registry_type
                ),
            )
        });
    let remotes = document.remotes.iter().enumerate().map(|(index, remote)| {
        (
            ServerSelection::Remote(index),
            format!(
                "Remote {index}: {} ({})",
                remote.url.as_deref().unwrap_or("missing URL"),
                remote.kind
            ),
        )
    });
    Ok(packages.chain(remotes).collect())
}

pub fn import_server_json(
    json: &str,
    selections: &[ServerSelection],
    values: &HashMap<String, String>,
) -> Result<(Vec<ExtensionEntry>, HashMap<String, String>)> {
    let document: ServerDocument =
        serde_json::from_str(json).context("invalid MCP registry server.json")?;
    if selections.is_empty() {
        bail!("no package or remote selected")
    }
    let base_name = document.name.rsplit('/').next().unwrap_or(&document.name);
    let multiple = selections.len() > 1;
    let mut secret_names = vec![];
    for selection in selections {
        match *selection {
            ServerSelection::Package(index) => {
                if let Some(package) = document.packages.get(index) {
                    if package.transport.kind == "stdio" {
                        secret_names.extend(secret_input_names(&package.environment_variables));
                        ensure_no_secret_argument_variables(&package.runtime_arguments)?;
                        ensure_no_secret_argument_variables(&package.package_arguments)?;
                    } else {
                        secret_names.extend(secret_transport_input_names(&package.transport));
                    }
                }
            }
            ServerSelection::Remote(index) => {
                if let Some(remote) = document.remotes.get(index) {
                    secret_names.extend(secret_transport_input_names(remote));
                }
            }
        }
    }
    secret_names.sort_unstable();
    secret_names.dedup();
    let secrets = secret_names
        .iter()
        .filter_map(|name| values.get(name).map(|value| (name.clone(), value.clone())))
        .collect();
    let public_values = values
        .iter()
        .filter(|(name, _)| !secret_names.contains(name))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let entries = selections
        .iter()
        .map(|selection| {
            let (name, config) = match *selection {
                ServerSelection::Package(index) => {
                    let package = document
                        .packages
                        .get(index)
                        .with_context(|| format!("package index {index} is out of range"))?;
                    let name = if multiple {
                        format!("{base_name}-package-{index}")
                    } else {
                        base_name.to_string()
                    };
                    let config = package_config(
                        name.clone(),
                        document.description.clone(),
                        package,
                        &public_values,
                    )?;
                    (name, config)
                }
                ServerSelection::Remote(index) => {
                    let remote = document
                        .remotes
                        .get(index)
                        .with_context(|| format!("remote index {index} is out of range"))?;
                    let name = if multiple {
                        format!("{base_name}-remote-{index}")
                    } else {
                        base_name.to_string()
                    };
                    let config = remote_config(
                        name.clone(),
                        document.description.clone(),
                        remote,
                        &public_values,
                    )?;
                    (name, config)
                }
            };
            let _ = name;
            Ok(ExtensionEntry {
                enabled: true,
                config,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((entries, secrets))
}

fn secret_input_names(inputs: &[Input]) -> impl Iterator<Item = String> + '_ {
    inputs
        .iter()
        .filter(|input| input.is_secret)
        .filter_map(|input| input.name.clone())
}

fn secret_transport_input_names(transport: &Transport) -> Vec<String> {
    secret_input_names(&transport.headers)
        .chain(
            transport
                .variables
                .iter()
                .filter(|(_, input)| input.is_secret)
                .map(|(name, _)| name.clone()),
        )
        .collect()
}

fn ensure_no_secret_argument_variables(arguments: &[Argument]) -> Result<()> {
    if let Some(name) = arguments.iter().find_map(|argument| {
        argument
            .variables
            .iter()
            .find(|(_, input)| input.is_secret)
            .map(|(name, _)| name)
    }) {
        bail!("secret argument variable '{name}' is not supported; use an environment variable")
    }
    Ok(())
}

fn package_config(
    name: String,
    description: String,
    package: &Package,
    values: &HashMap<String, String>,
) -> Result<ExtensionConfig> {
    if package.transport.kind != "stdio" {
        return remote_config(name, description, &package.transport, values);
    }
    let (cmd, mut args) = runtime(package)?;
    args.extend(resolve_arguments(&package.runtime_arguments, values)?);
    let (envs, env_keys) = resolve_inputs(&package.environment_variables, values)?;
    match package.registry_type.as_str() {
        "npm" => {
            args.push("-y".into());
            args.push(versioned(
                &package.identifier,
                package.version.as_deref(),
                "@",
            ));
        }
        "pypi" => args.push(versioned(
            &package.identifier,
            package.version.as_deref(),
            "==",
        )),
        "oci" => {
            args.extend(["--rm".into(), "-i".into()]);
            for input in &package.environment_variables {
                let name = input
                    .name
                    .as_deref()
                    .context("environment variable is missing name")?;
                args.extend(["-e".into(), name.into()]);
            }
            args.push(versioned(
                &package.identifier,
                package.version.as_deref(),
                ":",
            ));
        }
        "nuget" => args.push(versioned(
            &package.identifier,
            package.version.as_deref(),
            "@",
        )),
        "cargo" => {
            anyhow::ensure!(
                package.runtime_hint.is_none(),
                "cargo packages do not support runtimeHint"
            );
        }
        "mcpb" => bail!(
            "MCPB packages require bundle download, SHA-256 verification, extraction, and manifest processing; Goose does not yet support importing them"
        ),
        _other if package.runtime_hint.is_some() => args.push(package.identifier.clone()),
        other => bail!("unsupported registryType '{other}'; a runtimeHint is required"),
    }
    if package.registry_type == "nuget" && !package.package_arguments.is_empty() {
        args.push("--".into());
    }
    args.extend(resolve_arguments(&package.package_arguments, values)?);
    Ok(ExtensionConfig::Stdio {
        name,
        description,
        cmd,
        args,
        envs: Envs::new(envs),
        env_keys,
        timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
        cwd: None,
        bundled: Some(false),
        available_tools: vec![],
    })
}

fn runtime(package: &Package) -> Result<(String, Vec<String>)> {
    if let Some(runtime) = &package.runtime_hint {
        let args = if package.registry_type == "oci" {
            vec!["run".into()]
        } else {
            vec![]
        };
        return Ok((runtime.clone(), args));
    }
    Ok(match package.registry_type.as_str() {
        "npm" => ("npx".into(), vec![]),
        "pypi" => ("uvx".into(), vec![]),
        "oci" => ("docker".into(), vec!["run".into()]),
        "nuget" => ("dnx".into(), vec![]),
        "cargo" => (package.identifier.clone(), vec![]),
        "mcpb" => bail!(
            "MCPB packages require bundle download, SHA-256 verification, extraction, and manifest processing; Goose does not yet support importing them"
        ),
        other => bail!("unsupported registryType '{other}'"),
    })
}

fn remote_config(
    name: String,
    description: String,
    transport: &Transport,
    values: &HashMap<String, String>,
) -> Result<ExtensionConfig> {
    if transport.kind != "streamable-http" {
        bail!(
            "unsupported transport '{}'; Goose supports stdio and streamable-http",
            transport.kind
        );
    }
    let (vars, variable_env_keys) = resolve_transport_variables(&transport.variables, values)?;
    let uri = substitute(
        transport
            .url
            .as_deref()
            .context("streamable-http transport is missing url")?,
        &vars,
    );
    let (headers, mut env_keys) = resolve_named_inputs(&transport.headers, values)?;
    env_keys.extend(variable_env_keys);
    env_keys.sort_unstable();
    env_keys.dedup();
    Ok(ExtensionConfig::StreamableHttp {
        name,
        description,
        uri,
        envs: Envs::default(),
        env_keys,
        headers,
        timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
        socket: None,
        client_id: None,
        client_secret_key: None,
        scopes: vec![],
        bundled: Some(false),
        available_tools: vec![],
    })
}

fn resolve_arguments(items: &[Argument], values: &HashMap<String, String>) -> Result<Vec<String>> {
    let mut out = vec![];
    for item in items {
        let vars = resolve_variable_map(&item.variables, values)?;
        let key = item.value_hint.as_deref().or(item.name.as_deref());
        let value = item
            .value
            .clone()
            .or_else(|| key.and_then(|k| values.get(k).cloned()))
            .or_else(|| item.default.clone());
        if item.is_required && value.is_none() {
            bail!(
                "missing required value '{}'; pass --value {}=VALUE",
                key.unwrap_or("argument"),
                key.unwrap_or("argument")
            );
        }
        if let Some(value) = value {
            if item.kind == "named" {
                out.push(
                    item.name
                        .clone()
                        .context("named argument is missing name")?,
                );
            }
            out.push(substitute(&value, &vars));
        }
    }
    Ok(out)
}

fn resolve_inputs(
    items: &[Input],
    values: &HashMap<String, String>,
) -> Result<(HashMap<String, String>, Vec<String>)> {
    let mut fixed = HashMap::new();
    let mut keys = vec![];
    for input in items {
        let name = input
            .name
            .as_deref()
            .context("environment variable is missing name")?;
        if input.is_secret {
            keys.push(name.into());
            continue;
        }
        if let Some(value) = input
            .value
            .clone()
            .or_else(|| values.get(name).cloned())
            .or_else(|| input.default.clone())
        {
            fixed.insert(name.into(), value);
        } else if input.is_required {
            keys.push(name.into());
        }
    }
    Ok((fixed, keys))
}
fn resolve_named_inputs(
    items: &[Input],
    values: &HashMap<String, String>,
) -> Result<(HashMap<String, String>, Vec<String>)> {
    let mut fixed = HashMap::new();
    let mut keys = vec![];
    for input in items {
        let name = input.name.as_deref().context("header is missing name")?;
        if input.is_secret {
            keys.push(name.into());
            fixed.insert(name.into(), format!("${{{name}}}"));
            continue;
        }
        if let Some(v) = input
            .value
            .clone()
            .or_else(|| values.get(name).cloned())
            .or_else(|| input.default.clone())
        {
            fixed.insert(name.into(), v);
        } else if input.is_required {
            keys.push(name.into());
            fixed.insert(name.into(), format!("${{{name}}}"));
        }
    }
    Ok((fixed, keys))
}
fn resolve_transport_variables(
    inputs: &HashMap<String, Input>,
    values: &HashMap<String, String>,
) -> Result<(HashMap<String, String>, Vec<String>)> {
    let mut resolved = HashMap::new();
    let mut env_keys = vec![];
    for (name, input) in inputs {
        if input.is_secret {
            resolved.insert(name.clone(), format!("${{{name}}}"));
            env_keys.push(name.clone());
            continue;
        }
        let value = input
            .value
            .clone()
            .or_else(|| values.get(name).cloned())
            .or_else(|| input.default.clone());
        if input.is_required && value.is_none() {
            bail!("missing required value '{name}'; pass --value {name}=VALUE");
        }
        resolved.insert(name.clone(), value.unwrap_or_default());
    }
    Ok((resolved, env_keys))
}

fn resolve_variable_map(
    inputs: &HashMap<String, Input>,
    values: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    inputs
        .iter()
        .map(|(k, v)| {
            let value = v
                .value
                .clone()
                .or_else(|| values.get(k).cloned())
                .or_else(|| v.default.clone());
            if v.is_required && value.is_none() {
                bail!("missing required value '{k}'; pass --value {k}=VALUE");
            }
            Ok((k.clone(), value.unwrap_or_default()))
        })
        .collect()
}
fn substitute(template: &str, values: &HashMap<String, String>) -> String {
    values.iter().fold(template.to_string(), |s, (k, v)| {
        s.replace(&format!("{{{k}}}"), v)
    })
}
fn versioned(id: &str, version: Option<&str>, separator: &str) -> String {
    version.map_or_else(|| id.into(), |v| format!("{id}{separator}{v}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import_oci(runtime_hint: Option<&str>) -> ExtensionConfig {
        let runtime_hint = runtime_hint
            .map(|hint| format!(r#", "runtimeHint": "{hint}""#))
            .unwrap_or_default();
        let json = format!(
            r#"{{
                "name": "io.example/test-server",
                "description": "test",
                "packages": [{{
                    "registryType": "oci",
                    "identifier": "ghcr.io/example/test-server",
                    "version": "1.2.3"{runtime_hint},
                    "transport": {{ "type": "stdio" }}
                }}]
            }}"#
        );
        let (entries, _) =
            import_server_json(&json, &[ServerSelection::Package(0)], &HashMap::new()).unwrap();
        entries.into_iter().next().unwrap().config
    }

    #[test]
    fn oci_uses_declared_version() {
        let ExtensionConfig::Stdio { args, .. } = import_oci(None) else {
            panic!("expected stdio extension");
        };

        assert_eq!(args.last().unwrap(), "ghcr.io/example/test-server:1.2.3");
    }

    #[test]
    fn oci_runtime_hint_keeps_run_subcommand() {
        let ExtensionConfig::Stdio { cmd, args, .. } = import_oci(Some("podman")) else {
            panic!("expected stdio extension");
        };

        assert_eq!(cmd, "podman");
        assert_eq!(args.first().unwrap(), "run");
    }

    #[test]
    fn remote_transport_secrets_remain_references() {
        let json = r#"{
            "name": "io.example/test-server",
            "description": "test",
            "remotes": [{
                "type": "streamable-http",
                "url": "https://example.com/{token}",
                "headers": [{ "name": "Authorization", "isSecret": true }],
                "variables": { "token": { "isSecret": true } }
            }]
        }"#;
        let values = HashMap::from([
            ("Authorization".into(), "Bearer secret".into()),
            ("token".into(), "path-secret".into()),
        ]);

        let (entries, secrets) =
            import_server_json(json, &[ServerSelection::Remote(0)], &values).unwrap();
        let ExtensionConfig::StreamableHttp {
            uri,
            headers,
            env_keys,
            ..
        } = &entries[0].config
        else {
            panic!("expected streamable HTTP extension");
        };

        assert_eq!(uri, "https://example.com/${token}");
        assert_eq!(headers["Authorization"], "${Authorization}");
        assert_eq!(env_keys, &["Authorization", "token"]);
        assert_eq!(secrets, values);
    }

    #[test]
    fn secret_argument_variables_are_rejected() {
        let json = r#"{
            "name": "io.example/test-server",
            "description": "test",
            "packages": [{
                "registryType": "npm",
                "identifier": "test-server",
                "transport": { "type": "stdio" },
                "packageArguments": [{
                    "type": "positional",
                    "value": "{token}",
                    "variables": { "token": { "isSecret": true } }
                }]
            }]
        }"#;

        let error = import_server_json(
            json,
            &[ServerSelection::Package(0)],
            &HashMap::from([("token".into(), "secret".into())]),
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("secret argument variable 'token'"));
    }
}
