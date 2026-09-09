use reqwest::StatusCode;
use std::time::Duration;
use thiserror::Error;

use crate::request_log::LogError;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ProviderError {
    #[error("Provider is not configured")]
    NotConfigured,

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(String),

    #[error("Rate limit exceeded: {details}")]
    RateLimitExceeded {
        details: String,
        retry_delay: Option<Duration>,
    },

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    /// Bad input rather than an operational failure: retrying is pointless, but
    /// a different value may succeed.
    #[error("Invalid value: {0}")]
    InvalidValue(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Usage data error: {0}")]
    UsageError(String),

    #[error("Unsupported operation: {0}")]
    NotImplemented(String),

    #[error("Endpoint not found (404): {0}")]
    EndpointNotFound(String),

    #[error("Credits exhausted: {details}")]
    CreditsExhausted {
        details: String,
        top_up_url: Option<String>,
    },

    #[error("Provider refused request: {details}")]
    Refusal {
        details: String,
        category: Option<String>,
    },
}

impl ProviderError {
    pub fn stream_decode_error(error: impl std::fmt::Display) -> Self {
        ProviderError::NetworkError(format!("Stream decode error: {error}"))
    }

    pub fn telemetry_type(&self) -> &'static str {
        match self {
            ProviderError::NotConfigured => "not_configured",
            ProviderError::Authentication(_) => "auth",
            ProviderError::ContextLengthExceeded(_) => "context_length",
            ProviderError::RateLimitExceeded { .. } => "rate_limit",
            ProviderError::ServerError(_) => "server",
            ProviderError::NetworkError(_) => "network",
            ProviderError::RequestFailed(_) => "request",
            ProviderError::InvalidValue(_) => "invalid_value",
            ProviderError::ExecutionError(_) => "execution",
            ProviderError::UsageError(_) => "usage",
            ProviderError::NotImplemented(_) => "not_implemented",
            ProviderError::EndpointNotFound(_) => "endpoint_not_found",
            ProviderError::CreditsExhausted { .. } => "credits_exhausted",
            ProviderError::Refusal { .. } => "refusal",
        }
    }

    pub fn is_endpoint_not_found(&self) -> bool {
        matches!(self, ProviderError::EndpointNotFound(_))
    }

    /// Recover a typed `ProviderError` from a streaming decode error, falling
    /// back to a retryable stream decode error for errors that did not
    /// originate as one.
    pub fn from_stream_error(error: anyhow::Error) -> Self {
        error
            .downcast()
            .unwrap_or_else(ProviderError::stream_decode_error)
    }
}

fn is_network_error(err: &reqwest::Error) -> bool {
    err.is_connect() || err.is_timeout() || (err.status().is_none() && err.is_request())
}

fn sanitized_reqwest_url(error: &reqwest::Error) -> Option<String> {
    let mut url = error.url()?.clone();
    let _ = url.set_password(None);
    let _ = url.set_username("");
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

fn reqwest_error_category(error: &reqwest::Error) -> &'static str {
    if error.is_builder() {
        "Request builder error"
    } else if error.is_redirect() {
        "Redirect error"
    } else if error.is_status() {
        "HTTP status error"
    } else if error.is_body() {
        "Request body error"
    } else if error.is_decode() {
        "Response decode error"
    } else {
        "Request error"
    }
}

fn provider_error_from_reqwest(error: &reqwest::Error) -> ProviderError {
    if is_network_error(error) {
        let msg = if error.is_timeout() {
            "Request timed out — check your network connection and try again.".to_string()
        } else if error.is_connect() {
            if let Some(url) = error.url() {
                if let Some(host) = url.host_str() {
                    let port_info = url.port().map(|p| format!(":{}", p)).unwrap_or_default();
                    format!(
                        "Could not connect to {}{} — check your network connection and try again.",
                        host, port_info
                    )
                } else {
                    "Could not connect to the provider — check your network connection and try again.".to_string()
                }
            } else {
                "Could not connect to the provider — check your network connection and try again."
                    .to_string()
            }
        } else {
            "Network error — check your network connection and try again.".to_string()
        };
        return ProviderError::NetworkError(msg);
    }

    let mut details = Vec::new();
    if let Some(status) = error.status() {
        details.push(format!("status: {}", status));
    }
    if let Some(url) = sanitized_reqwest_url(error) {
        details.push(format!("url: {url}"));
    }

    let category = reqwest_error_category(error);
    let msg = if details.is_empty() {
        category.to_string()
    } else {
        format!("{category} ({})", details.join(", "))
    };
    ProviderError::RequestFailed(msg)
}

impl From<anyhow::Error> for ProviderError {
    fn from(error: anyhow::Error) -> Self {
        if let Some(provider_error) = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<ProviderError>())
        {
            return provider_error.clone();
        }
        if let Some(reqwest_err) = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<reqwest::Error>())
        {
            return provider_error_from_reqwest(reqwest_err);
        }
        if error.chain().any(|cause| {
            cause
                .downcast_ref::<tokio::time::error::Elapsed>()
                .is_some()
        }) {
            return ProviderError::NetworkError(
                "Request timed out — check your network connection and try again.".to_string(),
            );
        }
        ProviderError::ExecutionError(error.to_string())
    }
}

impl From<reqwest::Error> for ProviderError {
    fn from(error: reqwest::Error) -> Self {
        provider_error_from_reqwest(&error)
    }
}

impl From<LogError> for ProviderError {
    fn from(value: LogError) -> Self {
        ProviderError::ExecutionError(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[tokio::test]
    async fn direct_reqwest_error_redacts_unsupported_scheme_url() {
        let error = reqwest::Client::new()
            .get("ftp://url-user:url-password@provider.invalid/chat?api_key=query-secret")
            .send()
            .await
            .unwrap_err();

        assert!(error.to_string().contains("query-secret"));
        assert!(!is_network_error(&error));

        let message = ProviderError::from(error).to_string();
        assert!(!message.contains("query-secret"));
        assert!(!message.contains("url-user"));
        assert!(!message.contains("url-password"));
        assert!(message.contains("ftp://provider.invalid/chat"));
    }

    #[tokio::test]
    async fn anyhow_wrapped_status_error_redacts_url() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });

        let error = reqwest::Client::new()
            .get(format!(
                "http://url-user:url-password@{address}/chat?api_key=query-secret"
            ))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap_err();
        server.join().unwrap();

        assert!(error.to_string().contains("query-secret"));
        assert_eq!(error.status(), Some(StatusCode::UNAUTHORIZED));

        let message = ProviderError::from(anyhow::Error::new(error)).to_string();
        assert!(!message.contains("query-secret"));
        assert!(!message.contains("url-user"));
        assert!(!message.contains("url-password"));
        assert!(message.contains("status: 401 Unauthorized"));
        assert!(message.contains(&format!("http://{address}/chat")));
    }
}

#[derive(Debug)]
pub enum GoogleErrorCode {
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    TooManyRequests = 429,
    InternalServerError = 500,
    ServiceUnavailable = 503,
}

impl GoogleErrorCode {
    pub fn to_status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    pub fn from_code(code: u64) -> Option<Self> {
        match code {
            400 => Some(Self::BadRequest),
            401 => Some(Self::Unauthorized),
            403 => Some(Self::Forbidden),
            404 => Some(Self::NotFound),
            429 => Some(Self::TooManyRequests),
            500 => Some(Self::InternalServerError),
            503 => Some(Self::ServiceUnavailable),
            _ => Some(Self::InternalServerError),
        }
    }
}
