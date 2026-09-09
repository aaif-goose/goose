use goose_providers::base::Provider;
use goose_providers::conversation::message::Message;
use goose_providers::databricks_auth::DatabricksAuth;
use goose_providers::databricks_v2::DatabricksV2Provider;
use goose_providers::model::ModelConfig;
use goose_providers::retry::RetryConfig;
use goose_providers::thinking::ThinkingEffort;
use serde_json::{json, Value};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn model_service_effort_reaches_the_http_request() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let efforts = [
        ThinkingEffort::Low,
        ThinkingEffort::Medium,
        ThinkingEffort::High,
        ThinkingEffort::Max,
    ];
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("content-type", "text/event-stream")
                .set_body_string("data: [DONE]\n\n"),
        )
        .expect(efforts.len() as u64)
        .mount(&server)
        .await;
    let provider = DatabricksV2Provider::new(
        server.uri(),
        DatabricksAuth::token("test-token".to_string()),
        RetryConfig::new(0, 0, 1.0, 0),
        None,
        None,
        None,
        None,
        None,
    )?;
    let model = "catalog.schema.goose-claude-fable-5-1";
    for effort in efforts {
        let config = ModelConfig::new(model).with_thinking_effort(effort);
        drop(
            provider
                .stream(
                    &config,
                    "Be helpful",
                    &[Message::user().with_text("Reply OK")],
                    &[],
                )
                .await?,
        );
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), efforts.len());
    for (request, effort) in requests.iter().zip(efforts) {
        let payload: Value = request.body_json()?;
        assert_eq!(request.url.path(), "/ai-gateway/mlflow/v1/chat/completions");
        assert_eq!(payload["model"], model);
        assert_eq!(payload["thinking"], json!({"type": "adaptive"}));
        assert_eq!(payload["output_config"]["effort"], effort.to_string());
    }
    Ok(())
}
