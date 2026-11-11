//! Native, resolved connection inputs. Hosts still own configuration and credentials.
use super::{Client, ClientOptions, Connector, TransportClient};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "mcp-stdio")]
#[derive(Clone, Copy, Debug, Default)]
pub enum StderrPolicy {
    #[default]
    Null,
    Inherit,
}

#[cfg(feature = "mcp-stdio")]
#[derive(Clone)]
pub struct StdioConnector {
    command: String,
    pub args: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
    pub inherit_env: bool,
    pub cwd: Option<std::path::PathBuf>,
    pub stderr: StderrPolicy,
    pub options: ClientOptions,
}

#[cfg(feature = "mcp-stdio")]
impl StdioConnector {
    pub fn new(command: impl Into<String>, options: ClientOptions) -> Result<Self> {
        let command = command.into();
        anyhow::ensure!(!command.trim().is_empty(), "MCP command must not be empty");
        Ok(Self {
            command,
            args: Vec::new(),
            env: Default::default(),
            inherit_env: false,
            cwd: None,
            stderr: StderrPolicy::Null,
            options,
        })
    }
}

#[cfg(feature = "mcp-stdio")]
#[async_trait]
impl Connector for StdioConnector {
    async fn connect(
        &self,
        _scope: &str,
        cancel: CancellationToken,
    ) -> Result<Option<Arc<dyn Client>>> {
        let mut command = tokio::process::Command::new(&self.command);
        if !self.inherit_env {
            command.env_clear();
        }
        command.args(&self.args).envs(&self.env).kill_on_drop(true);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        command.stderr(match self.stderr {
            StderrPolicy::Null => std::process::Stdio::null(),
            StderrPolicy::Inherit => std::process::Stdio::inherit(),
        });
        tokio::select! {
            biased;
            _ = cancel.cancelled() => anyhow::bail!("MCP connection cancelled"),
            result = TransportClient::stdio(command, self.options.clone()) => Ok(Some(Arc::new(result?))),
        }
    }
}

#[cfg(feature = "mcp-http")]
#[derive(Clone)]
pub struct HttpConnector {
    url: reqwest::Url,
    client: reqwest::Client,
    pub options: ClientOptions,
}

#[cfg(feature = "mcp-http")]
impl HttpConnector {
    /// Default clients never follow redirects, avoiding forwarding credentials to another origin.
    pub fn new(
        url: &str,
        headers: impl IntoIterator<Item = (String, String)>,
        options: ClientOptions,
    ) -> Result<Self> {
        let url = reqwest::Url::parse(url)?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
            "MCP URL must use http or https"
        );
        anyhow::ensure!(
            url.username().is_empty() && url.password().is_none(),
            "MCP URL credentials must be supplied as headers"
        );
        let mut defaults = reqwest::header::HeaderMap::new();
        for (key, value) in headers {
            defaults.insert(
                reqwest::header::HeaderName::from_bytes(key.as_bytes())?,
                reqwest::header::HeaderValue::from_str(&value)?,
            );
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .default_headers(defaults)
            .build()?;
        Ok(Self {
            url,
            client,
            options,
        })
    }

    /// The host assumes responsibility for headers, TLS, proxies and redirect policy.
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }
}

#[cfg(feature = "mcp-http")]
#[async_trait]
impl Connector for HttpConnector {
    async fn connect(
        &self,
        _scope: &str,
        cancel: CancellationToken,
    ) -> Result<Option<Arc<dyn Client>>> {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => anyhow::bail!("MCP connection cancelled"),
            result = TransportClient::http(
                self.client.clone(),
                rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(self.url.as_str()),
                self.options.clone(),
            ) => Ok(Some(Arc::new(result?))),
        }
    }
}
