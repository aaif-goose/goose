use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

fn token_matches(candidate: Option<&str>, expected: &str) -> bool {
    candidate
        .map(|key| bool::from(key.as_bytes().ct_eq(expected.as_bytes())))
        .unwrap_or(false)
}

pub async fn check_token(
    State(state): State<String>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if request.uri().path() == "/status"
        || request.uri().path() == "/features"
        || request.uri().path() == "/mcp-ui-proxy"
        || request.uri().path() == "/mcp-app-proxy"
        || request.uri().path() == "/mcp-app-guest"
    {
        return Ok(next.run(request).await);
    }
    let secret_key = request
        .headers()
        .get("X-Secret-Key")
        .and_then(|value| value.to_str().ok());

    if token_matches(secret_key, &state) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub async fn check_acp_token(
    State(state): State<String>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let header_token = request
        .headers()
        .get("X-Secret-Key")
        .and_then(|value| value.to_str().ok());

    let query_token = request.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });

    if token_matches(header_token, &state) || token_matches(query_token.as_deref(), &state) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
