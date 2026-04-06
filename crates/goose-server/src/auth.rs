use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

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

    match secret_key {
        Some(key) if bool::from(key.as_bytes().ct_eq(state.as_bytes())) => {
            Ok(next.run(request).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
