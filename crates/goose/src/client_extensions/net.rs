use super::list_client_extensions;
use anyhow::{anyhow, bail, Result};
use futures::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use url::Url;

const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
const ALLOWED_METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];

#[derive(Debug)]
pub struct FetchResult {
    pub ok: bool,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub text: String,
}

fn allowed_origins(extension_id: &str) -> Result<Vec<String>> {
    let summary = list_client_extensions()?
        .into_iter()
        .find(|entry| entry.id == extension_id)
        .ok_or_else(|| anyhow!("client extension '{extension_id}' is not installed"))?;

    if !summary.enabled {
        bail!("client extension '{extension_id}' is disabled");
    }

    Ok(summary
        .manifest
        .get("network")
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

fn origin_of(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
}

pub async fn fetch(
    extension_id: &str,
    url: &str,
    method: Option<&str>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
) -> Result<FetchResult> {
    let method = method.unwrap_or("GET").to_ascii_uppercase();
    if !ALLOWED_METHODS.contains(&method.as_str()) {
        bail!("Unsupported method \"{method}\"");
    }

    let parsed = Url::parse(url).map_err(|_| anyhow!("Invalid URL \"{url}\""))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        bail!("Unsupported protocol \"{}:\"", parsed.scheme());
    }

    let origin = origin_of(&parsed);
    let allowed = allowed_origins(extension_id)?;
    if !allowed.iter().any(|entry| entry == &origin) {
        bail!(
            "Plugin \"{extension_id}\" has not allow-listed origin \"{origin}\" in its manifest's \"network\" field"
        );
    }

    let client = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build()?;
    let mut request = client.request(method.parse()?, url);
    if let Some(headers) = headers {
        for (key, value) in headers {
            request = request.header(key, value);
        }
    }
    if let Some(body) = body {
        request = request.body(body);
    }

    let response = request.send().await?;
    let status = response.status();
    let mut response_headers = HashMap::new();
    for (key, value) in response.headers() {
        if let Ok(value) = value.to_str() {
            response_headers.insert(key.to_string(), value.to_string());
        }
    }

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    while let Some(chunk) = stream.next().await {
        buffer.extend_from_slice(&chunk?);
        if buffer.len() > MAX_RESPONSE_BYTES {
            bail!("Response exceeded {MAX_RESPONSE_BYTES} bytes");
        }
    }

    Ok(FetchResult {
        ok: status.is_success(),
        status: status.as_u16(),
        headers: response_headers,
        text: String::from_utf8_lossy(&buffer).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_extensions::install_client_extension;
    use fs_err as fs;
    use serial_test::serial;
    use tempfile::TempDir;
    use wiremock::matchers::method as method_matcher;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct PathRootGuard(Option<std::ffi::OsString>);

    impl PathRootGuard {
        fn set(root: &std::path::Path) -> Self {
            let previous = std::env::var_os("GOOSE_PATH_ROOT");
            std::env::set_var("GOOSE_PATH_ROOT", root);
            Self(previous)
        }
    }

    impl Drop for PathRootGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(root) => std::env::set_var("GOOSE_PATH_ROOT", root),
                None => std::env::remove_var("GOOSE_PATH_ROOT"),
            }
        }
    }

    fn install_with_network(id: &str, network: &[&str]) {
        let source = TempDir::new().unwrap();
        fs::write(
            source.path().join("client-extension.json"),
            serde_json::json!({
                "id": id,
                "version": "0.1.0",
                "main": "index.html",
                "network": network,
            })
            .to_string(),
        )
        .unwrap();
        fs::write(source.path().join("index.html"), "<html></html>").unwrap();
        install_client_extension(source.path()).unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn rejects_a_url_whose_origin_is_not_allow_listed() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());
        install_with_network("demo", &[]);

        let error = fetch("demo", "http://127.0.0.1:1/api", None, None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("has not allow-listed origin \"http://127.0.0.1:1\""),
            "{error}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn rejects_an_invalid_url_before_checking_the_allowlist() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());
        install_with_network("demo", &["http://127.0.0.1:1"]);

        let error = fetch("demo", "not a url", None, None, None)
            .await
            .unwrap_err()
            .to_string();
        assert_eq!(error, "Invalid URL \"not a url\"");
    }

    #[tokio::test]
    #[serial]
    async fn fetches_an_allow_listed_origin_and_returns_the_body() {
        let mock_server = MockServer::start().await;
        Mock::given(method_matcher("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&mock_server)
            .await;

        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());
        let origin = mock_server.uri();
        install_with_network("demo", &[origin.as_str()]);

        let result = fetch(
            "demo",
            &format!("{origin}/api/repos"),
            None,
            Some(HashMap::from([("X-Loupe".to_string(), "tok".to_string())])),
            None,
        )
        .await
        .unwrap();

        assert!(result.ok);
        assert_eq!(result.status, 200);
        assert_eq!(result.text, "{}");
    }

    #[tokio::test]
    #[serial]
    async fn rejects_a_different_origin_under_the_same_allow_listed_host() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());
        install_with_network("demo", &["http://127.0.0.1:8455"]);

        let error = fetch("demo", "http://127.0.0.1:9999/api/repos", None, None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("\"http://127.0.0.1:9999\""), "{error}");
    }

    #[tokio::test]
    #[serial]
    async fn rejects_an_unsupported_method() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());
        install_with_network("demo", &["http://127.0.0.1:8455"]);

        let error = fetch(
            "demo",
            "http://127.0.0.1:8455/api/repos",
            Some("TRACE"),
            None,
            None,
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("Unsupported method \"TRACE\""), "{error}");
    }

    #[tokio::test]
    #[serial]
    async fn rejects_a_response_over_the_byte_limit() {
        let mock_server = MockServer::start().await;
        Mock::given(method_matcher("GET"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(vec![0u8; MAX_RESPONSE_BYTES + 1]),
            )
            .mount(&mock_server)
            .await;

        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());
        let origin = mock_server.uri();
        install_with_network("demo", &[origin.as_str()]);

        let error = fetch("demo", &format!("{origin}/big"), None, None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("exceeded"), "{error}");
    }

    #[tokio::test]
    #[serial]
    async fn rejects_a_disabled_extension() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());
        install_with_network("demo", &["http://127.0.0.1:8455"]);
        super::super::disable_client_extension("demo").unwrap();

        let error = fetch("demo", "http://127.0.0.1:8455/api", None, None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("disabled"), "{error}");
    }
}
