use agent_client_protocol::schema::v1::McpServer;
use anyhow::Result;
use futures::future::BoxFuture;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::acp::{
    configured_model_for_provider, extension_configs_to_mcp_servers, AcpProvider,
    AcpProviderConfig, ACP_CURRENT_MODEL,
};
use crate::config::search_path::SearchPaths;
use crate::config::{Config, GooseMode};
use crate::providers::base::{
    current_working_dir, ProviderDef, ProviderDescriptor, ProviderMetadata,
};
use crate::providers::catalog::ProviderSetupMetadata;

pub(crate) const AGY_ACP_PROVIDER_NAME: &str = "agy-acp";
const AGY_ACP_DOC_URL: &str = "https://antigravity.google";
pub(crate) const AGY_ACP_BINARY: &str = "agy";

const MODE_DEFAULT: &str = "default";
const MODE_ACCEPT_EDITS: &str = "accept-edits";
const MODE_AUTO: &str = "auto";

/// Provider wrapping the Google Antigravity CLI (`agy acp --stdio`) over ACP.
pub struct AgyAcpProvider;

impl goose_providers::base::ProviderDescriptor for AgyAcpProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            AGY_ACP_PROVIDER_NAME,
            "Antigravity CLI (ACP)",
            "Use goose with Google Antigravity CLI via its native ACP server.",
            ACP_CURRENT_MODEL,
            vec![],
            AGY_ACP_DOC_URL,
            vec![],
        )
        .with_setup_steps(vec![
            "Install the Antigravity CLI (`agy`) and ensure it is on your PATH",
            "Run `agy acp --login` to authenticate with your Google account",
        ])
        .with_setup(
            ProviderSetupMetadata::cli_agent(AGY_ACP_BINARY, &["agy-acp", "agy", "antigravity"])
                .with_acp()
                .with_docs_url(AGY_ACP_DOC_URL)
                .with_capabilities(true, true, false)
                .show_only_when_installed(),
        )
    }
}

impl AgyAcpProvider {
    fn create(
        extensions: Vec<crate::config::ExtensionConfig>,
        working_dir: PathBuf,
        use_default_model: bool,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Box::pin(async move {
            let config = Config::global();
            let resolved_command = SearchPaths::builder().with_npm().resolve(AGY_ACP_BINARY)?;
            let goose_mode = config.resolve_goose_mode()?;
            let model = if use_default_model {
                ACP_CURRENT_MODEL.to_string()
            } else {
                configured_model_for_provider(config, AGY_ACP_PROVIDER_NAME)
            };

            let args = vec![
                "acp".to_string(),
                "--stdio".to_string(),
                "--process-per-session".to_string(),
            ];
            let session_config_options = if model == ACP_CURRENT_MODEL {
                vec![]
            } else {
                vec![("model".to_string(), model)]
            };

            let mode_mapping = HashMap::from([
                (GooseMode::Auto, vec![MODE_AUTO.to_string()]),
                (GooseMode::Approve, vec![MODE_DEFAULT.to_string()]),
                (GooseMode::SmartApprove, vec![MODE_ACCEPT_EDITS.to_string()]),
            ]);

            let provider_config = AcpProviderConfig {
                command: resolved_command,
                args,
                env: vec![],
                env_remove: vec![],
                work_dir: working_dir,
                mcp_servers: sanitize_mcp_servers(extension_configs_to_mcp_servers(&extensions)),
                session_mode_id: mode_mapping[&goose_mode].first().cloned(),
                session_config_options,
                model_config_option_id: Some("model".to_string()),
                mode_mapping,
                notification_callback: None,
            };

            let metadata = Self::metadata();
            AcpProvider::connect(metadata.name, goose_mode, provider_config).await
        })
    }
}

impl ProviderDef for AgyAcpProvider {
    type Provider = AcpProvider;
    const SESSION_BOUND: bool = true;
    const RUNS_OWN_TOOL_LOOP: bool = true;

    fn from_env(
        extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Self::from_env_with_working_dir(extensions, current_working_dir(), tls_config)
    }

    fn from_env_with_working_dir(
        extensions: Vec<crate::config::ExtensionConfig>,
        working_dir: PathBuf,
        _tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Self::create(extensions, working_dir, false)
    }

    fn from_env_with_default_model(
        extensions: Vec<crate::config::ExtensionConfig>,
        _tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Self::create(extensions, current_working_dir(), true)
    }
}

/// `agy` names MCP tools `mcp_<server>_<tool>` and rejects names failing
/// `^[a-zA-Z0-9_-]{1,64}$`. Map disallowed characters to `_` and cap length.
fn sanitize_mcp_server_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if sanitized.is_empty() {
        "mcp".to_string()
    } else {
        sanitized
    }
}

fn unique_mcp_server_name(raw: &str, seen: &mut HashSet<String>) -> String {
    let base = sanitize_mcp_server_name(raw);
    if seen.insert(base.clone()) {
        return base;
    }
    let mut index = 2usize;
    loop {
        let suffix = format!("_{index}");
        let prefix_len = 64 - suffix.len();
        let prefix: String = base.chars().take(prefix_len).collect();
        let candidate = format!("{prefix}{suffix}");
        if seen.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

fn sanitize_mcp_servers(servers: Vec<McpServer>) -> Vec<McpServer> {
    let mut seen = HashSet::new();
    servers
        .into_iter()
        .map(|server| match server {
            McpServer::Stdio(mut s) => {
                s.name = unique_mcp_server_name(&s.name, &mut seen);
                McpServer::Stdio(s)
            }
            McpServer::Http(mut h) => {
                h.name = unique_mcp_server_name(&h.name, &mut seen);
                McpServer::Http(h)
            }
            McpServer::Sse(mut s) => {
                s.name = unique_mcp_server_name(&s.name, &mut seen);
                McpServer::Sse(s)
            }
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{McpServerHttp, McpServerStdio};

    #[test]
    fn metadata_uses_acp_defaults() {
        let meta = AgyAcpProvider::metadata();
        assert_eq!(meta.name, AGY_ACP_PROVIDER_NAME);
        assert_eq!(meta.default_model, ACP_CURRENT_MODEL);
        assert!(meta.setup.is_some_and(|s| s.show_only_when_installed));
    }

    #[test]
    fn sanitize_mcp_server_name_enforces_agy_pattern() {
        assert_eq!(
            sanitize_mcp_server_name("Chrome DevTools"),
            "Chrome_DevTools"
        );
        assert_eq!(sanitize_mcp_server_name("my-server_1"), "my-server_1");
        assert_eq!(sanitize_mcp_server_name(""), "mcp");
        assert_eq!(sanitize_mcp_server_name(&"a".repeat(80)).len(), 64);
    }

    #[test]
    fn sanitize_mcp_servers_rewrites_stdio_and_http_names() {
        let servers = sanitize_mcp_servers(vec![
            McpServer::Stdio(McpServerStdio::new("Chrome DevTools", "npx")),
            McpServer::Http(McpServerHttp::new("My HTTP", "https://example.com/mcp")),
        ]);
        match &servers[0] {
            McpServer::Stdio(s) => assert_eq!(s.name, "Chrome_DevTools"),
            _ => panic!("expected stdio"),
        }
        match &servers[1] {
            McpServer::Http(h) => assert_eq!(h.name, "My_HTTP"),
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn sanitize_mcp_servers_disambiguates_collisions() {
        let long_a = format!("{}a", "x".repeat(64));
        let long_b = format!("{}b", "x".repeat(64));
        let servers = sanitize_mcp_servers(vec![
            McpServer::Stdio(McpServerStdio::new("Chrome DevTools", "npx")),
            McpServer::Stdio(McpServerStdio::new("Chrome_DevTools", "npx")),
            McpServer::Stdio(McpServerStdio::new(long_a, "npx")),
            McpServer::Stdio(McpServerStdio::new(long_b, "npx")),
        ]);
        let names: Vec<&str> = servers
            .iter()
            .map(|s| match s {
                McpServer::Stdio(st) => st.name.as_str(),
                _ => panic!("expected stdio"),
            })
            .collect();
        assert_eq!(names[0], "Chrome_DevTools");
        assert_eq!(names[1], "Chrome_DevTools_2");
        assert_eq!(names[2].len(), 64);
        assert_eq!(names[3].len(), 64);
        assert_ne!(names[2], names[3]);
    }
}
