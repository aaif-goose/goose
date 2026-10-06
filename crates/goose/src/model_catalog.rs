use crate::config::paths::Paths;
use goose_providers::canonical::{load_cached_catalog, refresh_remote_catalog};
use std::path::PathBuf;

const CATALOG_URL: &str = "https://models.dev/api.json";

fn cache_dir() -> PathBuf {
    Paths::in_data_dir("model_catalog")
}

pub fn initialize() {
    let cache_dir = cache_dir();
    if let Err(error) = load_cached_catalog(&cache_dir) {
        tracing::warn!(%error, "ignoring invalid cached model catalog");
    }

    // Parsing the downloaded catalog is slow enough to stall the runtime's
    // timers, so the refresh runs on the blocking pool.
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        if let Err(error) = runtime.block_on(refresh_remote_catalog(CATALOG_URL, &cache_dir)) {
            tracing::warn!(%error, "failed to refresh remote model catalog");
        }
    });
}
