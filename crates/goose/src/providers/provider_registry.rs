use super::api_client::TlsConfig;
use super::backend::ProviderBackend;
use super::base::{
    AcpProviderDef, ConfigKey, ModelInfo, Provider, ProviderDef, ProviderMetadata, ProviderType,
    SessionBoundProviderDef,
};
use super::inventory::{InventoryIdentityInput, InventoryRegistration, InventoryResolvers};
use crate::config::{DeclarativeProviderConfig, ExtensionConfig};
use anyhow::Result;
use futures::future::BoxFuture;
use goose_providers::model::ModelConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub type ProviderConstructor = Arc<
    dyn Fn(
            Vec<ExtensionConfig>,
            Option<PathBuf>,
            Option<TlsConfig>,
            bool,
        ) -> BoxFuture<'static, Result<ProviderBackend>>
        + Send
        + Sync,
>;

pub type ProviderCleanup = Arc<dyn Fn() -> BoxFuture<'static, Result<()>> + Send + Sync>;

#[derive(Clone)]
pub struct ProviderEntry {
    metadata: ProviderMetadata,
    pub(crate) constructor: ProviderConstructor,
    pub(crate) inventory_identity: super::inventory::InventoryIdentityResolver,
    pub(crate) inventory_configured: super::inventory::InventoryConfiguredResolver,
    pub(crate) cleanup: Option<ProviderCleanup>,
    provider_type: ProviderType,
    supports_inventory_refresh: bool,
    tls_config: Option<TlsConfig>,
    toolshim: bool,
    session_bound: bool,
}

impl ProviderEntry {
    pub fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    pub fn provider_type(&self) -> ProviderType {
        self.provider_type
    }

    pub fn session_bound(&self) -> bool {
        self.session_bound
    }

    pub fn supports_inventory_refresh(&self) -> bool {
        self.supports_inventory_refresh
    }

    pub fn inventory_identity(&self) -> Result<InventoryIdentityInput> {
        (self.inventory_identity)()
    }

    pub fn inventory_configured(&self) -> bool {
        (self.inventory_configured)()
    }

    pub(crate) fn toolshim_enabled(&self, fallback: bool) -> bool {
        self.toolshim || fallback
    }

    pub fn normalize_model_config(&self, mut model: ModelConfig) -> Result<ModelConfig> {
        if self.toolshim_enabled(model.toolshim) {
            model = model.with_toolshim(true);
        }
        crate::model_config::materialize_model_config(&self.metadata.name, model)
    }

    /// Transitional compatibility API. Prefer `create_backend_with_default_model`.
    pub async fn create_with_default_model(
        &self,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<Arc<dyn Provider>> {
        Ok(self
            .create_backend_with_default_model(extensions)
            .await?
            .into_legacy_provider())
    }

    /// Transitional compatibility API. Prefer `create_backend`.
    pub async fn create(&self, extensions: Vec<ExtensionConfig>) -> Result<Arc<dyn Provider>> {
        Ok(self
            .create_backend(extensions)
            .await?
            .into_legacy_provider())
    }

    /// Transitional compatibility API. Prefer `create_backend_with_working_dir`.
    pub async fn create_with_working_dir(
        &self,
        extensions: Vec<ExtensionConfig>,
        working_dir: PathBuf,
    ) -> Result<Arc<dyn Provider>> {
        Ok(self
            .create_backend_with_working_dir(extensions, working_dir)
            .await?
            .into_legacy_provider())
    }

    pub async fn create_backend_with_default_model(
        &self,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<ProviderBackend> {
        (self.constructor)(extensions, None, self.tls_config.clone(), true).await
    }

    pub async fn create_backend(
        &self,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<ProviderBackend> {
        (self.constructor)(extensions, None, self.tls_config.clone(), false).await
    }

    pub async fn create_backend_with_working_dir(
        &self,
        extensions: Vec<ExtensionConfig>,
        working_dir: PathBuf,
    ) -> Result<ProviderBackend> {
        (self.constructor)(
            extensions,
            Some(working_dir),
            self.tls_config.clone(),
            false,
        )
        .await
    }
}

#[derive(Default)]
pub struct ProviderRegistry {
    pub(crate) entries: HashMap<String, ProviderEntry>,
    tls_config: Option<TlsConfig>,
}

impl ProviderRegistry {
    pub fn new(tls_config: Option<TlsConfig>) -> Self {
        Self {
            entries: HashMap::new(),
            tls_config,
        }
    }

    pub fn register<F>(&mut self, preferred: bool)
    where
        F: ProviderDef + 'static,
    {
        self.register_with_inventory::<F>(preferred, None);
    }

    pub fn register_with_inventory<F>(
        &mut self,
        preferred: bool,
        inventory_registration: Option<InventoryRegistration>,
    ) where
        F: ProviderDef + 'static,
    {
        let metadata = F::metadata();
        let name = metadata.name.clone();

        let inventory = InventoryResolvers::for_metadata(&metadata, inventory_registration);

        self.entries.insert(
            name,
            ProviderEntry {
                metadata,
                constructor: Arc::new(
                    |_extensions, _working_dir, tls_config, _use_default_model| {
                        Box::pin(async move {
                            Ok(ProviderBackend::Standard(Arc::new(
                                F::from_env(tls_config).await?,
                            )))
                        })
                    },
                ),
                inventory_identity: inventory.identity,
                inventory_configured: inventory.configured,
                cleanup: None,
                provider_type: if preferred {
                    ProviderType::Preferred
                } else {
                    ProviderType::Builtin
                },
                supports_inventory_refresh: inventory.supports_refresh,
                tls_config: self.tls_config.clone(),
                toolshim: false,
                session_bound: false,
            },
        );
    }

    pub fn register_session_bound<F>(&mut self, preferred: bool)
    where
        F: SessionBoundProviderDef + 'static,
    {
        let metadata = F::metadata();
        let name = metadata.name.clone();
        let inventory = InventoryResolvers::for_metadata(&metadata, None);

        self.entries.insert(
            name,
            ProviderEntry {
                metadata,
                constructor: Arc::new(
                    |extensions, _working_dir, tls_config, _use_default_model| {
                        Box::pin(async move {
                            Ok(ProviderBackend::Standard(Arc::new(
                                F::from_env(extensions, tls_config).await?,
                            )))
                        })
                    },
                ),
                inventory_identity: inventory.identity,
                inventory_configured: inventory.configured,
                cleanup: None,
                provider_type: if preferred {
                    ProviderType::Preferred
                } else {
                    ProviderType::Builtin
                },
                supports_inventory_refresh: inventory.supports_refresh,
                tls_config: self.tls_config.clone(),
                toolshim: false,
                session_bound: true,
            },
        );
    }

    pub fn register_acp_with_inventory<F>(
        &mut self,
        preferred: bool,
        inventory_registration: Option<InventoryRegistration>,
    ) where
        F: AcpProviderDef + 'static,
    {
        let metadata = F::metadata();
        let name = metadata.name.clone();

        let inventory = InventoryResolvers::for_metadata(&metadata, inventory_registration);

        self.entries.insert(
            name,
            ProviderEntry {
                metadata,
                constructor: Arc::new(|extensions, working_dir, tls_config, use_default_model| {
                    Box::pin(async move {
                        let provider = if use_default_model {
                            F::from_env_with_default_model(extensions, tls_config).await?
                        } else if let Some(working_dir) = working_dir {
                            F::from_env_with_working_dir(extensions, working_dir, tls_config)
                                .await?
                        } else {
                            F::from_env(extensions, tls_config).await?
                        };
                        Ok(ProviderBackend::Acp(Arc::new(provider)))
                    })
                }),
                inventory_identity: inventory.identity,
                inventory_configured: inventory.configured,
                cleanup: None,
                provider_type: if preferred {
                    ProviderType::Preferred
                } else {
                    ProviderType::Builtin
                },
                supports_inventory_refresh: inventory.supports_refresh,
                tls_config: self.tls_config.clone(),
                toolshim: false,
                session_bound: true,
            },
        );
    }

    pub fn register_with_name<P, F, G>(
        &mut self,
        config: &DeclarativeProviderConfig,
        provider_type: ProviderType,
        supports_inventory_refresh: bool,
        constructor: F,
        inventory_identity: G,
    ) where
        P: ProviderDef + 'static,
        F: Fn(Option<TlsConfig>) -> Result<P::Provider> + Send + Sync + 'static,
        G: Fn() -> Result<InventoryIdentityInput> + Send + Sync + 'static,
    {
        self.register_with_name_impl::<P, F, G>(
            config,
            provider_type,
            supports_inventory_refresh,
            constructor,
            inventory_identity,
            None,
        );
    }

    pub fn register_with_name_and_inventory_configured<P, F, G, H>(
        &mut self,
        config: &DeclarativeProviderConfig,
        provider_type: ProviderType,
        supports_inventory_refresh: bool,
        constructor: F,
        inventory_identity: G,
        inventory_configured: H,
    ) where
        P: ProviderDef + 'static,
        F: Fn(Option<TlsConfig>) -> Result<P::Provider> + Send + Sync + 'static,
        G: Fn() -> Result<InventoryIdentityInput> + Send + Sync + 'static,
        H: Fn() -> bool + Send + Sync + 'static,
    {
        self.register_with_name_impl::<P, F, G>(
            config,
            provider_type,
            supports_inventory_refresh,
            constructor,
            inventory_identity,
            Some(Arc::new(inventory_configured)),
        );
    }

    fn register_with_name_impl<P, F, G>(
        &mut self,
        config: &DeclarativeProviderConfig,
        provider_type: ProviderType,
        supports_inventory_refresh: bool,
        constructor: F,
        inventory_identity: G,
        inventory_configured: Option<super::inventory::InventoryConfiguredResolver>,
    ) where
        P: ProviderDef + 'static,
        F: Fn(Option<TlsConfig>) -> Result<P::Provider> + Send + Sync + 'static,
        G: Fn() -> Result<InventoryIdentityInput> + Send + Sync + 'static,
    {
        let base_metadata = P::metadata();
        let description = config
            .description
            .clone()
            .unwrap_or_else(|| format!("Custom {} provider", config.display_name));
        let default_model = config
            .models
            .first()
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let known_models: Vec<ModelInfo> = config
            .models
            .iter()
            .map(|m| ModelInfo {
                resolved_model: None,
                supports_cache_control: Some(m.supports_cache_control.unwrap_or(false)),
                ..m.clone()
            })
            .collect();

        let mut config_keys = if provider_type == ProviderType::Declarative {
            if !config.api_key_env.is_empty() {
                vec![ConfigKey::new(
                    &config.api_key_env,
                    config.requires_auth,
                    true,
                    None,
                    true,
                )]
            } else {
                Vec::new()
            }
        } else {
            let mut config_keys = base_metadata.config_keys.clone();

            if let Some(api_key_index) = config_keys.iter().position(|key| key.secret) {
                if !config.requires_auth || config.auth.is_some() {
                    config_keys.remove(api_key_index);
                } else if !config.api_key_env.is_empty() {
                    config_keys[api_key_index] =
                        ConfigKey::new(&config.api_key_env, false, true, None, true);
                }
            }

            config_keys
        };

        if let Some(ref env_vars) = config.env_vars {
            for ev in env_vars {
                // Default primary to `required` so required fields show prominently in the UI
                let primary = ev.primary.unwrap_or(ev.required);
                config_keys.push(ConfigKey::new(
                    &ev.name,
                    ev.required,
                    ev.secret,
                    ev.default.as_deref(),
                    primary,
                ));
            }
        }

        let custom_metadata = ProviderMetadata {
            name: config.name.clone(),
            display_name: config.display_name.clone(),
            description,
            default_model,
            known_models,
            model_doc_link: config
                .model_doc_link
                .clone()
                .unwrap_or(base_metadata.model_doc_link),
            config_keys,
            setup_steps: config.setup_steps.clone(),
            setup: config.setup.clone(),
            deprecated: None,
        };
        let inventory_config_keys = custom_metadata.config_keys.clone();
        let default_inventory_configured = Arc::new(move || {
            super::inventory::default_inventory_configured(
                &inventory_config_keys,
                crate::config::Config::global(),
            )
        });

        self.entries.insert(
            config.name.clone(),
            ProviderEntry {
                metadata: custom_metadata,
                constructor: Arc::new(move |_extensions, _working_dir, tls_config, _| {
                    let result = constructor(tls_config);
                    Box::pin(async move {
                        let provider = result?;
                        Ok(ProviderBackend::Standard(Arc::new(provider)))
                    })
                }),
                inventory_identity: Arc::new(inventory_identity),
                inventory_configured: inventory_configured.unwrap_or(default_inventory_configured),
                cleanup: None,
                provider_type,
                supports_inventory_refresh,
                tls_config: self.tls_config.clone(),
                toolshim: config.toolshim,
                session_bound: false,
            },
        );
    }

    pub fn set_cleanup(&mut self, name: &str, cleanup: ProviderCleanup) {
        if let Some(entry) = self.entries.get_mut(name) {
            entry.cleanup = Some(cleanup);
        }
    }

    pub fn with_providers<F>(mut self, setup: F) -> Self
    where
        F: FnOnce(&mut Self),
    {
        setup(&mut self);
        self
    }

    /// Transitional compatibility API. Prefer `create_backend`.
    pub async fn create(
        &self,
        name: &str,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<Arc<dyn Provider>> {
        Ok(self
            .create_backend(name, extensions)
            .await?
            .into_legacy_provider())
    }

    pub async fn create_backend(
        &self,
        name: &str,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<ProviderBackend> {
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", name))?;

        entry.create_backend(extensions).await
    }

    pub fn all_metadata_with_types(&self) -> Vec<(ProviderMetadata, ProviderType)> {
        self.entries
            .values()
            .map(|e| (e.metadata.clone(), e.provider_type))
            .collect()
    }

    pub fn remove_custom_providers(&mut self) {
        self.entries.retain(|name, _| !name.starts_with("custom_"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::declarative_providers::ProviderEngine;
    use crate::providers::openai_def::OpenAiProviderDef;

    struct TestStandardDef;
    struct TestSessionBoundDef;

    impl super::super::base::ProviderDescriptor for TestStandardDef {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new(
                "test-standard",
                "Test",
                "Test",
                "default",
                vec![],
                "",
                vec![],
            )
        }
    }

    impl super::super::base::ProviderDescriptor for TestSessionBoundDef {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new(
                "test-session",
                "Test",
                "Test",
                "default",
                vec![],
                "",
                vec![],
            )
        }
    }

    impl ProviderDef for TestStandardDef {
        type Provider = crate::providers::testprovider::TestProvider;

        fn from_env(_tls_config: Option<TlsConfig>) -> BoxFuture<'static, Result<Self::Provider>> {
            Box::pin(async { anyhow::bail!("standard") })
        }
    }

    impl SessionBoundProviderDef for TestSessionBoundDef {
        type Provider = crate::providers::testprovider::TestProvider;

        fn from_env(
            extensions: Vec<ExtensionConfig>,
            _tls_config: Option<TlsConfig>,
        ) -> BoxFuture<'static, Result<Self::Provider>> {
            Box::pin(async move {
                assert_eq!(extensions.len(), 1);
                assert_eq!(extensions[0].name(), "developer");
                anyhow::bail!("session")
            })
        }
    }

    fn test_extensions() -> Vec<ExtensionConfig> {
        vec![ExtensionConfig::Builtin {
            name: "developer".to_string(),
            description: String::new(),
            display_name: None,
            timeout: None,
            bundled: None,
            available_tools: vec![],
        }]
    }

    #[tokio::test]
    async fn standard_and_session_bound_registration_preserve_construction_behavior() {
        let mut registry = ProviderRegistry::new(None);
        registry.register::<TestStandardDef>(true);
        registry.register_session_bound::<TestSessionBoundDef>(false);

        for (name, session_bound, expected) in [
            ("test-standard", false, "standard"),
            ("test-session", true, "session"),
        ] {
            let entry = &registry.entries[name];
            assert_eq!(entry.session_bound(), session_bound);
            assert_eq!(
                entry
                    .create(test_extensions())
                    .await
                    .err()
                    .unwrap()
                    .to_string(),
                expected
            );
            assert_eq!(
                entry
                    .create_with_working_dir(test_extensions(), PathBuf::from("test-workspace"))
                    .await
                    .err()
                    .unwrap()
                    .to_string(),
                expected
            );
            assert_eq!(
                entry
                    .create_with_default_model(test_extensions())
                    .await
                    .err()
                    .unwrap()
                    .to_string(),
                expected
            );
        }
        assert_eq!(
            registry.entries["test-standard"].provider_type(),
            ProviderType::Preferred
        );
        assert_eq!(
            registry.entries["test-session"].provider_type(),
            ProviderType::Builtin
        );
    }

    struct TestAcpDef;

    impl super::super::base::ProviderDescriptor for TestAcpDef {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new(
                "test-acp",
                "Test ACP",
                "Test",
                "default",
                vec![],
                "",
                vec![],
            )
        }
    }

    impl AcpProviderDef for TestAcpDef {
        fn from_env(
            extensions: Vec<ExtensionConfig>,
            _tls_config: Option<TlsConfig>,
        ) -> BoxFuture<'static, Result<crate::acp::AcpProvider>> {
            Box::pin(async move {
                assert!(extensions.is_empty());
                anyhow::bail!("from_env")
            })
        }

        fn from_env_with_working_dir(
            extensions: Vec<ExtensionConfig>,
            working_dir: PathBuf,
            _tls_config: Option<TlsConfig>,
        ) -> BoxFuture<'static, Result<crate::acp::AcpProvider>> {
            Box::pin(async move {
                assert!(extensions.is_empty());
                assert_eq!(working_dir, PathBuf::from("test-workspace"));
                anyhow::bail!("working_dir")
            })
        }

        fn from_env_with_default_model(
            extensions: Vec<ExtensionConfig>,
            _tls_config: Option<TlsConfig>,
        ) -> BoxFuture<'static, Result<crate::acp::AcpProvider>> {
            Box::pin(async move {
                assert!(extensions.is_empty());
                anyhow::bail!("default_model")
            })
        }
    }

    #[tokio::test]
    async fn acp_registration_preserves_inventory_and_constructor_dispatch() {
        let mut registry = ProviderRegistry::new(None);
        registry.register_acp_with_inventory::<TestAcpDef>(
            true,
            Some(
                InventoryRegistration::new(true, || {
                    Ok(InventoryIdentityInput::new("test-acp", "test-family"))
                })
                .with_configured(|| false),
            ),
        );
        let entry = &registry.entries["test-acp"];
        assert_eq!(entry.metadata().name, "test-acp");
        assert!(entry.session_bound());
        assert_eq!(entry.provider_type(), ProviderType::Preferred);
        assert!(entry.supports_inventory_refresh());
        assert!(!entry.inventory_configured());
        assert_eq!(
            entry.inventory_identity().unwrap().provider_family,
            "test-family"
        );
        assert_eq!(
            entry.create(vec![]).await.err().unwrap().to_string(),
            "from_env"
        );
        assert_eq!(
            entry
                .create_with_working_dir(vec![], PathBuf::from("test-workspace"))
                .await
                .err()
                .unwrap()
                .to_string(),
            "working_dir"
        );
        assert_eq!(
            entry
                .create_with_default_model(vec![])
                .await
                .err()
                .unwrap()
                .to_string(),
            "default_model"
        );

        registry.register_acp_with_inventory::<TestAcpDef>(false, None);
        let entry = &registry.entries["test-acp"];
        assert_eq!(entry.provider_type(), ProviderType::Builtin);
        assert!(!entry.supports_inventory_refresh());
    }

    struct StubAcpDef;

    impl super::super::base::ProviderDescriptor for StubAcpDef {
        fn metadata() -> ProviderMetadata {
            <TestAcpDef as super::super::base::ProviderDescriptor>::metadata()
        }
    }

    impl AcpProviderDef for StubAcpDef {
        fn from_env(
            _extensions: Vec<ExtensionConfig>,
            _tls_config: Option<TlsConfig>,
        ) -> BoxFuture<'static, Result<crate::acp::AcpProvider>> {
            Box::pin(async { Ok(crate::acp::AcpProvider::new_test_stub()) })
        }
    }

    struct StubStandardDef;

    impl super::super::base::ProviderDescriptor for StubStandardDef {
        fn metadata() -> ProviderMetadata {
            <TestStandardDef as super::super::base::ProviderDescriptor>::metadata()
        }
    }

    fn standard_stub() -> crate::providers::testprovider::TestProvider {
        crate::providers::testprovider::TestProvider::new_recording(
            Arc::new(crate::acp::AcpProvider::new_test_stub()),
            "unused-recording.json",
        )
    }

    impl ProviderDef for StubStandardDef {
        type Provider = crate::providers::testprovider::TestProvider;

        fn from_env(_tls_config: Option<TlsConfig>) -> BoxFuture<'static, Result<Self::Provider>> {
            Box::pin(async { Ok(standard_stub()) })
        }
    }

    impl SessionBoundProviderDef for StubStandardDef {
        type Provider = crate::providers::testprovider::TestProvider;

        fn from_env(
            _extensions: Vec<ExtensionConfig>,
            _tls_config: Option<TlsConfig>,
        ) -> BoxFuture<'static, Result<Self::Provider>> {
            Box::pin(async { Ok(standard_stub()) })
        }
    }

    #[tokio::test]
    async fn registrations_retain_backend_variants() {
        let mut registry = ProviderRegistry::new(None);
        registry.register::<StubStandardDef>(true);
        assert!(matches!(
            registry
                .create_backend("test-standard", vec![])
                .await
                .unwrap(),
            ProviderBackend::Standard(_)
        ));
        registry.register_session_bound::<StubStandardDef>(false);
        let entry = &registry.entries["test-standard"];
        assert!(matches!(
            entry
                .create_backend_with_working_dir(vec![], PathBuf::from("workspace"))
                .await
                .unwrap(),
            ProviderBackend::Standard(_)
        ));
        registry.register_with_name::<StubStandardDef, _, _>(
            &test_config(),
            ProviderType::Declarative,
            false,
            |_| Ok(standard_stub()),
            || Ok(InventoryIdentityInput::new("custom_hf", "test")),
        );
        assert!(matches!(
            registry.create_backend("custom_hf", vec![]).await.unwrap(),
            ProviderBackend::Standard(_)
        ));
        registry.register_acp_with_inventory::<StubAcpDef>(false, None);
        let entry = &registry.entries["test-acp"];
        assert!(matches!(
            entry.create_backend(vec![]).await.unwrap(),
            ProviderBackend::Acp(_)
        ));
        assert!(matches!(
            entry
                .create_backend_with_default_model(vec![])
                .await
                .unwrap(),
            ProviderBackend::Acp(_)
        ));
        assert!(matches!(
            entry
                .create_backend_with_working_dir(vec![], PathBuf::from("workspace"))
                .await
                .unwrap(),
            ProviderBackend::Acp(_)
        ));
        assert_eq!(entry.create(vec![]).await.unwrap().get_name(), "acp-test");
    }

    fn test_config() -> DeclarativeProviderConfig {
        DeclarativeProviderConfig {
            name: "custom_hf".to_string(),
            engine: ProviderEngine::OpenAI,
            display_name: "Custom HF".to_string(),
            description: None,
            api_key_env: String::new(),
            base_url: "https://router.huggingface.co/v1".to_string(),
            models: vec![ModelInfo::new("test-model").with_context_limit(128_000)],
            headers: None,
            session_id_header_override: None,
            timeout_seconds: None,
            supports_streaming: Some(true),
            requires_auth: true,
            catalog_provider_id: Some("huggingface".to_string()),
            base_path: None,
            env_vars: None,
            auth: None,
            dynamic_models: None,
            skip_canonical_filtering: false,
            model_doc_link: None,
            setup_steps: vec![],
            toolshim: false,
            preserves_thinking: false,
            emit_clear_thinking: false,
            setup: None,
        }
    }

    #[test]
    fn register_with_name_can_override_inventory_configured() {
        let mut registry = ProviderRegistry::new(None);
        registry.register_with_name_and_inventory_configured::<OpenAiProviderDef, _, _, _>(
            &test_config(),
            ProviderType::Declarative,
            false,
            |_| unreachable!("constructor is not used by this test"),
            || Ok(InventoryIdentityInput::new("custom_hf", "huggingface")),
            || false,
        );

        let entry = registry.entries.get("custom_hf").unwrap();

        assert!(!entry.inventory_configured());
        assert!(entry.metadata().setup.is_none());
        assert!(entry.metadata().deprecated.is_none());
    }

    #[test]
    fn custom_provider_toolshim_uses_global_setting_as_fallback() {
        let mut registry = ProviderRegistry::new(None);
        for (name, toolshim) in [("custom_toolshim", true), ("custom_default", false)] {
            let mut config = test_config();
            config.name = name.to_string();
            config.toolshim = toolshim;
            registry.register_with_name::<OpenAiProviderDef, _, _>(
                &config,
                ProviderType::Custom,
                false,
                |_| unreachable!("constructor is not used by this test"),
                move || Ok(InventoryIdentityInput::new(name, name)),
            );
        }

        let toolshim = registry.entries["custom_toolshim"]
            .normalize_model_config(ModelConfig::new("test-model"))
            .unwrap();
        let fallback_enabled = registry.entries["custom_default"]
            .normalize_model_config(ModelConfig::new("test-model").with_toolshim(true))
            .unwrap();
        let fallback_disabled = registry.entries["custom_default"]
            .normalize_model_config(ModelConfig::new("test-model"))
            .unwrap();

        assert!(toolshim.toolshim);
        assert!(fallback_enabled.toolshim);
        assert!(!fallback_disabled.toolshim);
        assert!(registry.entries["custom_toolshim"].toolshim_enabled(false));
        assert!(registry.entries["custom_default"].toolshim_enabled(true));
        assert!(!registry.entries["custom_default"].toolshim_enabled(false));
    }
}
