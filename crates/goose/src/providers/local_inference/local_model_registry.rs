use crate::config::paths::Paths;
use crate::download_manager::{get_download_manager, DownloadStatus};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type")]
pub enum SamplingConfig {
    Greedy,
    Temperature {
        temperature: f32,
        top_k: i32,
        top_p: f32,
        min_p: f32,
        seed: Option<u32>,
    },
    MirostatV2 {
        tau: f32,
        eta: f32,
        seed: Option<u32>,
    },
}

impl Default for SamplingConfig {
    fn default() -> Self {
        SamplingConfig::Temperature {
            temperature: 0.8,
            top_k: 40,
            top_p: 0.95,
            min_p: 0.05,
            seed: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelSettings {
    pub context_size: Option<u32>,
    pub max_output_tokens: Option<usize>,
    #[serde(default)]
    pub sampling: SamplingConfig,
    #[serde(default = "default_repeat_penalty")]
    pub repeat_penalty: f32,
    #[serde(default = "default_repeat_last_n")]
    pub repeat_last_n: i32,
    #[serde(default)]
    pub frequency_penalty: f32,
    #[serde(default)]
    pub presence_penalty: f32,
    pub n_batch: Option<u32>,
    pub n_gpu_layers: Option<u32>,
    #[serde(default)]
    pub use_mlock: bool,
    pub flash_attention: Option<bool>,
    pub n_threads: Option<i32>,
    #[serde(default)]
    pub native_tool_calling: bool,
    #[serde(default)]
    pub use_jinja: bool,
    #[serde(default = "default_true")]
    pub enable_thinking: bool,
}

fn default_true() -> bool {
    true
}

fn default_repeat_penalty() -> f32 {
    1.0
}

fn default_repeat_last_n() -> i32 {
    64
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            context_size: None,
            max_output_tokens: None,
            sampling: SamplingConfig::default(),
            repeat_penalty: 1.0,
            repeat_last_n: 64,
            frequency_penalty: 0.0,
            presence_penalty: 0.0,
            n_batch: None,
            n_gpu_layers: None,
            use_mlock: false,
            flash_attention: None,
            n_threads: None,
            native_tool_calling: false,
            use_jinja: false,
            enable_thinking: true,
        }
    }
}

pub struct FeaturedModel {
    /// HuggingFace spec in "author/repo-GGUF:quantization" format.
    pub spec: &'static str,
    /// Whether this model's GGUF template supports native tool calling via llama.cpp.
    pub native_tool_calling: bool,
}

pub const FEATURED_MODELS: &[FeaturedModel] = &[
    FeaturedModel {
        spec: "bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M",
        native_tool_calling: false,
    },
    FeaturedModel {
        spec: "bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M",
        native_tool_calling: false,
    },
    FeaturedModel {
        spec: "bartowski/Hermes-2-Pro-Mistral-7B-GGUF:Q4_K_M",
        native_tool_calling: false,
    },
    FeaturedModel {
        spec: "bartowski/Mistral-Small-24B-Instruct-2501-GGUF:Q4_K_M",
        native_tool_calling: false,
    },
    FeaturedModel {
        spec: "unsloth/gemma-4-E4B-it-GGUF:Q4_K_M",
        native_tool_calling: true,
    },
    FeaturedModel {
        spec: "unsloth/gemma-4-26B-A4B-it-GGUF:Q4_K_M",
        native_tool_calling: true,
    },
];

pub fn default_settings_for_model(model_id: &str) -> ModelSettings {
    use super::hf_models::parse_model_spec;
    let model_repo = model_id.split(':').next().unwrap_or(model_id);
    let featured = FEATURED_MODELS.iter().find(|m| {
        if let Ok((repo_id, _quant)) = parse_model_spec(m.spec) {
            repo_id == model_repo
        } else {
            false
        }
    });
    ModelSettings {
        native_tool_calling: featured.is_some_and(|m| m.native_tool_calling),
        ..ModelSettings::default()
    }
}

/// Check if a model ID corresponds to a featured model.
pub fn is_featured_model(model_id: &str) -> bool {
    use super::hf_models::parse_model_spec;
    FEATURED_MODELS.iter().any(|m| {
        if let Ok((repo_id, quant)) = parse_model_spec(m.spec) {
            model_id_from_repo(&repo_id, &quant) == model_id
        } else {
            false
        }
    })
}

static REGISTRY: OnceLock<Mutex<LocalModelRegistry>> = OnceLock::new();

pub fn get_registry() -> &'static Mutex<LocalModelRegistry> {
    REGISTRY.get_or_init(|| {
        let registry = LocalModelRegistry::load().unwrap_or_default();
        Mutex::new(registry)
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModelEntry {
    pub id: String,
    pub repo_id: String,
    pub filename: String,
    pub quantization: String,
    pub local_path: PathBuf,
    pub source_url: String,
    #[serde(default)]
    pub settings: ModelSettings,
    #[serde(default)]
    pub size_bytes: u64,
}

impl LocalModelEntry {
    pub fn is_downloaded(&self) -> bool {
        self.local_path.exists()
    }

    pub fn is_downloading(&self) -> bool {
        let download_id = format!("{}-model", self.id);
        let manager = get_download_manager();
        manager.get_progress(&download_id).is_some()
    }

    pub fn download_status(&self) -> ModelDownloadStatus {
        if self.local_path.exists() {
            return ModelDownloadStatus::Downloaded;
        }

        let download_id = format!("{}-model", self.id);
        let manager = get_download_manager();
        if let Some(progress) = manager.get_progress(&download_id) {
            return match progress.status {
                DownloadStatus::Downloading => ModelDownloadStatus::Downloading {
                    progress_percent: progress.progress_percent,
                    bytes_downloaded: progress.bytes_downloaded,
                    total_bytes: progress.total_bytes,
                    speed_bps: progress.speed_bps.unwrap_or(0),
                },
                DownloadStatus::Completed => ModelDownloadStatus::Downloaded,
                DownloadStatus::Failed | DownloadStatus::Cancelled => {
                    ModelDownloadStatus::NotDownloaded
                }
            };
        }

        ModelDownloadStatus::NotDownloaded
    }

    pub fn file_size(&self) -> u64 {
        if self.size_bytes > 0 {
            return self.size_bytes;
        }
        std::fs::metadata(&self.local_path)
            .map(|m| m.len())
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelDownloadStatus {
    NotDownloaded,
    Downloading {
        progress_percent: f32,
        bytes_downloaded: u64,
        total_bytes: u64,
        speed_bps: u64,
    },
    Downloaded,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalModelRegistry {
    pub models: Vec<LocalModelEntry>,
}

impl LocalModelRegistry {
    fn registry_path() -> PathBuf {
        Paths::in_data_dir("models/registry.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::registry_path();
        if path.exists() {
            let lock_path = path.with_extension("json.lock");
            let lock_file = std::fs::File::create(&lock_path)?;
            fs2::FileExt::lock_shared(&lock_file)?;
            let contents = std::fs::read_to_string(&path)?;
            fs2::FileExt::unlock(&lock_file)?;
            let registry: LocalModelRegistry = serde_json::from_str(&contents)?;
            Ok(registry)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::registry_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let lock_path = path.with_extension("json.lock");
        let lock_file = std::fs::File::create(&lock_path)?;
        fs2::FileExt::lock_exclusive(&lock_file)?;

        let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        let contents = serde_json::to_string_pretty(self)?;
        std::io::Write::write_all(&mut tmp, contents.as_bytes())?;
        tmp.persist(&path)?;

        fs2::FileExt::unlock(&lock_file)?;
        Ok(())
    }

    /// Sync registry with featured models:
    /// add any featured models that are missing, remove non-downloaded non-featured models.
    pub fn sync_with_featured(&mut self, featured_entries: Vec<LocalModelEntry>) {
        let mut changed = false;

        for entry in featured_entries {
            if !self.models.iter().any(|m| m.id == entry.id) {
                self.models.push(entry);
                changed = true;
            }
        }

        let before_len = self.models.len();
        self.models
            .retain(|m| m.is_downloaded() || m.is_downloading() || is_featured_model(&m.id));
        if self.models.len() != before_len {
            changed = true;
        }

        if changed {
            let _ = self.save();
        }
    }

    pub fn add_model(&mut self, entry: LocalModelEntry) -> Result<()> {
        if let Some(existing) = self.models.iter_mut().find(|m| m.id == entry.id) {
            *existing = entry;
        } else {
            self.models.push(entry);
        }
        self.save()
    }

    pub fn remove_model(&mut self, id: &str) -> Result<()> {
        self.models.retain(|m| m.id != id);
        self.save()
    }

    pub fn get_model(&self, id: &str) -> Option<&LocalModelEntry> {
        self.models.iter().find(|m| m.id == id)
    }

    pub fn has_model(&self, id: &str) -> bool {
        self.models.iter().any(|m| m.id == id)
    }

    pub fn get_model_settings(&self, id: &str) -> Option<&ModelSettings> {
        self.models.iter().find(|m| m.id == id).map(|m| &m.settings)
    }

    pub fn update_model_settings(&mut self, id: &str, settings: ModelSettings) -> Result<()> {
        let entry = self
            .models
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", id))?;
        entry.settings = settings;
        self.save()
    }

    pub fn list_models(&self) -> &[LocalModelEntry] {
        &self.models
    }
}

/// Generate a unique ID for a model from its repo_id and quantization.
pub fn model_id_from_repo(repo_id: &str, quantization: &str) -> String {
    format!("{}:{}", repo_id, quantization)
}
