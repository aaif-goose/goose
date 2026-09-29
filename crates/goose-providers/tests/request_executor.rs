use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use goose_providers::api_client::{ApiClient, AuthMethod, RequestExecutor};
use reqwest::{Request, Response};
use serde_json::json;
use tokio::net::TcpListener;

#[derive(Default)]
struct RejectingExecutor(Mutex<Vec<Request>>);

#[async_trait]
impl RequestExecutor for RejectingExecutor {
    async fn execute(&self, request: Request) -> Result<Response> {
        self.0.lock().unwrap().push(request);
        anyhow::bail!("verification rejected")
    }
}

#[tokio::test]
async fn executor_cannot_be_bypassed_by_rebuilds_or_request_methods() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let executor = Arc::new(RejectingExecutor::default());
    let client = ApiClient::new_with_tls(
        format!("http://{}/v1", listener.local_addr().unwrap()),
        AuthMethod::BearerToken("test-key".to_string()),
        None,
    )
    .unwrap()
    .with_request_executor(executor.clone())
    .with_header("x-default", "default")
    .unwrap()
    .with_loopback_http_only()
    .unwrap()
    .with_query(vec![("api-version".to_string(), "test".to_string())]);

    assert!(client.response_get("models").await.is_err());
    assert!(client
        .response_post("chat/completions", &json!({"stream": false}))
        .await
        .is_err());
    assert!(client
        .request("chat/completions")
        .streaming(true)
        .header("x-default", "override")
        .unwrap()
        .response_post(&json!({"stream": true}))
        .await
        .is_err());
    assert!(client
        .request("audio/transcriptions")
        .multipart_post(reqwest::multipart::Form::new().text("model", "test"))
        .await
        .is_err());

    {
        let requests = executor.0.lock().unwrap();
        assert_eq!(requests.len(), 4);
        for request in requests.iter() {
            assert_eq!(request.headers()["authorization"], "Bearer test-key");
            assert_eq!(request.url().query(), Some("api-version=test"));
        }
        assert_eq!(requests[0].headers()["x-default"], "default");
        assert_eq!(requests[2].headers()["x-default"], "override");
        assert_eq!(requests[0].url().path(), "/v1/models");
        assert_eq!(requests[1].url().path(), "/v1/chat/completions");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                requests[2].body().unwrap().as_bytes().unwrap()
            )
            .unwrap(),
            json!({"stream": true})
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

struct HangingExecutor;

#[async_trait]
impl RequestExecutor for HangingExecutor {
    async fn execute(&self, _request: Request) -> Result<Response> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn verification_and_connection_share_the_request_deadline() {
    let client = ApiClient::with_timeout_and_tls(
        "https://example.invalid".to_string(),
        AuthMethod::NoAuth,
        Duration::from_millis(10),
        None,
    )
    .unwrap()
    .with_request_executor(Arc::new(HangingExecutor));
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        client
            .request("chat/completions")
            .streaming(true)
            .response_post(&json!({})),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.to_string().contains("timed out"));
}
