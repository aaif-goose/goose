use axum::{
    extract::{Path, Query, Request, State},
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::acp::transport::auth::token_matches;
use crate::config::paths::Paths;
use crate::config::Config;
use crate::plugins::formats::goose_native::read_manifest;

#[derive(Clone)]
struct ConfigApiState {
    secret_key: String,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

#[derive(Serialize)]
pub struct ConfigValueInfo {
    pub value: serde_json::Value,
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
}

fn check_auth(
    state: &ConfigApiState,
    headers: &axum::http::HeaderMap,
    token: Option<&str>,
) -> bool {
    let header_token = headers.get("X-Secret-Key").and_then(|v| v.to_str().ok());
    token_matches(header_token, &state.secret_key) || token_matches(token, &state.secret_key)
}

async fn get_plugin_config(
    State(state): State<ConfigApiState>,
    Path(plugin_id): Path<String>,
    Query(query): Query<TokenQuery>,
    req: Request,
) -> Result<Json<HashMap<String, ConfigValueInfo>>, StatusCode> {
    if !check_auth(&state, req.headers(), query.token.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let plugins_dir = Paths::plugins_dir();
    let plugin_dir = plugins_dir.join(&plugin_id);
    let Ok(Some(manifest)) = read_manifest(&plugin_dir) else {
        return Err(StatusCode::NOT_FOUND);
    };

    let config = Config::global();
    let mut result = HashMap::new();

    for (key, entry) in &manifest.config {
        let config_key = format!("plugins.{}.config.{}", manifest.name, key);
        let value = if config.get_secret::<String>(&config_key).is_ok() {
            serde_json::Value::String("****".to_string())
        } else {
            config
                .get_param::<serde_json::Value>(&config_key)
                .ok()
                .or_else(|| entry.default.clone())
                .unwrap_or(serde_json::Value::Null)
        };
        result.insert(
            key.clone(),
            ConfigValueInfo {
                value,
                kind: entry.kind.clone(),
                description: entry.description.clone(),
            },
        );
    }

    Ok(Json(result))
}

async fn patch_plugin_config(
    State(state): State<ConfigApiState>,
    Path(plugin_id): Path<String>,
    Query(query): Query<TokenQuery>,
    req: Request,
) -> Result<StatusCode, StatusCode> {
    if !check_auth(&state, req.headers(), query.token.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let plugins_dir = Paths::plugins_dir();
    let plugin_dir = plugins_dir.join(&plugin_id);
    let Ok(Some(manifest)) = read_manifest(&plugin_dir) else {
        return Err(StatusCode::NOT_FOUND);
    };

    let body_bytes = axum::body::to_bytes(req.into_body(), 1 << 20)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let updates: HashMap<String, serde_json::Value> =
        serde_json::from_slice(&body_bytes).map_err(|_| StatusCode::BAD_REQUEST)?;

    let config = Config::global();
    for (key, value) in updates {
        let Some(entry) = manifest.config.get(&key) else {
            continue;
        };
        let config_key = format!("plugins.{}.config.{}", manifest.name, key);
        if entry.kind == "secret" || config.get_secret::<String>(&config_key).is_ok() {
            continue;
        }
        config
            .set_param(&config_key, value)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(StatusCode::NO_CONTENT)
}

pub fn routes(secret_key: String) -> Router {
    let state = ConfigApiState { secret_key };
    Router::new()
        .route("/plugin-config/{plugin_id}", get(get_plugin_config))
        .route("/plugin-config/{plugin_id}", patch(patch_plugin_config))
        .with_state(state)
}
