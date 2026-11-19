use super::{extensions::add_extensions, Config, ExtensionConfig, ExtensionEntry};
use crate::agents::extension::TransportConfig;
use anyhow::{ensure, Context, Result};
use serde::Deserialize;

pub fn preview(extension: &TransportConfig) -> String {
    match extension {
        TransportConfig::Stdio { cmd, args, .. } => {
            shell_words::join(std::iter::once(cmd).chain(args))
        }
        TransportConfig::StreamableHttp { uri, .. } => uri.clone(),
    }
}

/// Installs external extensions that the caller has already confirmed.
pub async fn install(config: &Config, extensions: &[TransportConfig]) -> Result<()> {
    ensure!(!extensions.is_empty(), "no MCP extensions selected");
    let policy = match std::env::var_os("GOOSE_ALLOWLIST") {
        Some(location) => {
            Some(Allowlist::fetch(location.to_str().context("invalid GOOSE_ALLOWLIST URL")?).await?)
        }
        None => None,
    };
    install_with_policy(config, extensions, policy.as_ref())
}

fn install_with_policy(
    config: &Config,
    extensions: &[TransportConfig],
    policy: Option<&Allowlist>,
) -> Result<()> {
    if let Some(policy) = policy {
        policy.check(extensions)?;
    }
    let entries = extensions
        .iter()
        .cloned()
        .map(|extension| ExtensionEntry {
            enabled: true,
            config: ExtensionConfig::External(extension),
        })
        .collect::<Vec<_>>();
    add_extensions(config, &entries)
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
    async fn fetch(location: &str) -> Result<Self> {
        let url = url::Url::parse(location)?;
        ensure!(url.scheme() == "https", "GOOSE_ALLOWLIST requires HTTPS");
        let text = reqwest::Client::builder()
            .https_only(true)
            .timeout(std::time::Duration::from_secs(30))
            .build()?
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(serde_yaml::from_str(&text)?)
    }

    fn check(&self, extensions: &[TransportConfig]) -> Result<()> {
        ensure!(
            self.extensions
                .iter()
                .all(|entry| !entry.command.trim().is_empty()),
            "empty command in GOOSE_ALLOWLIST"
        );
        for extension in extensions {
            let allowed = self.extensions.iter().any(|entry| match extension {
                TransportConfig::Stdio { cmd, args, .. } => shell_words::split(&entry.command)
                    .is_ok_and(|words| words.iter().eq(std::iter::once(cmd).chain(args))),
                TransportConfig::StreamableHttp { uri, socket, .. } => {
                    socket.is_none() && !uri.contains('$') && entry.command == *uri
                }
            });
            ensure!(
                allowed,
                "extension '{}' is not permitted by GOOSE_ALLOWLIST",
                extension.name()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        extensions::get_enabled_extensions_with_config, DEFAULT_EXTENSION_TIMEOUT,
    };

    #[test]
    fn install_checks_and_saves_the_previewed_config() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.yaml");
        let secrets = dir.path().join("secrets.yaml");
        let config = Config::new_with_file_secrets(&file, &secrets).unwrap();
        let extension =
            TransportConfig::stdio("weather", "npx", "weather tools", DEFAULT_EXTENSION_TIMEOUT)
                .with_args(["-y", "weather", "two words"]);
        let remote = TransportConfig::streamable_http(
            "remote",
            "https://example.org/mcp",
            "remote tools",
            DEFAULT_EXTENSION_TIMEOUT,
        );
        let batch = [extension, remote];
        let allowlist = |commands: [String; 2]| Allowlist {
            extensions: commands
                .into_iter()
                .map(|command| AllowedExtension { command })
                .collect(),
        };
        let [stdio, http] = batch.each_ref().map(preview);
        for invalid in [
            ["npx -y weather two words".into(), http.clone()],
            ["npx -y weather".into(), http.clone()],
            [stdio.clone(), "https://example.org/other".into()],
        ] {
            assert!(install_with_policy(&config, &batch, Some(&allowlist(invalid))).is_err());
            assert!(!file.exists());
        }
        install_with_policy(&config, &batch, Some(&allowlist([stdio, http]))).unwrap();
        let saved = get_enabled_extensions_with_config(&config);
        for extension in batch {
            assert!(saved.contains(&ExtensionConfig::External(extension)));
        }
        assert!(!secrets.exists());
    }
}
