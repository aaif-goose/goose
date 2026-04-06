use goose_server::tls::{self_signed_config, TlsConfig};

#[cfg(not(feature = "native-tls"))]
#[test]
fn default_tls_config_is_rustls() {
    fn assert_type<T>(_: &T) {}
    let rt = tokio::runtime::Runtime::new().unwrap();
    let setup = rt.block_on(self_signed_config()).unwrap();
    // Proves TlsConfig resolves to RustlsConfig when native-tls is disabled.
    let _: &axum_server::tls_rustls::RustlsConfig = &setup.config;
    assert_type::<TlsConfig>(&setup.config);
}

#[cfg(feature = "native-tls")]
#[test]
fn native_tls_config_is_openssl() {
    fn assert_type<T>(_: &T) {}
    let rt = tokio::runtime::Runtime::new().unwrap();
    let setup = rt.block_on(self_signed_config()).unwrap();
    // Proves TlsConfig resolves to OpenSSLConfig when native-tls is enabled.
    let _: &axum_server::tls_openssl::OpenSSLConfig = &setup.config;
    assert_type::<TlsConfig>(&setup.config);
}

#[tokio::test]
async fn self_signed_config_produces_valid_fingerprint() {
    let setup = self_signed_config().await.unwrap();

    assert!(
        !setup.fingerprint.is_empty(),
        "fingerprint must not be empty"
    );

    let parts: Vec<&str> = setup.fingerprint.split(':').collect();
    assert_eq!(
        parts.len(),
        32,
        "SHA-256 fingerprint must have 32 hex pairs"
    );

    for part in &parts {
        assert_eq!(
            part.len(),
            2,
            "each fingerprint segment must be 2 hex chars"
        );
        assert!(
            part.chars().all(|c| c.is_ascii_hexdigit()),
            "fingerprint segment '{}' must be valid hex",
            part
        );
    }
}

#[tokio::test]
async fn self_signed_config_returns_usable_tls_config() {
    use axum::routing::get;
    use std::net::SocketAddr;

    let setup = self_signed_config().await.unwrap();

    let app = axum::Router::new().route("/health", get(|| async { "ok" }));
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));

    #[cfg(not(feature = "native-tls"))]
    let server = axum_server::bind_rustls(addr, setup.config);

    #[cfg(feature = "native-tls")]
    let server = axum_server::bind_openssl(addr, setup.config);

    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();

    let server_handle = tokio::spawn({
        let handle = handle.clone();
        async move {
            server
                .handle(handle)
                .serve(app.into_make_service())
                .await
                .unwrap();
        }
    });

    // Wait for the server to start listening.
    let listening_addr = loop {
        if let Some(addr) = handle.listening().await {
            break addr;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let resp = client
        .get(format!("https://{}/health", listening_addr))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");

    shutdown_handle.graceful_shutdown(None);
    let _ = server_handle.await;
}
