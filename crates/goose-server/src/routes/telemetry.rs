use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
#[cfg(feature = "telemetry")]
use goose::posthog::emit_event;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct TelemetryEventRequest {
    pub event_name: String,
    #[serde(default)]
    pub properties: HashMap<String, serde_json::Value>,
}

#[utoipa::path(
    post,
    path = "/telemetry/event",
    request_body = TelemetryEventRequest,
    responses(
        (status = 202, description = "Event accepted for processing")
    )
)]
async fn send_telemetry_event(
    State(_state): State<Arc<AppState>>,
    Json(request): Json<TelemetryEventRequest>,
) -> StatusCode {
    let event_name = request.event_name;
    let properties = request.properties;

    #[cfg(feature = "telemetry")]
    tokio::spawn(async move {
        if let Err(e) = emit_event(&event_name, properties).await {
            tracing::debug!("Failed to send telemetry event: {}", e);
        }
    });

    StatusCode::ACCEPTED
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/telemetry/event", post(send_telemetry_event))
        .with_state(state)
}
