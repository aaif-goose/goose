//! TLS configuration for the goose server.
//!
//! Two TLS backends are supported for the HTTPS listener via `axum-server`:
//!
//! - **`rustls-tls`** (enabled by default) – uses `axum-server/tls-rustls` with
//!   the `aws-lc-rs` crypto provider.
//! - **`native-tls`** – uses `axum-server/tls-openssl`, which links against the
//!   platform's OpenSSL (or a compatible fork such as LibreSSL / BoringSSL).
//!   On Linux this *is* the platform-native TLS stack; on macOS/Windows the
//!   `native-tls` crate used by the HTTP *client* delegates to Security.framework
//!   / SChannel respectively, but `axum-server` does not offer those backends so
//!   the server listener always uses OpenSSL when this feature is active.

use anyhow::Result;
use rcgen::{CertificateParams, DnType, KeyPair, SanType};

#[cfg(feature = "rustls-tls")]
pub type TlsConfig = axum_server::tls_rustls::RustlsConfig;

#[cfg(feature = "native-tls")]
pub type TlsConfig = axum_server::tls_openssl::OpenSSLConfig;

pub struct TlsSetup {
    pub config: TlsConfig,
    pub fingerprint: String,
}

fn generate_self_signed_cert() -> Result<(rcgen::Certificate, KeyPair)> {
    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(DnType::CommonName, "goosed localhost");
    params.subject_alt_names = vec![
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        SanType::DnsName("localhost".try_into()?),
    ];

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;
    Ok((cert, key_pair))
}

fn sha256_fingerprint(der: &[u8]) -> String {
    #[cfg(feature = "rustls-tls")]
    {
        let sha256 = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, der);
        sha256
            .as_ref()
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }

    #[cfg(feature = "native-tls")]
    {
        use openssl::hash::MessageDigest;
        let digest =
            openssl::hash::hash(MessageDigest::sha256(), der).expect("SHA-256 hash failed");
        digest
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }
}

/// Generate a self-signed TLS certificate for localhost (127.0.0.1) and
/// return a [`TlsSetup`] containing the server config and the SHA-256
/// fingerprint of the generated certificate (colon-separated hex).
///
/// The fingerprint is printed to stdout so the parent process (e.g. Electron)
/// can pin it and reject connections from any other certificate.
pub async fn self_signed_config() -> Result<TlsSetup> {
    #[cfg(feature = "rustls-tls")]
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let (cert, key_pair) = generate_self_signed_cert()?;

    let fingerprint = sha256_fingerprint(cert.der());
    println!("GOOSED_CERT_FINGERPRINT={fingerprint}");

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    #[cfg(feature = "rustls-tls")]
    let config = axum_server::tls_rustls::RustlsConfig::from_pem(
        cert_pem.into_bytes(),
        key_pem.into_bytes(),
    )
    .await?;

    #[cfg(feature = "native-tls")]
    let config =
        axum_server::tls_openssl::OpenSSLConfig::from_pem(cert_pem.as_bytes(), key_pem.as_bytes())?;

    Ok(TlsSetup {
        config,
        fingerprint,
    })
}
