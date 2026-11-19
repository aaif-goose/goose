use super::CanonicalModel;
use anyhow::{bail, Context, Result};
use futures::StreamExt;
use once_cell::sync::Lazy;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{RwLock, RwLockReadGuard};
use std::time::Duration;

const MAX_CATALOG_BYTES: usize = 32 * 1024 * 1024;
const CATALOG_FILENAME: &str = "models_dev_api.json";
const ETAG_FILENAME: &str = "models_dev_api.etag";

static ACTIVE_REGISTRY: Lazy<RwLock<Result<CanonicalModelRegistry>>> = Lazy::new(|| {
    let bundled = zstd::decode_all(
        include_bytes!(concat!(env!("OUT_DIR"), "/canonical_models.json.zst")).as_slice(),
    )
    .context("Failed to decompress bundled canonical models JSON")
    .and_then(|json| CanonicalModelRegistry::from_json(std::str::from_utf8(&json)?));
    RwLock::new(bundled)
});

pub struct CanonicalModelRegistryGuard(RwLockReadGuard<'static, Result<CanonicalModelRegistry>>);

impl Deref for CanonicalModelRegistryGuard {
    type Target = CanonicalModelRegistry;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("validated registry")
    }
}

#[derive(Debug, Clone)]
pub struct CanonicalModelRegistry {
    models: HashMap<(String, String), CanonicalModel>,
}

impl CanonicalModelRegistry {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }

    pub fn bundled() -> Result<CanonicalModelRegistryGuard> {
        let guard = ACTIVE_REGISTRY
            .read()
            .map_err(|_| anyhow::anyhow!("canonical model registry lock poisoned"))?;
        if let Err(error) = &*guard {
            anyhow::bail!("{error}");
        }
        Ok(CanonicalModelRegistryGuard(guard))
    }

    pub fn from_json(content: &str) -> Result<Self> {
        let models: Vec<CanonicalModel> =
            serde_json::from_str(content).context("Failed to parse canonical models JSON")?;
        if models.is_empty() {
            bail!("canonical model catalog is empty");
        }

        let mut seen = HashSet::new();
        let mut registry = Self::new();
        for model in models {
            if !seen.insert(model.id.clone()) {
                bail!("duplicate canonical model id: {}", model.id);
            }
            let (provider, model_name) = model
                .id
                .split_once('/')
                .with_context(|| format!("invalid canonical model id: {}", model.id))?;
            registry.register(provider, model_name, model.clone());
        }
        Ok(registry)
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .context("Failed to read canonical models file")?;
        Self::from_json(&content)
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut models: Vec<&CanonicalModel> = self.models.values().collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));
        let json = serde_json::to_string_pretty(&models)
            .context("Failed to serialize canonical models")?;
        std::fs::write(path.as_ref(), json).context("Failed to write canonical models file")?;
        Ok(())
    }

    pub fn register(&mut self, provider: &str, model: &str, canonical_model: CanonicalModel) {
        self.models
            .insert((provider.to_string(), model.to_string()), canonical_model);
    }

    pub fn get(&self, provider: &str, model: &str) -> Option<&CanonicalModel> {
        self.models.get(&(provider.to_string(), model.to_string()))
    }

    pub fn get_all_models_for_provider(&self, provider: &str) -> Vec<CanonicalModel> {
        self.models
            .iter()
            .filter(|((p, _), _)| p == provider)
            .map(|(_, model)| model.clone())
            .collect()
    }

    pub fn all_models(&self) -> Vec<&CanonicalModel> {
        self.models.values().collect()
    }

    pub fn count(&self) -> usize {
        self.models.len()
    }
}

impl Default for CanonicalModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn activate(registry: CanonicalModelRegistry) -> Result<()> {
    *ACTIVE_REGISTRY
        .write()
        .map_err(|_| anyhow::anyhow!("canonical model registry lock poisoned"))? = Ok(registry);
    Ok(())
}

pub fn load_cached_catalog(cache_dir: &Path) -> Result<bool> {
    let path = cache_dir.join(CATALOG_FILENAME);
    if !path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(path)?;
    activate(super::models_dev::from_models_dev(&content)?)?;
    Ok(true)
}

pub struct RemoteCatalog {
    body: Vec<u8>,
    etag: Option<String>,
}

/// Returns `None` when the cached catalog is still current.
pub async fn fetch_remote_catalog(url: &str, cache_dir: &Path) -> Result<Option<RemoteCatalog>> {
    let client = reqwest::Client::builder().build()?;
    let etag_path = cache_dir.join(ETAG_FILENAME);
    let mut request = client.get(url).timeout(Duration::from_secs(15)).header(
        "User-Agent",
        "goose/model-catalog (https://github.com/aaif-goose/goose)",
    );
    if let Ok(etag) = std::fs::read_to_string(&etag_path) {
        request = request.header(IF_NONE_MATCH, etag);
    }

    let response = request.send().await?.error_for_status()?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(None);
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_CATALOG_BYTES as u64)
    {
        bail!("remote canonical model catalog exceeds size limit");
    }
    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if body.len() + chunk.len() > MAX_CATALOG_BYTES {
            bail!("remote canonical model catalog exceeds size limit");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Some(RemoteCatalog { body, etag }))
}

impl RemoteCatalog {
    pub fn install(self, cache_dir: &Path) -> Result<()> {
        let content =
            std::str::from_utf8(&self.body).context("canonical model catalog is not UTF-8")?;
        let registry = super::models_dev::from_models_dev(content)?;

        std::fs::create_dir_all(cache_dir)?;
        atomic_write(cache_dir.join(CATALOG_FILENAME), &self.body)?;
        if let Some(etag) = self.etag {
            atomic_write(cache_dir.join(ETAG_FILENAME), etag.as_bytes())?;
        }
        activate(registry)
    }
}

fn atomic_write(destination: PathBuf, content: &[u8]) -> Result<()> {
    let temporary = destination.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&temporary, content)?;
    if let Err(error) = std::fs::rename(&temporary, &destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_duplicate_and_malformed_catalogs() {
        assert!(CanonicalModelRegistry::from_json("[]").is_err());
        let model = r#"{"id":"openai/test","name":"Test","tool_call":true}"#;
        assert!(CanonicalModelRegistry::from_json(&format!("[{model},{model}]")).is_err());
        assert!(CanonicalModelRegistry::from_json(
            r#"[{"id":"invalid","name":"Test","tool_call":true}]"#
        )
        .is_err());
    }
}
