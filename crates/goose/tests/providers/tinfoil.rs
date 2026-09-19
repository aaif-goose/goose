use super::*;
use std::collections::VecDeque;
use std::sync::Mutex as StdMutex;

#[tokio::test]
#[ignore = "requires network access to Tinfoil and its attestation services"]
async fn live_tinfoil_attestation_and_pinned_connection() {
    let mut enclave = SecureClient::new(TINFOIL_HOST, TINFOIL_REPO, "");
    tokio::time::timeout(Duration::from_secs(60), enclave.verify())
        .await
        .unwrap()
        .unwrap();
    assert!(enclave.is_verified());
    let response = enclave
        .http_client()
        .unwrap()
        .get(format!("https://{TINFOIL_HOST}/health"))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
}

struct MockEnclave {
    verified: bool,
    verification_fails: bool,
    verifications: usize,
    requests: StdMutex<Vec<Request>>,
    responses: StdMutex<VecDeque<Result<Response>>>,
}

#[async_trait]
impl AttestedEnclave for MockEnclave {
    fn is_verified(&self) -> bool {
        self.verified
    }

    async fn verify(&mut self) -> Result<()> {
        self.verifications += 1;
        self.verified = !self.verification_fails;
        if self.verification_fails {
            bail!("invalid attestation");
        }
        Ok(())
    }

    async fn execute(&self, request: Request) -> Result<Response> {
        assert!(self.verified);
        self.requests.lock().unwrap().push(request);
        self.responses.lock().unwrap().pop_front().unwrap()
    }
}

fn transport(responses: Vec<Result<Response>>) -> TinfoilTransport<MockEnclave> {
    TinfoilTransport {
        enclave: Mutex::new(MockEnclave {
            verified: true,
            verification_fails: false,
            verifications: 0,
            requests: StdMutex::new(Vec::new()),
            responses: StdMutex::new(responses.into()),
        }),
        cache_secret: "test-cache-secret".to_string(),
    }
}

fn request() -> Request {
    reqwest::Client::new()
        .post(format!("{TINFOIL_API_URL}/chat/completions"))
        .bearer_auth("test-key")
        .json(&serde_json::json!({"model": "gpt-oss-120b", "stream": true, "messages": []}))
        .build()
        .unwrap()
}

async fn connection_error() -> anyhow::Error {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let error = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{address}"))
        .send()
        .await
        .unwrap_err();
    assert!(error.is_connect());
    error.into()
}

#[tokio::test]
async fn failed_verification_never_sends_inference() {
    let transport = transport(vec![]);
    {
        let mut enclave = transport.enclave.lock().await;
        enclave.verified = false;
        enclave.verification_fails = true;
    }
    assert!(transport
        .execute(request())
        .await
        .unwrap_err()
        .to_string()
        .contains("verification failed"));
    assert!(transport
        .enclave
        .lock()
        .await
        .requests
        .lock()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn connection_failure_reverifies_before_retrying() {
    let transport = transport(vec![
        Err(connection_error().await),
        Err(anyhow::anyhow!("response sentinel")),
    ]);
    assert_eq!(
        transport.execute(request()).await.unwrap_err().to_string(),
        "response sentinel"
    );
    let enclave = transport.enclave.lock().await;
    assert_eq!(enclave.verifications, 1);
    let requests = enclave.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        assert_eq!(request.headers()["authorization"], "Bearer test-key");
        let body: Value =
            serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(body["user_cache_secret"], "test-cache-secret");
        assert_eq!(body["stream"], true);
    }
}

#[tokio::test]
async fn failed_reverification_does_not_retry_or_reuse_stale_trust() {
    let transport = transport(vec![Err(connection_error().await)]);
    transport.enclave.lock().await.verification_fails = true;
    assert!(transport.execute(request()).await.is_err());
    assert!(transport.execute(request()).await.is_err());
    let enclave = transport.enclave.lock().await;
    assert_eq!(enclave.requests.lock().unwrap().len(), 1);
    assert_eq!(enclave.verifications, 2);
    assert!(!enclave.verified);
}

#[tokio::test]
async fn application_errors_do_not_trigger_reverification_or_replay() {
    let transport = transport(vec![Err(anyhow::anyhow!("application error"))]);
    assert!(transport.execute(request()).await.is_err());
    let enclave = transport.enclave.lock().await;
    assert_eq!(enclave.verifications, 0);
    assert_eq!(enclave.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn rejects_other_origins_and_host_overrides_before_sending() {
    let transport = transport(vec![]);
    for url in [
        "http://inference.tinfoil.sh/v1/models",
        "https://example.com/v1/models",
        "https://inference.tinfoil.sh:444/v1/models",
    ] {
        let request = reqwest::Client::new().get(url).build().unwrap();
        assert!(transport.execute(request).await.is_err());
    }
    let mut request = request();
    request
        .headers_mut()
        .insert(reqwest::header::HOST, "example.com".parse().unwrap());
    assert!(transport.execute(request).await.is_err());
    assert!(transport
        .enclave
        .lock()
        .await
        .requests
        .lock()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn tinfoil_is_registered_with_api_key_and_model_discovery() {
    let entry = crate::providers::get_from_registry("tinfoil")
        .await
        .unwrap();
    assert_eq!(entry.metadata().default_model, "gpt-oss-120b");
    assert_eq!(entry.metadata().config_keys.len(), 1);
    assert_eq!(entry.metadata().config_keys[0].name, "TINFOIL_API_KEY");
    assert!(entry.metadata().config_keys[0].secret);
    assert!(entry.supports_inventory_refresh());
}

#[tokio::test]
async fn verified_transport_preserves_model_discovery_streaming_tools_and_usage() {
    use futures::StreamExt;
    use goose_providers::base::Provider;
    use goose_providers::model::ModelConfig;
    use serde_json::json;

    let models = http::Response::builder()
        .header("content-type", "application/json")
        .body(json!({"data": [{"id": "gpt-oss-120b"}]}).to_string())
        .unwrap();
    let chunks = [
        json!({"model": "gpt-oss-120b", "choices": [{"index": 0, "delta": {
            "role": "assistant", "tool_calls": [{"index": 0, "id": "call_1", "type": "function",
                "function": {"name": "developer__shell", "arguments": "{\"command\":"}}]
        }}]}),
        json!({"model": "gpt-oss-120b", "choices": [{"index": 0, "delta": {
            "tool_calls": [{"index": 0, "function": {"arguments": "\"echo 391\"}"}}]
        }, "finish_reason": "tool_calls"}]}),
        json!({"model": "gpt-oss-120b", "choices": [], "usage": {
            "prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20
        }}),
    ];
    let body = chunks
        .iter()
        .map(|chunk| format!("data: {chunk}\n\n"))
        .collect::<String>()
        + "data: [DONE]\n\n";
    let completion = http::Response::builder()
        .header("content-type", "text/event-stream")
        .body(body)
        .unwrap();
    let transport = Arc::new(transport(vec![Ok(models.into()), Ok(completion.into())]));
    let api = ApiClient::new_with_tls(
        TINFOIL_API_URL.to_string(),
        AuthMethod::BearerToken("test-key".to_string()),
        None,
    )
    .unwrap()
    .with_request_executor(transport.clone());
    let provider = OpenAiCompatibleProvider::new("tinfoil".to_string(), api, String::new());
    assert_eq!(
        provider.fetch_supported_models().await.unwrap(),
        vec!["gpt-oss-120b"]
    );
    let mut stream = provider
        .stream(&ModelConfig::new("gpt-oss-120b"), "Be helpful", &[], &[])
        .await
        .unwrap();
    let mut calls = Vec::new();
    let mut usage = None;
    while let Some(event) = stream.next().await {
        let (message, event_usage) = event.unwrap();
        if let Some(message) = message {
            for content in message.content {
                if let Some(call) = content.as_tool_request() {
                    calls.push(call.clone());
                }
            }
        }
        if event_usage.is_some() {
            usage = event_usage;
        }
    }
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "call_1");
    let call = calls[0].tool_call.as_ref().unwrap();
    assert_eq!(call.name, "developer__shell");
    assert_eq!(call.arguments.as_ref().unwrap()["command"], "echo 391");
    assert_eq!(usage.unwrap().usage.total_tokens, Some(20));
    let enclave = transport.enclave.lock().await;
    let requests = enclave.requests.lock().unwrap();
    assert_eq!(requests[0].url().path(), "/v1/models");
    assert_eq!(requests[1].url().path(), "/v1/chat/completions");
}
