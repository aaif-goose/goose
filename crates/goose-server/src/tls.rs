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

use anyhow::{bail, Result};
use rcgen::{CertificateParams, DnType, KeyPair, SanType};
use std::path::Path;

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

/// Load TLS configuration from user-provided PEM certificate and key files.
///
/// The SHA-256 fingerprint of the leaf certificate is computed and printed to
/// stdout so the parent process (e.g. Electron) can pin it, just like the
/// self-signed path.
pub async fn from_pem_files(cert_path: &Path, key_path: &Path) -> Result<TlsSetup> {
    let cert_pem = std::fs::read(cert_path)?;
    let key_pem = std::fs::read(key_path)?;

    // Parse the first PEM block to extract the DER-encoded certificate for fingerprinting.
    let der = pem::parse(&cert_pem)?.into_contents();
    let fingerprint = sha256_fingerprint(&der);
    println!("GOOSED_CERT_FINGERPRINT={fingerprint}");

    #[cfg(feature = "rustls-tls")]
    let config = {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        axum_server::tls_rustls::RustlsConfig::from_pem(cert_pem, key_pem.clone()).await?
    };

    #[cfg(feature = "native-tls")]
    let config = axum_server::tls_openssl::OpenSSLConfig::from_pem(&cert_pem, &key_pem)?;

    Ok(TlsSetup {
        config,
        fingerprint,
    })
}

/// Set up TLS, using user-provided PEM files if both paths are given,
/// otherwise generating a self-signed certificate.
pub async fn setup_tls(cert_path: Option<&str>, key_path: Option<&str>) -> Result<TlsSetup> {
    match (cert_path, key_path) {
        (Some(cert), Some(key)) => from_pem_files(Path::new(cert), Path::new(key)).await,
        (None, None) => self_signed_config().await,
        _ => bail!("Both GOOSE_TLS_CERT_PATH and GOOSE_TLS_KEY_PATH must be set, or neither"),
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
