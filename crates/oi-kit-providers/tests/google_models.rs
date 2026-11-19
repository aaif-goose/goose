use oi_kit_providers::{base::Provider, google::GoogleProvider};
use serde_json::{json, Value};
use wiremock::{
    matchers::{header, method, path, query_param, query_param_is_missing},
    Mock, MockServer, ResponseTemplate,
};

fn provider(server: &MockServer) -> GoogleProvider {
    GoogleProvider::new(server.uri(), "test-key".into(), None, None, None).unwrap()
}

async fn mount_page(server: &MockServer, token: Option<&str>, response: Value) {
    let mock = Mock::given(method("GET"))
        .and(path("/v1beta/models"))
        .and(header("x-goog-api-key", "test-key"));
    let mock = match token {
        Some(token) => mock.and(query_param("pageToken", token)),
        None => mock.and(query_param_is_missing("pageToken")),
    };
    mock.respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn lists_models_across_all_pages() {
    let server = MockServer::start().await;
    let token = "next+/=%&";
    mount_page(
        &server,
        None,
        json!({"models": [{"name": "models/gemini-z"}], "nextPageToken": token}),
    )
    .await;
    mount_page(
        &server,
        Some(token),
        json!({"models": [{"name": "models/gemini-a"}], "nextPageToken": "last"}),
    )
    .await;
    mount_page(
        &server,
        Some("last"),
        json!({"models": [{"name": "models/gemini-b"}]}),
    )
    .await;

    let models = provider(&server).fetch_supported_models().await.unwrap();
    assert_eq!(models, ["gemini-a", "gemini-b", "gemini-z"]);
}

#[tokio::test]
async fn rejects_repeated_page_tokens() {
    let server = MockServer::start().await;
    for token in [None, Some("repeated")] {
        mount_page(
            &server,
            token,
            json!({"models": [], "nextPageToken": "repeated"}),
        )
        .await;
    }

    let error = provider(&server)
        .fetch_supported_models()
        .await
        .unwrap_err();
    assert!(error.to_string().contains("repeated page token"));
}
