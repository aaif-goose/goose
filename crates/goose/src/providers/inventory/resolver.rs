use super::{default_inventory_configured, default_inventory_identity, InventoryIdentityInput};
use crate::config::Config;
use crate::providers::base::ProviderMetadata;
use anyhow::Result;
use once_cell::sync::Lazy;
use std::sync::Arc;

static DEFAULT_INVENTORY_IDENTITY_RESOLVER: Lazy<InventoryIdentityResolver> =
    Lazy::new(|| Arc::new(|| unreachable!("default inventory identity resolver marker")));

pub fn default_inventory_identity_resolver() -> InventoryIdentityResolver {
    Arc::clone(&DEFAULT_INVENTORY_IDENTITY_RESOLVER)
}

pub type InventoryIdentityResolver = Arc<dyn Fn() -> Result<InventoryIdentityInput> + Send + Sync>;
pub type InventoryConfiguredResolver = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone)]
pub struct InventoryRegistration {
    pub supports_refresh: bool,
    pub identity: InventoryIdentityResolver,
    pub configured: Option<InventoryConfiguredResolver>,
}

impl InventoryRegistration {
    pub fn new<G>(supports_refresh: bool, identity: G) -> Self
    where
        G: Fn() -> Result<InventoryIdentityInput> + Send + Sync + 'static,
    {
        Self {
            supports_refresh,
            identity: Arc::new(identity),
            configured: None,
        }
    }

    pub fn with_configured<H>(mut self, configured: H) -> Self
    where
        H: Fn() -> bool + Send + Sync + 'static,
    {
        self.configured = Some(Arc::new(configured));
        self
    }
}

#[derive(Clone)]
pub struct InventoryResolvers {
    pub supports_refresh: bool,
    pub identity: InventoryIdentityResolver,
    pub configured: InventoryConfiguredResolver,
}

impl InventoryResolvers {
    pub fn for_metadata(
        metadata: &ProviderMetadata,
        registration: Option<InventoryRegistration>,
    ) -> Self {
        let metadata_for_identity = metadata.clone();
        let default_identity = Arc::new(move || {
            Ok(default_inventory_identity(
                &metadata_for_identity.name,
                &metadata_for_identity.name,
                &metadata_for_identity.config_keys,
                Config::global(),
            ))
        });

        let config_keys = metadata.config_keys.clone();
        let provider_id = metadata.name.clone();
        let default_configured = Arc::new(move || {
            default_inventory_configured(&provider_id, &config_keys, Config::global(), false)
        });

        match registration {
            Some(registration) => Self {
                supports_refresh: registration.supports_refresh,
                identity: if Arc::ptr_eq(
                    &registration.identity,
                    &default_inventory_identity_resolver(),
                ) {
                    default_identity
                } else {
                    registration.identity
                },
                configured: registration.configured.unwrap_or(default_configured),
            },
            None => Self {
                supports_refresh: false,
                identity: default_identity,
                configured: default_configured,
            },
        }
    }
}
