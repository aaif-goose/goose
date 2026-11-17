#![cfg(feature = "live-websocket")]

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use goose_providers::{
    api_client::TlsConfig,
    live_transport_websocket::WebSocketLiveTransport,
    openai_live::{
        OpenAiLiveClient, OpenAiLiveSessionConfig, OpenAiLiveSessionId, OpenAiLiveWebSocketRequest,
    },
};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_rustls::{rustls, TlsAcceptor};
use tokio_tungstenite::{accept_hdr_async, tungstenite::Message};

#[tokio::test]
#[allow(clippy::result_large_err)]
async fn websocket_connect_performs_handshake_and_waits_until_ready() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_hdr_async(stream, |request: &http::Request<()>, response| {
            assert_eq!(request.headers()["authorization"], "Bearer test-key");
            Ok(response)
        })
        .await
        .unwrap();
        let message = socket.next().await.unwrap().unwrap();
        let Message::Text(text) = message else {
            panic!("expected initial text message");
        };
        let session_start: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(session_start["type"], "session.start");
        socket
            .send(Message::Text(
                json!({
                    "type": "session.started",
                    "event_id": "event_started_1",
                    "session": { "id": "session_1" }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
    });

    let connected = OpenAiLiveClient::new("test-key")
        .with_websocket_endpoint(format!("ws://{address}/v1/live/sessions"))
        .websocket(OpenAiLiveSessionConfig {
            model: "gpt-live-test".into(),
            instructions: "test".into(),
            voice: None,
            input_messages: vec![],
            extra_session_fields: Default::default(),
        })
        .connect()
        .await?;

    let _connected = connected;
    server.await?;
    Ok(())
}

#[test_case::test_case(false, "localhost", false; "rejects an untrusted certificate")]
#[test_case::test_case(true, "localhost", true; "accepts a configured CA")]
#[test_case::test_case(true, "127.0.0.1", false; "verifies the hostname with a configured CA")]
#[tokio::test]
async fn websocket_uses_provider_tls_configuration(
    trust_certificate: bool,
    hostname: &str,
    expect_success: bool,
) -> Result<()> {
    let mut ca_params = rcgen::CertificateParams::default();
    ca_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Goose WebSocket test CA");
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign];
    let ca = rcgen::CertifiedIssuer::self_signed(ca_params, rcgen::KeyPair::generate()?)?;
    let server_key = rcgen::KeyPair::generate()?;
    let mut server_params = rcgen::CertificateParams::new(vec!["localhost".into()])?;
    server_params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let certificate = server_params.signed_by(&server_key, &ca)?;
    let ca_file = tempfile::NamedTempFile::new()?;
    std::fs::write(ca_file.path(), ca.pem())?;
    let server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(
        vec![certificate.der().clone()],
        rustls::pki_types::PrivatePkcs8KeyDer::from(server_key.serialize_der()).into(),
    )?;
    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        if let Ok(stream) = acceptor.accept(stream).await {
            let mut socket = tokio_tungstenite::accept_async(stream).await?;
            socket
                .send(Message::Text(json!({ "type": "test" }).to_string().into()))
                .await?;
        }
        anyhow::Ok(())
    });

    let client = OpenAiLiveClient::new("test-key")
        .with_websocket_endpoint(format!("wss://{hostname}:{port}/v1/live/sessions"));
    let client = if trust_certificate {
        client.with_tls_config(TlsConfig::new().with_ca_cert(ca_file.path().to_path_buf()))
    } else {
        client
    };
    let result = timeout(
        Duration::from_secs(5),
        client
            .existing_session(OpenAiLiveSessionId("session_1".into()))
            .connect(),
    )
    .await?;
    assert_eq!(result.is_ok(), expect_success, "{:?}", result.err());
    timeout(Duration::from_secs(5), server).await???;
    Ok(())
}

#[tokio::test]
#[allow(clippy::result_large_err)]
async fn websocket_rejects_an_invalid_upgrade_response() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let _ = accept_hdr_async(
            stream,
            |_: &http::Request<()>, mut response: http::Response<()>| {
                response
                    .headers_mut()
                    .insert("sec-websocket-accept", "invalid".parse().unwrap());
                Ok(response)
            },
        )
        .await;
        anyhow::Ok(())
    });
    let request = OpenAiLiveWebSocketRequest {
        endpoint: format!("ws://{address}/"),
        headers: vec![],
        initial_messages: vec![],
    };
    assert!(WebSocketLiveTransport::connect(request).await.is_err());
    timeout(Duration::from_secs(5), server).await???;
    Ok(())
}

#[tokio::test]
async fn websocket_does_not_follow_redirects() -> Result<()> {
    let target = wiremock::MockServer::start().await;
    let redirect = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(302).insert_header("Location", target.uri()))
        .mount(&redirect)
        .await;
    let request = OpenAiLiveWebSocketRequest {
        endpoint: redirect.uri().replacen("http://", "ws://", 1),
        headers: vec![("Authorization".into(), "Bearer test-key".into())],
        initial_messages: vec![],
    };
    assert!(WebSocketLiveTransport::connect(request).await.is_err());
    assert!(target.received_requests().await.unwrap().is_empty());
    Ok(())
}
