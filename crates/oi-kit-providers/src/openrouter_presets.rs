use crate::api_client::ApiClient;
use crate::base::ModelInfo;
use crate::errors::ProviderError;
use crate::http_status::read_json_response;
use crate::openai_compatible::handle_status;
use futures::{stream, StreamExt, TryStreamExt};
use serde::{de::DeserializeOwned, Deserialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const PRESET_PREFIX: &str = "@preset/";
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const SUCCESS_TTL: Duration = Duration::from_secs(300);
const FAILURE_TTL: Duration = Duration::from_secs(60);

pub(crate) struct PresetCatalog {
    client: ApiClient,
    metadata: Mutex<HashMap<String, CachedPreset>>,
}

impl PresetCatalog {
    pub(crate) fn new(client: ApiClient) -> Self {
        Self {
            client,
            metadata: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn list(&self) -> Vec<String> {
        match tokio::time::timeout(DISCOVERY_TIMEOUT, self.list_active()).await {
            Ok(Ok(models)) => models,
            Ok(Err(error)) => {
                tracing::warn!(%error, "Could not list OpenRouter presets");
                Vec::new()
            }
            Err(_) => {
                tracing::warn!("Timed out listing OpenRouter presets");
                Vec::new()
            }
        }
    }

    async fn list_active(&self) -> Result<Vec<String>, ProviderError> {
        let mut models = Vec::new();
        let mut offset = 0;
        loop {
            let page: PresetPage = self
                .get(&format!("api/v1/presets?limit=100&offset={offset}"))
                .await?;
            if page.data.is_empty() {
                break;
            }
            offset += page.data.len();
            models.extend(
                page.data
                    .into_iter()
                    .filter(|preset| preset.status == "active" && valid_slug(&preset.slug))
                    .map(|preset| format!("{PRESET_PREFIX}{}", preset.slug)),
            );
            if offset >= page.total_count {
                break;
            }
        }
        Ok(models)
    }

    pub(crate) async fn model_info(&self, model: &str) -> Option<ModelInfo> {
        let slug = preset_slug(model)?;
        let mut cache = self.metadata.lock().await;
        let now = Instant::now();
        cache.retain(|_, preset| now < preset.expires_at);
        if let Some(cached) = cache.get(slug) {
            return cached.info.clone();
        }
        let metadata = tokio::time::timeout(DISCOVERY_TIMEOUT, self.resolve(model, slug))
            .await
            .ok()
            .and_then(Result::ok)
            .flatten();
        let ttl = if metadata.is_some() {
            SUCCESS_TTL
        } else {
            FAILURE_TTL
        };
        cache.insert(
            slug.to_owned(),
            CachedPreset {
                expires_at: Instant::now() + ttl,
                info: metadata.clone(),
            },
        );
        metadata
    }

    async fn resolve(&self, model: &str, slug: &str) -> Result<Option<ModelInfo>, ProviderError> {
        let response: Data<Preset> = self.get(&format!("api/v1/presets/{slug}")).await?;
        let preset = response.data;
        if preset.status != "active" {
            return Ok(None);
        }
        let Some(version) = preset.designated_version else {
            return Ok(None);
        };
        let config = version.config;
        let routing = config.provider.unwrap_or_default();
        let mut models = HashSet::new();
        for model in config.model.iter().chain(config.models.iter().flatten()) {
            let model = model
                .strip_suffix(":nitro")
                .or_else(|| model.strip_suffix(":floor"))
                .unwrap_or(model);
            if model == "openrouter/auto" || !valid_model_id(model) {
                return Ok(None);
            }
            models.insert(model.to_owned());
        }
        if models.is_empty() {
            return Ok(None);
        }

        let resolved_model = (models.len() == 1)
            .then(|| models.iter().next().cloned())
            .flatten();
        let routes: Vec<ModelEndpoints> = stream::iter(models)
            .map(|model| async move {
                let response: Data<ModelEndpoints> = self
                    .get(&format!("api/v1/models/{model}/endpoints"))
                    .await?;
                Ok::<_, ProviderError>(response.data)
            })
            .buffer_unordered(4)
            .try_collect()
            .await?;

        let mut eligible = Vec::new();
        for model_endpoints in &routes {
            let endpoints = model_endpoints
                .endpoints
                .iter()
                .filter(|endpoint| routing.allows(&endpoint.tag))
                .collect::<Vec<_>>();
            if endpoints.is_empty() {
                return Ok(None);
            }
            eligible.extend(endpoints);
        }

        let mut info = ModelInfo::new(model);
        info.resolved_model = resolved_model;
        info.context_limit = eligible
            .iter()
            .map(|endpoint| endpoint.context_length)
            .min();
        info.reasoning = eligible.iter().all(|endpoint| {
            endpoint.supported_parameters.iter().any(|parameter| {
                matches!(
                    parameter.as_str(),
                    "reasoning" | "include_reasoning" | "reasoning_effort"
                )
            })
        });
        info.supports_vision = Some(routes.iter().all(|route| {
            route
                .architecture
                .input_modalities
                .iter()
                .any(|mode| mode == "image")
        }));
        info.input_token_cost = common_price(&eligible, |price| price.prompt.as_deref());
        info.output_token_cost = common_price(&eligible, |price| price.completion.as_deref());
        info.cache_read_token_cost =
            common_price(&eligible, |price| price.input_cache_read.as_deref());
        info.cache_write_token_cost =
            common_price(&eligible, |price| price.input_cache_write.as_deref());
        info.currency = Some("$".to_owned());
        Ok(Some(info))
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ProviderError> {
        let response = self.client.request(path).response_get().await?;
        read_json_response(handle_status(response).await?).await
    }
}

pub(crate) fn preset_slug(model: &str) -> Option<&str> {
    model
        .strip_prefix(PRESET_PREFIX)
        .filter(|slug| valid_slug(slug))
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_model_id(model: &str) -> bool {
    let Some((author, name)) = model.split_once('/') else {
        return false;
    };
    valid_slug(author)
        && !name.is_empty()
        && name.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn common_price(
    endpoints: &[&Endpoint],
    field: impl Fn(&EndpointPricing) -> Option<&str>,
) -> Option<f64> {
    let price = |endpoint: &Endpoint| {
        field(&endpoint.pricing)?
            .parse::<f64>()
            .ok()
            .filter(|price| price.is_finite() && *price >= 0.0)
    };
    let first = price(endpoints.first()?)?;
    endpoints
        .iter()
        .all(|endpoint| price(endpoint) == Some(first))
        .then_some(first)
}

struct CachedPreset {
    expires_at: Instant,
    info: Option<ModelInfo>,
}

#[derive(Deserialize)]
struct Data<T> {
    data: T,
}

#[derive(Deserialize)]
struct PresetPage {
    data: Vec<PresetSummary>,
    total_count: usize,
}

#[derive(Deserialize)]
struct PresetSummary {
    slug: String,
    status: String,
}

#[derive(Deserialize)]
struct Preset {
    status: String,
    designated_version: Option<PresetVersion>,
}

#[derive(Deserialize)]
struct PresetVersion {
    config: PresetConfig,
}

#[derive(Deserialize)]
struct PresetConfig {
    model: Option<String>,
    models: Option<Vec<String>>,
    provider: Option<ProviderRouting>,
}

#[derive(Default, Deserialize)]
struct ProviderRouting {
    #[serde(default)]
    only: Vec<String>,
    #[serde(default)]
    ignore: Vec<String>,
    #[serde(default)]
    order: Vec<String>,
    allow_fallbacks: Option<bool>,
}

impl ProviderRouting {
    fn allows(&self, tag: &str) -> bool {
        let matches = |filter: &String| {
            tag.eq_ignore_ascii_case(filter)
                || tag
                    .split_once('/')
                    .is_some_and(|(provider, _)| provider.eq_ignore_ascii_case(filter))
        };
        (self.only.is_empty() || self.only.iter().any(matches))
            && !self.ignore.iter().any(matches)
            && (self.allow_fallbacks != Some(false)
                || self.order.is_empty()
                || self.order.iter().any(matches))
    }
}

#[derive(Deserialize)]
struct ModelEndpoints {
    architecture: Architecture,
    endpoints: Vec<Endpoint>,
}

#[derive(Deserialize)]
struct Architecture {
    input_modalities: Vec<String>,
}

#[derive(Deserialize)]
struct Endpoint {
    tag: String,
    context_length: usize,
    #[serde(default)]
    supported_parameters: Vec<String>,
    pricing: EndpointPricing,
}

#[derive(Deserialize)]
struct EndpointPricing {
    prompt: Option<String>,
    completion: Option<String>,
    input_cache_read: Option<String>,
    input_cache_write: Option<String>,
}
