use crate::{config_resolver, model::ModelSettings};
use anyhow::{bail, Result};

pub const EREDU_BACKEND_ID: &str = "eredu";
pub const LLAMACPP_BACKEND_ID: &str = "llamacpp";
pub const GGUF_FORMAT: &str = "gguf";
pub const SAFETENSORS_FORMAT: &str = "safetensors";

pub fn default_backend(format: &str) -> &'static str {
    if format == GGUF_FORMAT {
        LLAMACPP_BACKEND_ID
    } else {
        EREDU_BACKEND_ID
    }
}

pub fn select_backend(
    format: &str,
    model_override: Option<&str>,
    global_override: Option<&str>,
) -> Result<String> {
    let backend = model_override
        .or(global_override)
        .unwrap_or_else(|| default_backend(format));
    match backend {
        EREDU_BACKEND_ID => Ok(backend.into()),
        LLAMACPP_BACKEND_ID if format == GGUF_FORMAT => Ok(backend.into()),
        LLAMACPP_BACKEND_ID => bail!(
            "llama.cpp does not support the {format} format; select Eredu or the format default"
        ),
        _ => bail!("Unknown local inference backend '{backend}'; choose eredu or llamacpp"),
    }
}

pub fn configured_backend(format: &str, settings: &ModelSettings) -> Result<String> {
    let global = config_resolver::string_param("GOOSE_LOCAL_BACKEND")?;
    select_backend(format, settings.backend_id.as_deref(), global.as_deref())
}

pub fn available_backends(format: &str) -> Vec<String> {
    let mut backends = Vec::new();
    if format == GGUF_FORMAT {
        backends.push(LLAMACPP_BACKEND_ID.into());
    }
    if cfg!(all(feature = "mlx", target_os = "macos")) {
        backends.push(EREDU_BACKEND_ID.into());
    }
    backends
}
