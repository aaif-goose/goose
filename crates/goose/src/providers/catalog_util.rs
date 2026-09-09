pub use goose_providers::canonical::catalog::{
    ModelCapabilities, ModelTemplate, ProviderCatalogEntry, ProviderFormat,
    ProviderSetupCapabilities, ProviderSetupCatalogEntry, ProviderSetupCategory,
    ProviderSetupField, ProviderSetupFieldOverride, ProviderSetupGroup, ProviderSetupMetadata,
    ProviderSetupMethod, ProviderTemplate,
};
use std::collections::HashSet;

pub async fn get_providers_by_format(format: ProviderFormat) -> Vec<ProviderCatalogEntry> {
    let native_provider_ids = super::init::providers()
        .await
        .into_iter()
        .map(|(metadata, _)| metadata.name)
        .collect::<HashSet<_>>();

    goose_providers::canonical::catalog::get_providers_by_format(format, &native_provider_ids)
}

pub async fn get_setup_catalog_entries() -> Vec<ProviderSetupCatalogEntry> {
    goose_providers::canonical::catalog::get_setup_catalog_entries(
        super::providers()
            .await
            .into_iter()
            .map(|(metadata, _)| metadata),
    )
}

pub fn get_provider_template(provider_id: &str) -> Option<ProviderTemplate> {
    goose_providers::canonical::catalog::get_provider_template(provider_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::ProviderType;

    #[tokio::test]
    async fn test_zai_provider() {
        let zai = crate::providers::get_from_registry("zai")
            .await
            .expect("z.ai should be registered as a declarative provider");
        assert_eq!(zai.provider_type(), ProviderType::Declarative);

        let metadata = zai.metadata();
        assert_eq!(metadata.display_name, "Z.AI");
        assert!(
            !metadata.known_models.is_empty(),
            "z.ai should have known models"
        );
        assert!(
            metadata
                .config_keys
                .iter()
                .any(|key| key.name == "ZHIPU_API_KEY"),
            "z.ai should expose its API key config"
        );

        let setup_entries = get_setup_catalog_entries().await;
        let setup_entry = setup_entries
            .iter()
            .find(|entry| entry.provider_id == "zai")
            .expect("z.ai should be in the setup catalog");
        assert_eq!(setup_entry.setup_method, ProviderSetupMethod::SingleApiKey);

        let template = get_provider_template("zai");
        assert!(template.is_some(), "z.ai should have a template");

        let template = template.unwrap();
        println!("Z.AI template: {} models", template.models.len());
        for model in template.models.iter().take(3) {
            println!(
                "  - {} ({}K context)",
                model.name,
                model.context_limit / 1000
            );
        }
        assert!(
            !template.models.is_empty(),
            "z.ai template should have models"
        );
    }

    #[tokio::test]
    async fn groq_uses_its_canonical_display_name() {
        let groq = crate::providers::get_from_registry("groq").await.unwrap();
        assert_eq!(groq.metadata().display_name, "Groq");
    }

    #[tokio::test]
    async fn setup_catalog_includes_goose_and_curated_fields() {
        let entries = get_setup_catalog_entries().await;

        let goose = entries
            .iter()
            .find(|entry| entry.provider_id == "goose")
            .expect("setup catalog should include synthetic goose");
        assert_eq!(goose.category, ProviderSetupCategory::Agent);
        assert!(!goose.acp);
        assert_eq!(goose.setup_method, ProviderSetupMethod::None);
        assert!(goose.fields.is_empty());

        let cursor = entries
            .iter()
            .find(|entry| entry.provider_id == "cursor-agent")
            .expect("setup catalog should include cursor-agent");
        assert_eq!(cursor.category, ProviderSetupCategory::Agent);
        assert!(!cursor.acp);

        let claude = entries
            .iter()
            .find(|entry| entry.provider_id == "claude-acp")
            .expect("setup catalog should include claude-acp");
        assert_eq!(claude.category, ProviderSetupCategory::Agent);
        assert!(claude.acp);

        let ollama = entries
            .iter()
            .find(|entry| entry.provider_id == "ollama")
            .expect("setup catalog should include ollama");
        assert_eq!(ollama.setup_method, ProviderSetupMethod::ConfigFields);
        assert_eq!(ollama.fields.len(), 1);
        assert_eq!(ollama.fields[0].key, "OLLAMA_HOST");
        assert_eq!(ollama.fields[0].label, "Host");
        assert_eq!(
            ollama.fields[0].default_value.as_deref(),
            Some("http://localhost:11434")
        );

        let databricks = entries
            .iter()
            .find(|entry| entry.provider_id == "databricks")
            .expect("setup catalog should include databricks");
        assert_eq!(
            databricks.setup_method,
            ProviderSetupMethod::HostWithOauthFallback
        );
        assert_eq!(
            databricks
                .fields
                .iter()
                .map(|field| field.key.as_str())
                .collect::<Vec<_>>(),
            ["DATABRICKS_HOST", "DATABRICKS_TOKEN"]
        );

        let huggingface = entries
            .iter()
            .find(|entry| entry.provider_id == "huggingface")
            .expect("setup catalog should include huggingface");
        assert_eq!(huggingface.setup_method, ProviderSetupMethod::SingleApiKey);
        assert_eq!(
            huggingface
                .fields
                .iter()
                .map(|field| field.key.as_str())
                .collect::<Vec<_>>(),
            ["HF_TOKEN"]
        );

        let atomic_chat = entries
            .iter()
            .find(|entry| entry.provider_id == "atomic_chat")
            .expect("setup catalog should include atomic_chat declarative provider");
        assert_eq!(atomic_chat.setup_method, ProviderSetupMethod::ConfigFields);
        let host_field = atomic_chat
            .fields
            .iter()
            .find(|field| field.key == "ATOMIC_CHAT_HOST")
            .expect("atomic_chat should expose ATOMIC_CHAT_HOST");
        assert_eq!(host_field.label, "Host URL");
        assert_eq!(
            host_field.default_value.as_deref(),
            Some("http://localhost:1337")
        );
    }

    #[tokio::test]
    async fn setup_catalog_preserves_curated_providers_and_hides_deprecated_ones() {
        let provider_ids = get_setup_catalog_entries()
            .await
            .into_iter()
            .map(|entry| entry.provider_id)
            .collect::<std::collections::HashSet<_>>();

        for provider_id in ["goose", "claude-acp", "openai"] {
            assert!(provider_ids.contains(provider_id), "missing {provider_id}");
        }

        for (provider_id, replacement) in [
            ("claude-code", "claude-acp"),
            ("codex", "codex-acp"),
            ("gemini-cli", "gemini_oauth"),
        ] {
            assert!(!provider_ids.contains(provider_id));
            let metadata = crate::providers::get_from_registry(provider_id)
                .await
                .unwrap()
                .metadata()
                .clone();
            assert_eq!(
                metadata.deprecated.and_then(|value| value.replacement),
                Some(replacement.to_string())
            );
        }
    }
}
