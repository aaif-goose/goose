use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SamplingConfig {
    #[default]
    Inherit,
    Greedy,
    Temperature {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        top_k: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        top_p: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_p: Option<f32>,
        seed: Option<u32>,
    },
    MirostatV2 {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
        tau: f32,
        eta: f32,
        seed: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallingMode {
    #[default]
    Auto,
    ForceNative,
    ForceEmulated,
}

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatTemplate {
    #[serde(alias = "auto")]
    #[default]
    Embedded,
    Builtin {
        name: String,
    },
    CustomInline {
        template: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cached_shards: Option<usize>,
    pub context_size: Option<u32>,
    pub max_output_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_model: Option<String>,
    #[serde(default)]
    pub sampling: SamplingConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_last_n: Option<i32>,
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
    #[serde(default)]
    pub presence_penalty: Option<f32>,
    pub n_batch: Option<u32>,
    pub n_gpu_layers: Option<u32>,
    #[serde(default)]
    pub use_mlock: bool,
    pub flash_attention: Option<bool>,
    pub n_threads: Option<i32>,
    #[serde(default)]
    pub tool_calling: ToolCallingMode,
    #[serde(default)]
    pub chat_template: ChatTemplate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_thinking: Option<bool>,
    #[serde(default)]
    pub vision_capable: bool,
    #[serde(default = "default_image_token_estimate")]
    pub image_token_estimate: usize,
    #[serde(default)]
    pub mmproj_size_bytes: u64,
}

fn default_image_token_estimate() -> usize {
    256
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            backend_id: None,
            device: None,
            max_cached_shards: None,
            context_size: None,
            max_output_tokens: None,
            draft_model: None,
            sampling: SamplingConfig::default(),
            repeat_penalty: None,
            repeat_last_n: None,
            frequency_penalty: None,
            presence_penalty: None,
            n_batch: None,
            n_gpu_layers: None,
            use_mlock: false,
            flash_attention: None,
            n_threads: None,
            tool_calling: ToolCallingMode::Auto,
            chat_template: ChatTemplate::Embedded,
            enable_thinking: None,
            vision_capable: false,
            image_token_estimate: default_image_token_estimate(),
            mmproj_size_bytes: 0,
        }
    }
}
