use crate::paths::Paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Registry {
    paths: Vec<String>,
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

fn registry_path() -> PathBuf {
    Paths::data_dir().join("local_inference_local_models.json")
}

fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(|| {
        let loaded = std::fs::read(registry_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Mutex::new(loaded)
    })
}

fn save(reg: &Registry) -> Result<()> {
    let path = registry_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(reg)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn register(path: &str) -> Result<String> {
    let canonical =
        std::fs::canonicalize(path).map_err(|_| anyhow::anyhow!("File not found: {}", path))?;
    if canonical.extension().and_then(|e| e.to_str()) != Some("gguf") {
        anyhow::bail!("Only GGUF files are supported");
    }
    let model_id = canonical.to_string_lossy().to_string();
    let mut reg = registry().lock().unwrap();
    if !reg.paths.contains(&model_id) {
        reg.paths.push(model_id.clone());
        save(&reg)?;
    }
    Ok(model_id)
}

pub fn unregister(model_id: &str) -> Result<()> {
    let mut reg = registry().lock().unwrap();
    let before = reg.paths.len();
    reg.paths.retain(|p| p != model_id);
    if reg.paths.len() != before {
        save(&reg)?;
    }
    Ok(())
}

pub fn list() -> Vec<String> {
    registry().lock().unwrap().paths.clone()
}
