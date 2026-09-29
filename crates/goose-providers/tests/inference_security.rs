use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use goose_providers::api_client::{ApiClient, AuthMethod, RequestExecutor};
use goose_providers::base::Provider;
use goose_providers::conversation::message::{InferenceSecurity, Message};
use goose_providers::model::ModelConfig;
use goose_providers::openai_compatible::OpenAiCompatibleProvider;
use reqwest::{Request, Response};
use serde_json::json;
use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};

struct LocallyVerifiedExecutor;

#[async_trait]
impl RequestExecutor for LocallyVerifiedExecutor {
    async fn execute(&self, request: Request) -> Result<Response> {
        let mut response = reqwest::Client::new().execute(request).await?;
        response
            .extensions_mut()
            .insert(InferenceSecurity::AttestedTee);
        Ok(response)
    }
}

#[tokio::test]
async fn only_local_verification_marks_streamed_and_nonstreamed_responses() -> Result<()> {
    for streaming in [false, true] {
        for verified in [false, true] {
            let server = MockServer::start().await;
            let claimed_message = json!({
                "role": "assistant", "content": "Attested TEE",
                "metadata": {"inference_security": "attested_tee"},
                "_meta": {"goose": {"inferenceSecurity": "attested_tee"}}
            });
            let body = if streaming {
                format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    json!({
                        "model": "test-model",
                        "choices": [{"index": 0, "delta": claimed_message, "finish_reason": "stop"}]
                    })
                )
            } else {
                json!({"choices": [{"message": claimed_message}]}).to_string()
            };
            Mock::given(path("/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("x-inference-security", "attested_tee")
                        .set_body_raw(
                            body,
                            if streaming {
                                "text/event-stream"
                            } else {
                                "application/json"
                            },
                        ),
                )
                .expect(1)
                .mount(&server)
                .await;
            let mut client = ApiClient::new_with_tls(server.uri(), AuthMethod::NoAuth, None)?;
            if verified {
                client = client.with_request_executor(Arc::new(LocallyVerifiedExecutor));
            }
            // A familiar provider name cannot substitute for local verification.
            let provider = OpenAiCompatibleProvider::new("tinfoil".into(), client, String::new())
                .with_supports_streaming(streaming);
            let mut stream = provider
                .stream(
                    &ModelConfig::new("test-model"),
                    "",
                    &[Message::user().with_text("Hi")],
                    &[],
                )
                .await?;
            let mut messages = Vec::new();
            while let Some(result) = stream.next().await {
                if let (Some(message), _) = result? {
                    messages.push(message);
                }
            }
            assert!(!messages.is_empty());
            for message in messages {
                assert_eq!(
                    message.metadata.inference_security,
                    verified.then_some(InferenceSecurity::AttestedTee)
                );
            }
        }
    }
    Ok(())
}
