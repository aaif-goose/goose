use super::{PlatformExtension, PlatformExtensionContext, PLATFORM_EXTENSIONS};
use crate::agents::extension::ExtensionConfig;
use crate::agents::tool_snapshot::{PlatformEntry, PlatformTools};
use crate::config::extensions::name_to_key;
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;

pub(crate) type PlatformClients =
    std::collections::HashMap<(String, String), Arc<dyn PlatformExtension>>;

pub(crate) fn is_platform(config: &ExtensionConfig) -> bool {
    matches!(
        config,
        ExtensionConfig::Builtin { .. } | ExtensionConfig::Platform { .. }
    ) && PLATFORM_EXTENSIONS.contains_key(name_to_key(&config.name()).as_str())
}

#[derive(Default)]
pub struct PlatformExtensions {
    clients: Mutex<PlatformClients>,
}

impl PlatformExtensions {
    pub(crate) fn select(
        &self,
        scope: &str,
        working_dir: Option<std::path::PathBuf>,
        configs: &[ExtensionConfig],
        context: &PlatformExtensionContext,
    ) -> Arc<PlatformTools> {
        let selected: std::collections::HashSet<_> = configs
            .iter()
            .filter(|config| is_platform(config))
            .map(ExtensionConfig::key)
            .collect();
        let mut clients = self.clients.lock().expect("platform clients unavailable");
        clients.retain(|(session, key), _| session != scope || selected.contains(key));
        let entries = configs
            .iter()
            .filter(|config| is_platform(config))
            .filter_map(|config| {
                let key = name_to_key(&config.name());
                let definition = PLATFORM_EXTENSIONS.get(key.as_str())?;
                let cache_key = (scope.to_string(), key.clone());
                let client = match clients.get(&cache_key) {
                    Some(client) => Arc::clone(client),
                    None => {
                        let client: Arc<dyn PlatformExtension> =
                            Arc::from((definition.client_factory)(context.clone())?);
                        clients.insert(cache_key, Arc::clone(&client));
                        client
                    }
                };
                Some(PlatformEntry {
                    key,
                    config: config.clone(),
                    client,
                    unprefixed: definition.unprefixed_tools,
                })
            })
            .collect();
        drop(clients);
        Arc::new(PlatformTools {
            entries,
            catalog: OnceCell::new(),
            session_id: scope.to_string(),
            working_dir,
        })
    }

    pub(crate) fn release(&self, session: &str) {
        self.clients
            .lock()
            .expect("platform clients unavailable")
            .retain(|(scope, _), _| scope != session);
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, scope: &str, key: &str) -> bool {
        self.clients
            .lock()
            .unwrap()
            .contains_key(&(scope.to_string(), key.to_string()))
    }
}
