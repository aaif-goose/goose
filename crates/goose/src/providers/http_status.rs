//! Format-agnostic HTTP status → `ProviderError` mapping.
//!
//! Used by providers regardless of their wire format (OpenAI, Anthropic,
//! Google, etc.). Parses both `{"error":{"message":"..."}}` and
//! `{"message":"..."}` error shapes.

use std::time::{Duration, SystemTime};

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::{Response, StatusCode};
use serde_json::Value;

use super::errors::ProviderError;

/// Hard cap on retry delays we'll honor from remote responses. A malformed
/// 429 with `retry_after_seconds: 1e30` (or a far-future HTTP-date) should
/// degrade to "no retry hint" rather than freeze the agent or panic when
/// converting to `Duration`. One hour is well past any legitimate
/// rate-limit window.
const MAX_RETRY_AFTER_SECS: f64 = 3600.0;

/// Extract a retry delay from a 429 response. Prefers the body's
/// `error.metadata.retry_after_seconds` (OpenRouter shape, more precise than
/// the integer header) and falls back to the RFC 7231 `Retry-After` header
/// in either its delay-seconds form or its HTTP-date form.
fn extract_retry_after(headers: &HeaderMap, payload: Option<&Value>) -> Option<Duration> {
    if let Some(secs) = payload
        .and_then(|p| p.get("error"))
        .and_then(|e| e.get("metadata"))
        .and_then(|m| m.get("retry_after_seconds"))
        .and_then(|v| v.as_f64())
    {
        if let Some(d) = duration_from_finite_secs(secs) {
            return Some(d);
        }
    }

    headers
        .get(RETRY_AFTER)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| parse_retry_after_header(s.trim()))
}

/// Convert a finite, non-negative, in-range seconds value to a `Duration`.
/// Returns `None` for NaN, negative, infinite, or absurdly large inputs —
/// `Duration::from_secs_f64` panics on the latter.
fn duration_from_finite_secs(secs: f64) -> Option<Duration> {
    if !secs.is_finite() || secs < 0.0 {
        return None;
    }
    let clamped = secs.min(MAX_RETRY_AFTER_SECS);
    Some(Duration::from_secs_f64(clamped))
}

/// Parse `Retry-After` per RFC 7231 §7.1.3: either a non-negative integer
/// number of seconds, or an HTTP-date (interpreted as the absolute time at
/// which the request may be retried). A past date is honored as "retry
/// now" (`Duration::ZERO`) rather than dropped — clock skew or near-now
/// timestamps plus network latency commonly produce an HTTP-date that is
/// already in the past, and falling back to exponential backoff would
/// add unnecessary delay against an explicit server hint.
fn parse_retry_after_header(value: &str) -> Option<Duration> {
    if let Ok(secs) = value.parse::<u64>() {
        return duration_from_finite_secs(secs as f64);
    }
    let target = parse_http_date(value)?;
    let delay = target
        .duration_since(SystemTime::now())
        .unwrap_or(Duration::ZERO);
    duration_from_finite_secs(delay.as_secs_f64())
}

/// Parse the three HTTP-date forms RFC 7231 §7.1.1.1 requires recipients to
/// accept: IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`), the obsolete RFC 850
/// form (`Sunday, 06-Nov-94 08:49:37 GMT`), and asctime (`Sun Nov  6 08:49:37
/// 1994`). All three are interpreted as GMT.
fn parse_http_date(value: &str) -> Option<SystemTime> {
    let value = value.trim();
    if let Ok(dt) = DateTime::parse_from_rfc2822(value) {
        return Some(SystemTime::from(dt));
    }
    if let Some(body) = value.strip_suffix(" GMT") {
        if let Ok(naive) = NaiveDateTime::parse_from_str(body, "%A, %d-%b-%y %H:%M:%S") {
            return Some(SystemTime::from(Utc.from_utc_datetime(&naive)));
        }
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%a %b %e %H:%M:%S %Y") {
        return Some(SystemTime::from(Utc.from_utc_datetime(&naive)));
    }
    None
}

fn check_context_length_exceeded(text: &str) -> bool {
    let check_phrases = [
        "too long",
        "context length",
        "context_length_exceeded",
        "reduce the length",
        "token count",
        "exceeds",
        "exceed context limit",
        "input length",
        "max_tokens",
        "decrease input length",
        "context limit",
        "maximum prompt length",
    ];
    let text_lower = text.to_lowercase();
    check_phrases
        .iter()
        .any(|phrase| text_lower.contains(phrase))
}

pub fn map_http_error_to_provider_error(
    status: StatusCode,
    payload: Option<Value>,
) -> ProviderError {
    let extract_message = || -> String {
        payload
            .as_ref()
            .and_then(|p| {
                p.get("error")
                    .and_then(|e| e.get("message"))
                    .or_else(|| p.get("message"))
                    .and_then(|m| m.as_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| payload.as_ref().map(|p| p.to_string()).unwrap_or_default())
    };

    let error = match status {
        StatusCode::OK => unreachable!("Should not call this function with OK status"),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::Authentication(format!(
            "Authentication failed. Status: {}. Response: {}",
            status,
            extract_message()
        )),
        StatusCode::NOT_FOUND => {
            ProviderError::RequestFailed(format!("Resource not found (404): {}", extract_message()))
        }
        StatusCode::PAYMENT_REQUIRED => ProviderError::CreditsExhausted {
            details: extract_message(),
            top_up_url: None,
        },
        StatusCode::PAYLOAD_TOO_LARGE => ProviderError::ContextLengthExceeded(extract_message()),
        StatusCode::BAD_REQUEST => {
            let payload_str = extract_message();
            if check_context_length_exceeded(&payload_str) {
                ProviderError::ContextLengthExceeded(payload_str)
            } else {
                ProviderError::RequestFailed(format!("Bad request (400): {}", payload_str))
            }
        }
        StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimitExceeded {
            details: extract_message(),
            retry_delay: None,
        },
        _ if status.is_server_error() => {
            ProviderError::ServerError(format!("Server error ({}): {}", status, extract_message()))
        }
        _ => ProviderError::RequestFailed(format!(
            "Request failed with status {}: {}",
            status,
            extract_message()
        )),
    };

    if !status.is_success() {
        tracing::warn!(
            "Provider request failed with status: {}. Payload: {:?}. Returning error: {:?}",
            status,
            payload,
            error
        );
    }

    error
}

pub async fn handle_status(response: Response) -> Result<Response, ProviderError> {
    let status = response.status();
    if !status.is_success() {
        let headers = response.headers().clone();
        let body = response.text().await.unwrap_or_default();
        let payload = serde_json::from_str::<Value>(&body).ok();
        let mut err = map_http_error_to_provider_error(status, payload.clone());
        if let ProviderError::RateLimitExceeded { details, .. } = &err {
            err = ProviderError::RateLimitExceeded {
                details: details.clone(),
                retry_delay: extract_retry_after(&headers, payload.as_ref()),
            };
        }
        return Err(err);
    }
    Ok(response)
}

pub async fn handle_response(response: Response) -> Result<Value, ProviderError> {
    let response = handle_status(response).await?;

    response.json::<Value>().await.map_err(|e| {
        ProviderError::RequestFailed(format!("Response body is not valid JSON: {}", e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn empty_headers() -> HeaderMap {
        HeaderMap::new()
    }

    fn headers_with_retry_after(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, value.parse().unwrap());
        h
    }

    #[test]
    fn retry_after_prefers_body_seconds_over_header() {
        let payload = json!({
            "error": {
                "metadata": { "retry_after_seconds": 22.148 }
            }
        });
        let headers = headers_with_retry_after("5");
        let delay = extract_retry_after(&headers, Some(&payload));
        assert_eq!(delay, Some(Duration::from_secs_f64(22.148)));
    }

    #[test]
    fn retry_after_falls_back_to_header_when_body_missing() {
        let headers = headers_with_retry_after("17");
        let delay = extract_retry_after(&headers, None);
        assert_eq!(delay, Some(Duration::from_secs(17)));
    }

    #[test]
    fn retry_after_returns_none_when_neither_present() {
        let payload = json!({ "error": { "message": "rate limited" } });
        let delay = extract_retry_after(&empty_headers(), Some(&payload));
        assert!(delay.is_none());
    }

    #[test]
    fn retry_after_ignores_negative_or_nan_body_seconds() {
        let payload = json!({ "error": { "metadata": { "retry_after_seconds": -1.0 } } });
        assert!(extract_retry_after(&empty_headers(), Some(&payload)).is_none());

        let payload = json!({ "error": { "metadata": { "retry_after_seconds": "not a number" } } });
        assert!(extract_retry_after(&empty_headers(), Some(&payload)).is_none());
    }

    #[test]
    fn retry_after_past_http_date_means_retry_now() {
        // RFC 7231 allows an HTTP-date in `Retry-After`; a past date means
        // "you may retry now" — surface that as `Duration::ZERO` rather
        // than `None`, so we honor the server hint instead of dropping
        // back to exponential backoff (clock skew or near-now timestamps
        // plus latency commonly land us here).
        let headers = headers_with_retry_after("Fri, 31 Dec 1999 23:59:59 GMT");
        let delay = extract_retry_after(&headers, None);
        assert_eq!(delay, Some(Duration::ZERO));
    }

    #[test]
    fn retry_after_future_http_date_parsed() {
        // A future HTTP-date should produce a positive duration up to the cap.
        let target = chrono::Utc::now() + chrono::Duration::seconds(45);
        let header_value = target.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let headers = headers_with_retry_after(&header_value);
        let delay = extract_retry_after(&headers, None).expect("should parse future HTTP-date");
        // Allow some slack for the clock advancing between header creation and parse.
        assert!(
            delay >= Duration::from_secs(30) && delay <= Duration::from_secs(60),
            "expected ~45s, got {delay:?}"
        );
    }

    #[test]
    fn retry_after_parses_rfc850_http_date() {
        // RFC 7231 recipients must accept the obsolete RFC 850 date syntax.
        // Build a future date in that form and assert it round-trips.
        let target = chrono::Utc::now() + chrono::Duration::seconds(90);
        let header_value = target.format("%A, %d-%b-%y %H:%M:%S GMT").to_string();
        let headers = headers_with_retry_after(&header_value);
        let delay = extract_retry_after(&headers, None).expect("rfc850 date should parse");
        assert!(
            delay >= Duration::from_secs(60) && delay <= Duration::from_secs(120),
            "expected ~90s, got {delay:?}"
        );
    }

    #[test]
    fn retry_after_parses_asctime_http_date() {
        // The third HTTP-date form RFC 7231 requires recipients to accept.
        // asctime has no timezone marker; we interpret it as GMT.
        let target = chrono::Utc::now() + chrono::Duration::seconds(120);
        let header_value = target.format("%a %b %e %H:%M:%S %Y").to_string();
        let headers = headers_with_retry_after(&header_value);
        let delay = extract_retry_after(&headers, None).expect("asctime date should parse");
        assert!(
            delay >= Duration::from_secs(90) && delay <= Duration::from_secs(150),
            "expected ~120s, got {delay:?}"
        );
    }

    #[test]
    fn retry_after_clamps_absurd_body_seconds() {
        // `Duration::from_secs_f64(1e30)` panics; the clamp keeps the agent alive.
        let payload = json!({ "error": { "metadata": { "retry_after_seconds": 1e30 } } });
        let delay = extract_retry_after(&empty_headers(), Some(&payload));
        assert_eq!(delay, Some(Duration::from_secs_f64(MAX_RETRY_AFTER_SECS)));
    }

    #[test]
    fn retry_after_clamps_infinite_body_seconds() {
        let payload = json!({ "error": { "metadata": { "retry_after_seconds": f64::INFINITY } } });
        assert!(extract_retry_after(&empty_headers(), Some(&payload)).is_none());
    }
}
