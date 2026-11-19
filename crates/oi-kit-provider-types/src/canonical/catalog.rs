use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::base::{ConfigKey, ProviderMetadata};

use super::CanonicalModelRegistry;

const PROVIDER_METADATA_JSON: &str = include_str!("data/provider_metadata.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderMetadataEntry {
    pub id: String,
    pub display_name: String,
    pub npm: Option<String>,
    pub api: Option<String>,
    pub doc: Option<String>,
    pub env: Vec<String>,
    pub model_count: usize,
}

static PROVIDER_METADATA: Lazy<HashMap<String, ProviderMetadataEntry>> = Lazy::new(|| {
    serde_json::from_str::<Vec<ProviderMetadataEntry>>(PROVIDER_METADATA_JSON)
        .unwrap_or_else(|e| {
            eprintln!("Failed to parse provider metadata: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect()
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderFormat {
    OpenAI,
    Anthropic,
    Ollama,
}

impl ProviderFormat {
    pub fn as_str(&self) -> &str {
        match self {
            ProviderFormat::OpenAI => "openai",
            ProviderFormat::Anthropic => "anthropic",
            ProviderFormat::Ollama => "ollama",
        }
    }
}

impl std::str::FromStr for ProviderFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "openai" | "openai_compatible" => Ok(ProviderFormat::OpenAI),
            "anthropic" | "anthropic_compatible" => Ok(ProviderFormat::Anthropic),
            "ollama" | "ollama_compatible" => Ok(ProviderFormat::Ollama),
            _ => Err(format!("unknown provider format: {}", s)),
        }
    }
}

fn detect_format_from_npm(npm: &str) -> Option<ProviderFormat> {
    if npm.contains("openai") {
        Some(ProviderFormat::OpenAI)
    } else if npm.contains("anthropic") {
        Some(ProviderFormat::Anthropic)
    } else if npm.contains("ollama") {
        Some(ProviderFormat::Ollama)
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderCatalogEntry {
    pub id: String,
    pub name: String,
    pub format: String,
    pub api_url: String,
    pub model_count: usize,
    pub doc_url: String,
    pub env_var: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderTemplate {
    pub id: String,
    pub name: String,
    pub format: String,
    pub api_url: String,
    pub models: Vec<ModelTemplate>,
    pub supports_streaming: bool,
    pub env_var: String,
    pub doc_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelTemplate {
    pub id: String,
    pub name: String,
    pub context_limit: usize,
    pub capabilities: ModelCapabilities,
    pub deprecated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelCapabilities {
    pub tool_call: bool,
    pub reasoning: bool,
    pub attachment: bool,
    pub temperature: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupCategory {
    Agent,
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupMethod {
    None,
    SingleApiKey,
    ConfigFields,
    HostWithOauthFallback,
    OauthBrowser,
    OauthDeviceCode,
    CloudCredentials,
    Local,
    CliAuth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupGroup {
    Default,
    Additional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSetupField {
    pub key: String,
    pub label: String,
    pub secret: bool,
    pub required: bool,
    pub placeholder: Option<String>,
    pub default_value: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct ProviderSetupCapabilities {
    pub install: bool,
    pub auth: bool,
    pub auth_status: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSetupCatalogEntry {
    pub provider_id: String,
    pub display_name: String,
    pub category: ProviderSetupCategory,
    pub acp: bool,
    pub description: String,
    pub setup_method: ProviderSetupMethod,
    pub docs_url: Option<String>,
    pub group: ProviderSetupGroup,
    pub fields: Vec<ProviderSetupField>,
    pub aliases: Vec<String>,
    pub native_connect_query: Option<String>,
    pub binary_name: Option<String>,
    #[serde(default)]
    pub setup_capabilities: ProviderSetupCapabilities,
    pub show_only_when_installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSetupMetadata {
    pub category: ProviderSetupCategory,
    #[serde(default)]
    pub acp: bool,
    pub setup_method: ProviderSetupMethod,
    pub group: ProviderSetupGroup,
    #[serde(default)]
    pub docs_url: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub native_connect_query: Option<String>,
    #[serde(default)]
    pub binary_name: Option<String>,
    #[serde(default)]
    pub setup_capabilities: ProviderSetupCapabilities,
    #[serde(default)]
    pub show_only_when_installed: bool,
    #[serde(default)]
    pub field_overrides: Vec<ProviderSetupFieldOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSetupFieldOverride {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub default_value: Option<String>,
}

impl ProviderSetupMetadata {
    pub fn new(
        category: ProviderSetupCategory,
        setup_method: ProviderSetupMethod,
        group: ProviderSetupGroup,
    ) -> Self {
        Self {
            category,
            acp: false,
            setup_method,
            group,
            docs_url: None,
            aliases: Vec::new(),
            native_connect_query: None,
            binary_name: None,
            setup_capabilities: ProviderSetupCapabilities {
                install: false,
                auth: false,
                auth_status: false,
            },
            show_only_when_installed: false,
            field_overrides: Vec::new(),
        }
    }

    pub fn cli_agent(binary_name: &str, aliases: &[&str]) -> Self {
        let mut setup = Self::new(
            ProviderSetupCategory::Agent,
            ProviderSetupMethod::CliAuth,
            ProviderSetupGroup::Default,
        );
        setup.binary_name = Some(binary_name.to_string());
        setup.aliases = aliases.iter().map(|alias| alias.to_string()).collect();
        setup
    }

    pub fn api_key(group: ProviderSetupGroup) -> Self {
        Self::new(
            ProviderSetupCategory::Model,
            ProviderSetupMethod::SingleApiKey,
            group,
        )
    }

    pub fn with_docs_url(mut self, docs_url: &str) -> Self {
        self.docs_url = Some(docs_url.to_string());
        self
    }

    pub fn with_aliases(mut self, aliases: &[&str]) -> Self {
        self.aliases = aliases.iter().map(|alias| alias.to_string()).collect();
        self
    }

    pub fn with_native_connect_query(mut self, query: &str) -> Self {
        self.native_connect_query = Some(query.to_string());
        self
    }

    pub fn with_capabilities(mut self, install: bool, auth: bool, auth_status: bool) -> Self {
        self.setup_capabilities = ProviderSetupCapabilities {
            install,
            auth,
            auth_status,
        };
        self
    }

    pub fn with_acp(mut self) -> Self {
        self.acp = true;
        self
    }

    pub fn show_only_when_installed(mut self) -> Self {
        self.show_only_when_installed = true;
        self
    }

    pub fn with_field(
        mut self,
        key: &str,
        label: &str,
        placeholder: Option<&str>,
        default_value: Option<&str>,
    ) -> Self {
        self.field_overrides.push(ProviderSetupFieldOverride {
            key: key.to_string(),
            label: label.to_string(),
            placeholder: placeholder.map(str::to_string),
            default_value: default_value.map(str::to_string),
        });
        self
    }
}

fn field_label(key: &str) -> String {
    let label = key
        .strip_prefix("GOOSE_")
        .unwrap_or(key)
        .replace('_', " ")
        .to_lowercase();
    label
        .split_whitespace()
        .map(|word| {
            if matches!(
                word,
                "api" | "url" | "id" | "openai" | "aws" | "gcp" | "llm" | "oauth"
            ) {
                word.to_uppercase()
            } else {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn setup_field(config_key: &ConfigKey, setup: &ProviderSetupMetadata) -> ProviderSetupField {
    let field_override = setup
        .field_overrides
        .iter()
        .find(|field| field.key == config_key.name);
    ProviderSetupField {
        key: config_key.name.clone(),
        label: field_override
            .map(|field| field.label.clone())
            .unwrap_or_else(|| {
                if config_key.secret && setup.setup_method == ProviderSetupMethod::SingleApiKey {
                    "API Key".to_string()
                } else {
                    field_label(&config_key.name)
                }
            }),
        secret: config_key.secret,
        required: config_key.required,
        placeholder: field_override
            .and_then(|field| field.placeholder.clone())
            .or_else(|| {
                (config_key.secret && setup.setup_method == ProviderSetupMethod::SingleApiKey)
                    .then(|| "Paste your API key".to_string())
            }),
        default_value: field_override
            .and_then(|field| field.default_value.clone())
            .or_else(|| config_key.default.clone()),
    }
}

fn setup_entry_from_metadata(metadata: ProviderMetadata) -> Option<ProviderSetupCatalogEntry> {
    let setup = metadata.setup?;
    let fields = metadata
        .config_keys
        .iter()
        .filter(|key| key.primary)
        .map(|key| setup_field(key, &setup))
        .collect();
    Some(ProviderSetupCatalogEntry {
        provider_id: metadata.name,
        display_name: metadata.display_name,
        category: setup.category,
        acp: setup.acp,
        description: metadata.description,
        setup_method: setup.setup_method,
        docs_url: setup
            .docs_url
            .or_else(|| (!metadata.model_doc_link.is_empty()).then_some(metadata.model_doc_link)),
        group: setup.group,
        fields,
        aliases: setup.aliases,
        native_connect_query: setup.native_connect_query,
        binary_name: setup.binary_name,
        setup_capabilities: setup.setup_capabilities,
        show_only_when_installed: setup.show_only_when_installed,
    })
}

pub fn get_providers_by_format(
    format: ProviderFormat,
    native_provider_ids: &HashSet<String>,
) -> Vec<ProviderCatalogEntry> {
    let mut entries: Vec<ProviderCatalogEntry> = PROVIDER_METADATA
        .values()
        .filter_map(|metadata| {
            if native_provider_ids.contains(&metadata.id) {
                return None;
            }

            let npm = metadata.npm.as_ref()?;
            let detected_format = detect_format_from_npm(npm)?;

            if detected_format != format {
                return None;
            }

            let api_url = metadata.api.as_ref()?.clone();

            let env_var = metadata.env.first().cloned().unwrap_or_else(|| {
                format!("{}_API_KEY", metadata.id.to_uppercase().replace('-', "_"))
            });

            Some(ProviderCatalogEntry {
                id: metadata.id.clone(),
                name: metadata.display_name.clone(),
                format: detected_format.as_str().to_string(),
                api_url,
                model_count: metadata.model_count,
                doc_url: metadata.doc.clone().unwrap_or_default(),
                env_var,
            })
        })
        .collect();

    // Sort by name
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

pub fn get_setup_catalog_entries(
    registry_metadata: impl IntoIterator<Item = ProviderMetadata>,
) -> Vec<ProviderSetupCatalogEntry> {
    let goose = ProviderSetupCatalogEntry {
        provider_id: "goose".to_string(),
        display_name: "Goose".to_string(),
        category: ProviderSetupCategory::Agent,
        acp: false,
        description: "Block's open-source coding agent".to_string(),
        setup_method: ProviderSetupMethod::None,
        docs_url: None,
        group: ProviderSetupGroup::Default,
        fields: Vec::new(),
        aliases: vec!["goose".to_string()],
        native_connect_query: None,
        binary_name: None,
        setup_capabilities: ProviderSetupCapabilities {
            install: false,
            auth: false,
            auth_status: false,
        },
        show_only_when_installed: false,
    };
    let mut entries = registry_metadata
        .into_iter()
        .filter_map(setup_entry_from_metadata)
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        setup_group_rank(a.group)
            .cmp(&setup_group_rank(b.group))
            .then_with(|| setup_category_rank(a.category).cmp(&setup_category_rank(b.category)))
            .then_with(|| a.display_name.cmp(&b.display_name))
            .then_with(|| a.provider_id.cmp(&b.provider_id))
    });
    entries.insert(0, goose);
    entries
}

fn setup_group_rank(group: ProviderSetupGroup) -> u8 {
    match group {
        ProviderSetupGroup::Default => 0,
        ProviderSetupGroup::Additional => 1,
    }
}

fn setup_category_rank(category: ProviderSetupCategory) -> u8 {
    match category {
        ProviderSetupCategory::Agent => 0,
        ProviderSetupCategory::Model => 1,
    }
}

pub fn get_provider_template(provider_id: &str) -> Option<ProviderTemplate> {
    let metadata = PROVIDER_METADATA.get(provider_id)?;

    let npm = metadata.npm.as_ref()?;
    let format = detect_format_from_npm(npm)?;

    let api_url = metadata.api.as_ref()?.clone();

    let models: Vec<ModelTemplate> = CanonicalModelRegistry::bundled()
        .ok()
        .map(|registry| {
            registry
                .get_all_models_for_provider(provider_id)
                .into_iter()
                .map(|model| {
                    // Extract just the model ID (without provider prefix)
                    let model_id = model
                        .id
                        .strip_prefix(&format!("{}/", provider_id))
                        .unwrap_or(&model.id)
                        .to_string();

                    ModelTemplate {
                        id: model_id,
                        name: model.name.clone(),
                        context_limit: model.limit.context,
                        capabilities: ModelCapabilities {
                            tool_call: model.tool_call,
                            reasoning: model.reasoning.unwrap_or(false),
                            attachment: model.attachment.unwrap_or(false),
                            temperature: model.temperature.unwrap_or(false),
                        },
                        deprecated: false, // Canonical models don't have deprecated flag
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let env_var = metadata
        .env
        .first()
        .cloned()
        .unwrap_or_else(|| format!("{}_API_KEY", provider_id.to_uppercase().replace('-', "_")));

    Some(ProviderTemplate {
        id: metadata.id.clone(),
        name: metadata.display_name.clone(),
        format: format.as_str().to_string(),
        api_url,
        models,
        supports_streaming: true, // Default to true
        env_var,
        doc_url: metadata.doc.clone().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_provider(
        name: &str,
        display_name: &str,
        category: ProviderSetupCategory,
        group: ProviderSetupGroup,
    ) -> ProviderMetadata {
        ProviderMetadata::new(name, display_name, "", "", vec![], "", vec![]).with_setup(
            ProviderSetupMetadata::new(category, ProviderSetupMethod::ConfigFields, group),
        )
    }

    #[test]
    fn setup_catalog_has_stable_presentation_order() {
        let entries = get_setup_catalog_entries(vec![
            setup_provider(
                "gamma",
                "Gamma",
                ProviderSetupCategory::Model,
                ProviderSetupGroup::Additional,
            ),
            setup_provider(
                "beta",
                "Beta",
                ProviderSetupCategory::Model,
                ProviderSetupGroup::Default,
            ),
            setup_provider(
                "alpha",
                "Alpha",
                ProviderSetupCategory::Agent,
                ProviderSetupGroup::Default,
            ),
        ]);

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.provider_id.as_str())
                .collect::<Vec<_>>(),
            ["goose", "alpha", "beta", "gamma"]
        );
    }

    #[test]
    fn single_api_key_uses_api_key_presentation() {
        let metadata = ProviderMetadata::new(
            "example",
            "Example",
            "",
            "",
            vec![],
            "",
            vec![ConfigKey::new("EXAMPLE_TOKEN", true, true, None, true)],
        )
        .with_setup(ProviderSetupMetadata::api_key(ProviderSetupGroup::Default));

        let entry = setup_entry_from_metadata(metadata).unwrap();
        assert_eq!(entry.fields[0].label, "API Key");
        assert_eq!(
            entry.fields[0].placeholder.as_deref(),
            Some("Paste your API key")
        );
    }

    #[test]
    fn other_secret_fields_do_not_claim_to_be_api_keys() {
        let metadata = ProviderMetadata::new(
            "example",
            "Example",
            "",
            "",
            vec![],
            "",
            vec![ConfigKey::new("EXAMPLE_TOKEN", true, true, None, true)],
        )
        .with_setup(ProviderSetupMetadata::new(
            ProviderSetupCategory::Model,
            ProviderSetupMethod::ConfigFields,
            ProviderSetupGroup::Default,
        ));

        let entry = setup_entry_from_metadata(metadata).unwrap();
        assert_eq!(entry.fields[0].label, "Example Token");
        assert_eq!(entry.fields[0].placeholder, None);
    }
}
