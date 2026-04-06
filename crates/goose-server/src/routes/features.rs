use axum::{routing::get, Json, Router};
use serde::Serialize;
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct FeaturesResponse {
    /// Map of feature name to enabled status
    pub features: HashMap<String, bool>,
}

#[utoipa::path(
    get,
    path = "/features",
    responses(
        (status = 200, description = "Compile-time feature flags", body = FeaturesResponse),
    )
)]
pub async fn get_features() -> Json<FeaturesResponse> {
    let mut features = HashMap::new();

    features.insert(
        "local-inference".to_string(),
        cfg!(feature = "local-inference"),
    );
    features.insert("code-mode".to_string(), cfg!(feature = "code-mode"));

    Json(FeaturesResponse { features })
}

pub fn routes() -> Router {
    Router::new().route("/features", get(get_features))
}
