use goose::conversation::message::Message;
use goose::providers::api_client::{ApiClient, AuthMethod};
use goose::providers::base::Provider;
use goose::providers::openai::OpenAiProviderBuilder;
use goose::session_context::{session_id_request_builder, SESSION_ID_HEADER};
use goose_providers::model::ModelConfig;
use serde_json::json;
use std::sync::Arc;
use std::sync::Mutex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

#[derive(Clone, Default)]
struct HeaderCapture {
    captured_headers: Arc<Mutex<Vec<Option<String>>>>,
}

impl HeaderCapture {
    fn new() -> Self {
        Self {
            captured_headers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn capture_session_header(&self, req: &Request) {
        let session_id = req
            .headers
            .get(SESSION_ID_HEADER)
            .map(|v| v.to_str().unwrap().to_string());
        self.captured_headers.lock().unwrap().push(session_id);
    }

    fn get_captured(&self) -> Vec<Option<String>> {
        self.captured_headers.lock().unwrap().clone()
    }
}

fn create_test_provider(mock_server_url: &str) -> Box<dyn Provider> {
    let api_client = ApiClient::new_with_tls(
        mock_server_url.to_string(),
        AuthMethod::BearerToken("test-key".to_string()),
        None,
    )
    .unwrap()
    .with_request_builder(session_id_request_builder());
    Box::new(
        OpenAiProviderBuilder::new(api_client)
            .session_id_provider(Arc::new(goose::session_context::current_session_id))
            .build(),
    )
}

async fn setup_mock_server() -> (MockServer, HeaderCapture, Box<dyn Provider>) {
    let mock_server = MockServer::start().await;
    let capture = HeaderCapture::new();
    let chat_capture = capture.clone();
    let responses_capture = capture.clone();

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |req: &Request| {
            chat_capture.capture_session_header(req);
            // Return SSE streaming format
            let sse_response = format!(
                "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
                json!({
                    "choices": [{
                        "delta": {
                            "content": "Hi there! How can I help you today?",
                            "role": "assistant"
                        },
                        "index": 0
                    }],
                    "created": 1755133833,
                    "id": "chatcmpl-test",
                    "model": "gpt-5-nano"
                }),
                json!({
                    "choices": [],
                    "usage": {
                        "completion_tokens": 10,
                        "prompt_tokens": 8,
                        "total_tokens": 18
                    }
                })
            );
            ResponseTemplate::new(200)
                .set_body_string(sse_response)
                .insert_header("content-type", "text/event-stream")
        })
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |req: &Request| {
            responses_capture.capture_session_header(req);
            let sse_response = format!(
                "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
                json!({
                    "type": "response.created",
                    "sequence_number": 1,
                    "response": {
                        "id": "resp_test",
                        "object": "response",
                        "created_at": 1755133833,
                        "status": "in_progress",
                        "model": "gpt-5-nano",
                        "output": []
                    }
                }),
                json!({
                    "type": "response.output_text.delta",
                    "sequence_number": 2,
                    "item_id": "msg_test",
                    "output_index": 0,
                    "content_index": 0,
                    "delta": "Hi there! How can I help you today?"
                }),
                json!({
                    "type": "response.completed",
                    "sequence_number": 3,
                    "response": {
                        "id": "resp_test",
                        "object": "response",
                        "created_at": 1755133833,
                        "status": "completed",
                        "model": "gpt-5-nano",
                        "output": [],
                        "usage": {
                            "input_tokens": 8,
                            "output_tokens": 10,
                            "total_tokens": 18
                        }
                    }
                })
            );
            ResponseTemplate::new(200)
                .set_body_string(sse_response)
                .insert_header("content-type", "text/event-stream")
        })
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());
    (mock_server, capture, provider)
}

async fn make_request(provider: &dyn Provider, session_id: &str) {
    make_request_with_model(provider, session_id, ModelConfig::new("gpt-5-nano")).await;
}

async fn make_request_with_model(
    provider: &dyn Provider,
    session_id: &str,
    model_config: ModelConfig,
) {
    let message = Message::user().with_text("test message");
    let _ = goose::session_context::with_session_id(
        Some(session_id.to_string()),
        provider.complete(
            &model_config,
            "You are a helpful assistant.",
            &[message],
            &[],
        ),
    )
    .await
    .unwrap();
}

#[tokio::test]
#[cfg(feature = "otel")]
async fn test_session_id_propagates_to_log_records() {
    use opentelemetry::logs::AnyValue;
    use opentelemetry::Key;
    use opentelemetry_appender_tracing::layer::{
        OpenTelemetryTracingBridge, TracingSpanAttributes,
    };
    use opentelemetry_sdk::logs::{InMemoryLogExporterBuilder, SdkLoggerProvider};
    use tracing_subscriber::prelude::*;

    let exporter = InMemoryLogExporterBuilder::default().build();
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();

    let layer = OpenTelemetryTracingBridge::builder(&provider)
        .with_tracing_span_attributes(TracingSpanAttributes::allowlist(["session.id"]))
        .build();
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    let span = tracing::info_span!("test", session.id = "test-session-42");
    let _enter = span.enter();
    tracing::info!("hello from test");
    drop(_enter);
    drop(_guard);

    provider.force_flush().unwrap();
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    let log = &logs[0];

    let has_session_id = log.record.attributes_iter().any(|(k, v)| {
        k == &Key::new("session.id")
            && matches!(v, AnyValue::String(s) if s.as_str() == "test-session-42")
    });
    assert!(has_session_id);
}

#[tokio::test]
async fn test_session_id_propagation_to_llm() {
    let (_server, capture, provider) = setup_mock_server().await;

    make_request(provider.as_ref(), "integration-test-session-123").await;

    assert_eq!(
        capture.get_captured(),
        vec![Some("integration-test-session-123".to_string())]
    );
}

#[tokio::test]
async fn test_session_id_is_sent_as_responses_prompt_cache_key() {
    let (server, _, provider) = setup_mock_server().await;

    make_request(provider.as_ref(), "integration-test-session-123").await;

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = requests[0].body_json().unwrap();
    assert_eq!(
        body["prompt_cache_key"],
        json!("integration-test-session-123")
    );
}

#[tokio::test]
async fn test_explicit_prompt_cache_key_overrides_session_id() {
    let (server, _, provider) = setup_mock_server().await;
    let model_config =
        ModelConfig::new("gpt-5-nano").with_merged_request_params(std::collections::HashMap::from(
            [("prompt_cache_key".to_string(), json!("shared-workflow"))],
        ));

    make_request_with_model(provider.as_ref(), "session-123", model_config).await;

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = requests[0].body_json().unwrap();
    assert_eq!(body["prompt_cache_key"], json!("shared-workflow"));
}

#[tokio::test]
async fn test_session_id_always_present() {
    let (_server, capture, provider) = setup_mock_server().await;

    make_request(provider.as_ref(), "test-session-id").await;

    assert_eq!(
        capture.get_captured(),
        vec![Some("test-session-id".to_string())]
    );
}

#[tokio::test]
async fn test_session_id_matches_across_calls() {
    let (_server, capture, provider) = setup_mock_server().await;

    let session_id = "consistent-session-456";
    make_request(provider.as_ref(), session_id).await;
    make_request(provider.as_ref(), session_id).await;
    make_request(provider.as_ref(), session_id).await;

    assert_eq!(
        capture.get_captured(),
        vec![Some(session_id.to_string()); 3]
    );
}

#[tokio::test]
async fn test_different_sessions_have_different_ids() {
    let (_server, capture, provider) = setup_mock_server().await;

    let session_id_1 = "session-one";
    let session_id_2 = "session-two";
    make_request(provider.as_ref(), session_id_1).await;
    make_request(provider.as_ref(), session_id_2).await;

    assert_eq!(
        capture.get_captured(),
        vec![
            Some(session_id_1.to_string()),
            Some(session_id_2.to_string())
        ]
    );
}
