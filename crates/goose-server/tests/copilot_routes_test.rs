use axum::body::Body;
use axum::http::{Request, StatusCode};
use goose::config::Config;
use goose::copilot::{cached_installation_id, CopilotPrefs, INSTALLATION_ID_CONFIG_KEY};
use goose_server::routes::copilot::routes;
use goose_server::state::AppState;
use tower::ServiceExt;

fn secret_header() -> (&'static str, &'static str) {
    ("x-secret-key", "test-secret")
}

#[tokio::test(flavor = "multi_thread")]
async fn copilot_prefs_get_put_roundtrip() {
    let state = AppState::new(true).await.unwrap();
    let app = routes(state);

    let get = Request::builder()
        .uri("/copilot/prefs")
        .method("GET")
        .header(secret_header().0, secret_header().1)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(get).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let put = Request::builder()
        .uri("/copilot/prefs")
        .method("PUT")
        .header(secret_header().0, secret_header().1)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "prefs": {
                    "schema_version": 1,
                    "auto_review_on_pr_open": false,
                    "trigger_preference": "manual-only",
                    "trigger_permission": "anyone",
                    "allow_act_on_issues": false,
                    "specific_users_allowlist": [],
                    "allow_commit_on_fix": false,
                    "allow_open_new_prs": false,
                    "review_severity": "medium",
                    "custom_instructions": "route-test",
                    "review_output_style": "both",
                    "review_model_choice": "default"
                }
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = app.clone().oneshot(put).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let get = Request::builder()
        .uri("/copilot/prefs")
        .method("GET")
        .header(secret_header().0, secret_header().1)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(get).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prefs: CopilotPrefs = serde_json::from_slice(&body).unwrap();
    assert_eq!(prefs.custom_instructions, "route-test");
}

#[tokio::test(flavor = "multi_thread")]
async fn copilot_status_and_disconnect_clear_install_id() {
    let state = AppState::new(true).await.unwrap();
    let app = routes(state);
    let config = Config::global();

    config
        .set_param(INSTALLATION_ID_CONFIG_KEY, serde_json::json!(42_u64))
        .unwrap();

    let status = Request::builder()
        .uri("/copilot/status")
        .method("GET")
        .header(secret_header().0, secret_header().1)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(status).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["installation_id"], 42);

    assert_eq!(cached_installation_id(config), Some(42));

    let disconnect = Request::builder()
        .uri("/copilot/setup")
        .method("DELETE")
        .header(secret_header().0, secret_header().1)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(disconnect).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    assert_eq!(cached_installation_id(config), None);
}
