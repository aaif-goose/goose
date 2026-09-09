use crate::config::paths::Paths;
use goose_providers::canonical::{load_cached_catalog, refresh_remote_catalog};
use std::path::PathBuf;

const CATALOG_URL_ENV: &str = "GOOSE_MODEL_CATALOG_URL";

fn cache_dir() -> PathBuf {
    Paths::in_data_dir("model_catalog")
}

pub fn initialize() {
    let cache_dir = cache_dir();
    if let Err(error) = load_cached_catalog(&cache_dir) {
        tracing::warn!(%error, "ignoring invalid cached model catalog");
    }

    let Ok(url) = std::env::var(CATALOG_URL_ENV) else {
        return;
    };
    if url.trim().is_empty() {
        return;
    }

    tokio::spawn(async move {
        if let Err(error) = refresh_remote_catalog(&url, &cache_dir).await {
            tracing::warn!(%error, "failed to refresh remote model catalog");
        }
    });
}
