//! Isolated in its own test binary: this test mutates process-wide proxy
//! environment variables, which would otherwise be observed by unrelated
//! tests in the same process that build HTTP clients.

use std::time::Duration;

use goose_providers::api_client::{ApiClient, AuthMethod};
use tokio::net::TcpListener;

#[tokio::test]
async fn loopback_transport_does_not_use_environment_proxy() {
    let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_uri = format!("http://{}", proxy.local_addr().unwrap());
    let _guard = env_lock::lock_env([
        ("HTTP_PROXY", Some(proxy_uri.as_str())),
        ("http_proxy", Some(proxy_uri.as_str())),
        ("NO_PROXY", Some("")),
        ("no_proxy", Some("")),
    ]);
    let client = ApiClient::new_with_tls(
        "http://127.0.0.1:9".to_string(),
        AuthMethod::BearerToken("secret".to_string()),
        None,
    )
    .unwrap()
    .with_loopback_http_only()
    .unwrap()
    .with_header("x-test", "value")
    .unwrap();

    assert!(client.response_get("models").await.is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), proxy.accept())
            .await
            .is_err()
    );
}
