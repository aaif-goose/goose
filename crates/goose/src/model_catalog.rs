use crate::config::paths::Paths;
use goose_providers::canonical::{fetch_remote_catalog, load_cached_catalog};
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

    tokio::spawn(async move {
        if let Err(error) = refresh(cache_dir).await {
            tracing::warn!(%error, "failed to refresh remote model catalog");
        }
    });
}

async fn refresh(cache_dir: PathBuf) -> anyhow::Result<()> {
    let Some(catalog) = fetch_remote_catalog(CATALOG_URL, &cache_dir).await? else {
        return Ok(());
    };
    // Parsing the catalog is slow enough to stall the runtime's timers. Only
    // this step goes to the blocking pool, so shutting the runtime down still
    // cancels the download instead of waiting for it.
    tokio::task::spawn_blocking(move || catalog.install(&cache_dir)).await?
}
