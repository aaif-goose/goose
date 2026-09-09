use eredu::api::{PreparedChatGenerationSettings, TextModelOptions};
use eredu_core::TextSamplingStrategy;
use eredu_text::tokenizer::ModelChatTemplate;
use goose_provider_types::{errors::ProviderError, model::ModelConfig};

use super::error;
use crate::model::{ChatTemplate, ModelSettings, SamplingConfig};

pub fn generation_settings(
    settings: &ModelSettings,
    request: &ModelConfig,
) -> Result<PreparedChatGenerationSettings, ProviderError> {
    let mut result = PreparedChatGenerationSettings::default();
    let overrides = &mut result.overrides;
    match settings.sampling {
        SamplingConfig::Inherit => {}
        SamplingConfig::Greedy => overrides.do_sample = Some(false),
        SamplingConfig::Temperature {
            temperature,
            top_k,
            top_p,
            min_p,
            seed,
        } => {
            overrides.temperature = temperature;
            overrides.top_k = top_k;
            overrides.top_p = top_p;
            overrides.min_p = min_p;
            result.seed = u64::from(seed.unwrap_or(0));
        }
        SamplingConfig::MirostatV2 {
            temperature,
            tau,
            eta,
            seed,
        } => {
            overrides.temperature = temperature;
            result.strategy = TextSamplingStrategy::MirostatV2 { tau, eta };
            result.seed = u64::from(seed.unwrap_or(0));
        }
    }
    overrides.repetition_penalty = settings.repeat_penalty;
    overrides.repeat_last_n = settings.repeat_last_n;
    overrides.frequency_penalty = settings.frequency_penalty;
    overrides.presence_penalty = settings.presence_penalty;
    overrides.max_new_tokens = settings.max_output_tokens;
    if let Some(params) = &request.request_params {
        macro_rules! override_param {
            ($key:literal, $field:ident) => {
                if let Some(value) = params.get($key).filter(|value| !value.is_null()) {
                    overrides.$field = Some(serde_json::from_value(value.clone()).map_err(error)?);
                }
            };
        }
        override_param!("do_sample", do_sample);
        override_param!("temperature", temperature);
        override_param!("top_k", top_k);
        override_param!("top_p", top_p);
        override_param!("min_p", min_p);
        override_param!("repetition_penalty", repetition_penalty);
        override_param!("repeat_last_n", repeat_last_n);
        override_param!("frequency_penalty", frequency_penalty);
        override_param!("presence_penalty", presence_penalty);
        override_param!("max_new_tokens", max_new_tokens);
        if let Some(seed) = params.get("seed").filter(|value| !value.is_null()) {
            result.seed = serde_json::from_value(seed.clone()).map_err(error)?;
        }
    }
    if let Some(temperature) = request.temperature {
        overrides.temperature = Some(temperature);
    }
    // Request temperature takes precedence over a saved greedy selection.
    if (request.temperature.is_some() || request.request_param::<f32>("temperature").is_some())
        && matches!(settings.sampling, SamplingConfig::Greedy)
        && request.request_param::<bool>("do_sample").is_none()
    {
        overrides.do_sample = None;
    }
    if let Some(max_tokens) = request.max_tokens {
        overrides.max_new_tokens = Some(usize::try_from(max_tokens).map_err(error)?);
    }
    Ok(result)
}

pub fn text_options(template: &ChatTemplate) -> Result<TextModelOptions, ProviderError> {
    let chat_template = match template {
        ChatTemplate::Embedded => None,
        ChatTemplate::CustomInline { template } => {
            Some(ModelChatTemplate::Single(template.clone()))
        }
        ChatTemplate::Builtin { name } => Some(ModelChatTemplate::Single(
            builtin_template(name)?.to_owned(),
        )),
    };
    Ok(TextModelOptions { chat_template })
}

// Builtins are explicit template sources. Eredu owns their rendering and protocol detection.
pub fn builtin_template(name: &str) -> Result<&'static str, ProviderError> {
    match name.trim() {
        "chatml" => Ok("{% for message in messages %}{{ '<|im_start|>' + message['role'] + '\\n' + message['content'] + '<|im_end|>\\n' }}{% endfor %}{% if add_generation_prompt %}{{ '<|im_start|>assistant\\n' }}{% endif %}"),
        _ => Err(error(format!("Eredu has no Goose builtin template '{name}'. Use the embedded checkpoint template or supply its Jinja source as a custom template."))),
    }
}
