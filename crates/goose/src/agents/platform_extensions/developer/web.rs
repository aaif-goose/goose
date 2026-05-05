use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use rmcp::model::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WebFetchParams {
    /// Absolute http:// or https:// URL to fetch.
    pub url: String,
    /// How to handle the response body. Defaults to `text`.
    #[serde(default)]
    pub save_as: SaveAsFormat,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SaveAsFormat {
    /// Decode body as UTF-8 text.
    #[default]
    Text,
    /// Decode body as UTF-8 text and validate it parses as JSON.
    Json,
    /// Treat body as raw bytes; always saved to a temp file.
    Binary,
}

/// Bodies up to this size are returned inline in the tool result.
/// Larger bodies (and all binary responses) are written to a temp file
/// and the path is returned instead.
const INLINE_BYTE_LIMIT: usize = 64 * 1024;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct WebFetchTool {
    http_client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        let user_agent = format!("goose/{}", env!("CARGO_PKG_VERSION"));
        let http_client = reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { http_client }
    }

    pub async fn fetch(&self, params: WebFetchParams) -> CallToolResult {
        let url = params.url.trim();
        if url.is_empty() {
            return error_result("URL cannot be empty.");
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return error_result(&format!(
                "URL must start with http:// or https://, got: {url}"
            ));
        }

        let response = match self
            .http_client
            .get(url)
            .header("Accept", "text/markdown, text/html, application/json, */*")
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return error_result(&format!("Failed to fetch URL: {e}")),
        };

        let status = response.status();
        if !status.is_success() {
            return error_result(&format!("HTTP request failed with status: {status}"));
        }

        match params.save_as {
            SaveAsFormat::Text => match response.text().await {
                Ok(text) => deliver_text(text, "txt"),
                Err(e) => error_result(&format!("Failed to read body as text: {e}")),
            },
            SaveAsFormat::Json => match response.text().await {
                Ok(text) => {
                    if let Err(e) = serde_json::from_str::<Value>(&text) {
                        return error_result(&format!("Invalid JSON response: {e}"));
                    }
                    deliver_text(text, "json")
                }
                Err(e) => error_result(&format!("Failed to read body as text: {e}")),
            },
            SaveAsFormat::Binary => match response.bytes().await {
                Ok(bytes) => deliver_bytes(&bytes, "bin"),
                Err(e) => error_result(&format!("Failed to read body bytes: {e}")),
            },
        }
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

fn deliver_text(text: String, extension: &str) -> CallToolResult {
    if text.len() <= INLINE_BYTE_LIMIT {
        return CallToolResult::success(vec![Content::text(text).with_priority(0.0)]);
    }
    match write_to_temp(text.as_bytes(), extension) {
        Ok(path) => CallToolResult::success(vec![Content::text(format!(
            "Content saved to: {} ({} bytes)",
            path.display(),
            text.len()
        ))
        .with_priority(0.0)]),
        Err(e) => error_result(&format!("Failed to write temp file: {e}")),
    }
}

fn deliver_bytes(bytes: &[u8], extension: &str) -> CallToolResult {
    match write_to_temp(bytes, extension) {
        Ok(path) => CallToolResult::success(vec![Content::text(format!(
            "Content saved to: {} ({} bytes)",
            path.display(),
            bytes.len()
        ))
        .with_priority(0.0)]),
        Err(e) => error_result(&format!("Failed to write temp file: {e}")),
    }
}

fn write_to_temp(bytes: &[u8], extension: &str) -> std::io::Result<PathBuf> {
    let mut file = tempfile::Builder::new()
        .prefix("goose-web-")
        .suffix(&format!(".{extension}"))
        .tempfile()?;
    file.write_all(bytes)?;
    file.flush()?;
    let (_, path) = file.keep().map_err(|e| e.error)?;
    Ok(path)
}

fn error_result(message: &str) -> CallToolResult {
    CallToolResult::error(vec![Content::text(message.to_string()).with_priority(0.0)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::RawContent;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn first_text(result: &CallToolResult) -> String {
        match &result.content[0].raw {
            RawContent::Text(t) => t.text.clone(),
            _ => panic!("expected text content"),
        }
    }

    fn extract_temp_path(message: &str) -> &str {
        message
            .strip_prefix("Content saved to: ")
            .and_then(|s| s.rsplit_once(" ("))
            .map(|(p, _)| p)
            .expect("message should contain a saved path")
    }

    #[tokio::test]
    async fn fetch_text_returns_inline_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/page"))
            .respond_with(ResponseTemplate::new(200).set_body_string("hello world"))
            .mount(&server)
            .await;

        let tool = WebFetchTool::new();
        let result = tool
            .fetch(WebFetchParams {
                url: format!("{}/page", server.uri()),
                save_as: SaveAsFormat::Text,
            })
            .await;

        assert_eq!(result.is_error, Some(false));
        assert_eq!(first_text(&result), "hello world");
    }

    #[tokio::test]
    async fn fetch_non_2xx_returns_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let tool = WebFetchTool::new();
        let result = tool
            .fetch(WebFetchParams {
                url: format!("{}/missing", server.uri()),
                save_as: SaveAsFormat::Text,
            })
            .await;

        assert_eq!(result.is_error, Some(true));
        assert!(first_text(&result).contains("404"));
    }

    #[tokio::test]
    async fn fetch_json_validates_payload() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/json-good"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true}"#))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/json-bad"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let tool = WebFetchTool::new();

        let ok = tool
            .fetch(WebFetchParams {
                url: format!("{}/json-good", server.uri()),
                save_as: SaveAsFormat::Json,
            })
            .await;
        assert_eq!(ok.is_error, Some(false));
        assert_eq!(first_text(&ok), r#"{"ok":true}"#);

        let bad = tool
            .fetch(WebFetchParams {
                url: format!("{}/json-bad", server.uri()),
                save_as: SaveAsFormat::Json,
            })
            .await;
        assert_eq!(bad.is_error, Some(true));
        assert!(first_text(&bad).to_lowercase().contains("invalid json"));
    }

    #[tokio::test]
    async fn fetch_binary_writes_to_temp_file() {
        let server = MockServer::start().await;
        let payload: Vec<u8> = (0u8..32).collect();
        Mock::given(method("GET"))
            .and(path("/bin"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(payload.clone())
                    .insert_header("content-type", "application/octet-stream"),
            )
            .mount(&server)
            .await;

        let tool = WebFetchTool::new();
        let result = tool
            .fetch(WebFetchParams {
                url: format!("{}/bin", server.uri()),
                save_as: SaveAsFormat::Binary,
            })
            .await;

        assert_eq!(result.is_error, Some(false));
        let msg = first_text(&result);
        let path_str = extract_temp_path(&msg);
        let bytes = std::fs::read(path_str).expect("temp file should be readable");
        assert_eq!(bytes, payload);
        std::fs::remove_file(path_str).ok();
    }

    #[tokio::test]
    async fn fetch_text_over_limit_spills_to_temp_file() {
        let server = MockServer::start().await;
        let big = "x".repeat(INLINE_BYTE_LIMIT + 1);
        Mock::given(method("GET"))
            .and(path("/big"))
            .respond_with(ResponseTemplate::new(200).set_body_string(big.clone()))
            .mount(&server)
            .await;

        let tool = WebFetchTool::new();
        let result = tool
            .fetch(WebFetchParams {
                url: format!("{}/big", server.uri()),
                save_as: SaveAsFormat::Text,
            })
            .await;

        assert_eq!(result.is_error, Some(false));
        let msg = first_text(&result);
        let path_str = extract_temp_path(&msg);
        let bytes = std::fs::read(path_str).expect("temp file should be readable");
        assert_eq!(bytes.len(), big.len());
        std::fs::remove_file(path_str).ok();
    }

    #[tokio::test]
    async fn fetch_rejects_empty_url() {
        let tool = WebFetchTool::new();
        let result = tool
            .fetch(WebFetchParams {
                url: String::new(),
                save_as: SaveAsFormat::Text,
            })
            .await;
        assert_eq!(result.is_error, Some(true));
        assert!(first_text(&result).to_lowercase().contains("empty"));
    }

    #[tokio::test]
    async fn fetch_rejects_non_http_scheme() {
        let tool = WebFetchTool::new();
        let result = tool
            .fetch(WebFetchParams {
                url: "file:///etc/passwd".to_string(),
                save_as: SaveAsFormat::Text,
            })
            .await;
        assert_eq!(result.is_error, Some(true));
        assert!(first_text(&result).contains("http"));
    }
}
