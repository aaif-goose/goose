use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const ACP_URL_ENV: &str = "GOOSE_ACP_URL";
const PLUGIN_ID_ENV: &str = "GOOSE_PLUGIN_ID";
const PLUGIN_TOKEN_ENV: &str = "GOOSE_PLUGIN_TOKEN";

#[derive(Clone)]
pub struct HostClient {
    http: reqwest::Client,
    base_url: String,
    plugin_id: String,
    token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest<T: Serialize> {
    jsonrpc: &'static str,
    method: String,
    params: T,
    id: u64,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetFetchResult {
    pub ok: bool,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub text: String,
}

impl HostClient {
    pub fn from_env() -> Result<Self> {
        let base_url =
            std::env::var(ACP_URL_ENV).map_err(|_| anyhow::anyhow!("{ACP_URL_ENV} not set"))?;
        let plugin_id = std::env::var(PLUGIN_ID_ENV).unwrap_or_default();
        let token = std::env::var(PLUGIN_TOKEN_ENV).ok();
        Ok(Self {
            http: reqwest::Client::new(),
            base_url,
            plugin_id,
            token,
        })
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub async fn net_fetch(
        &self,
        url: &str,
        method: &str,
        headers: HashMap<String, String>,
        body: Option<String>,
    ) -> Result<NetFetchResult> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Params<'a> {
            extension_id: &'a str,
            url: &'a str,
            method: &'a str,
            headers: &'a HashMap<String, String>,
            body: Option<&'a str>,
        }

        self.call(
            "_goose/unstable/client-extensions/net/fetch",
            Params {
                extension_id: &self.plugin_id,
                url,
                method,
                headers: &headers,
                body: body.as_deref(),
            },
        )
        .await
    }

    pub async fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Params<'a> {
            extension_id: &'a str,
            key: &'a str,
        }

        #[derive(Deserialize)]
        struct Resp {
            value: Option<serde_json::Value>,
        }

        let resp: Resp = self
            .call(
                "_goose/unstable/client-extensions/storage/get",
                Params {
                    extension_id: &self.plugin_id,
                    key,
                },
            )
            .await?;
        Ok(resp.value)
    }

    pub async fn storage_set(&self, key: &str, value: serde_json::Value) -> Result<()> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Params<'a> {
            extension_id: &'a str,
            key: &'a str,
            value: serde_json::Value,
        }

        self.call::<_, serde_json::Value>(
            "_goose/unstable/client-extensions/storage/set",
            Params {
                extension_id: &self.plugin_id,
                key,
                value,
            },
        )
        .await?;
        Ok(())
    }

    async fn call<P: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R> {
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
            id: 1,
        };

        let mut builder = self.http.post(&self.base_url).json(&req);
        if let Some(token) = &self.token {
            builder = builder.header("x-secret-key", token);
        }
        let resp = builder.send().await?.json::<JsonRpcResponse<R>>().await?;

        if let Some(err) = resp.error {
            bail!("ACP error: {}", err.message);
        }

        resp.result.ok_or_else(|| anyhow::anyhow!("ACP: no result"))
    }
}
