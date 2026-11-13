use super::goose_host::GooseExtensionHost;
use crate::agents::container::Container;
use crate::agents::extension_config::{name_to_key, ExtensionConfig, ExtensionResult};
use crate::agents::extension_runtime::host::ExtensionHost;
use crate::agents::extension_runtime::{ConnectRequest, Connector, Registration};
use crate::agents::mcp_client::McpClientTrait;
use crate::agents::platform_extensions::PLATFORM_EXTENSIONS;
use async_trait::async_trait;
use std::sync::Arc;

pub(super) fn registration(
    config: ExtensionConfig,
    container: Option<Container>,
    host: Arc<GooseExtensionHost>,
) -> Registration {
    let def = match &config {
        ExtensionConfig::Builtin { name, .. } | ExtensionConfig::Platform { name, .. } => {
            PLATFORM_EXTENSIONS.get(name_to_key(name).as_str())
        }
        _ => None,
    };
    use std::hash::{Hash, Hasher};
    let mut revision = std::collections::hash_map::DefaultHasher::new();
    container.as_ref().map(|c| c.id()).hash(&mut revision);
    let revision = if def.is_some() { 0 } else { revision.finish() };
    let mut registration = match &config {
        ExtensionConfig::External(transport) => Registration::from_config_with_command(
            transport.clone(),
            host.clone(),
            Arc::new(GoosePlacement { container, host }),
        ),
        _ => Registration::new(
            config.name(),
            Arc::new(GooseConnector {
                config: config.clone(),
                container,
                host,
            }),
        ),
    }
    .with_revision(revision)
    .with_metadata(serde_json::to_value(&config).expect("config serializes"));
    let available_tools = match &config {
        ExtensionConfig::Builtin {
            available_tools, ..
        }
        | ExtensionConfig::Platform {
            available_tools, ..
        }
        | ExtensionConfig::External(goose_agent::extension::ExtensionConfig::Stdio {
            available_tools,
            ..
        })
        | ExtensionConfig::External(goose_agent::extension::ExtensionConfig::StreamableHttp {
            available_tools,
            ..
        }) => available_tools.clone(),
    };
    registration = registration.with_available_tools(available_tools);
    if def.is_some() {
        registration = registration.trusted().location_independent();
    }
    if def.is_some_and(|def| def.unprefixed_tools) {
        registration = registration.unprefixed();
    }
    registration
}

struct GooseConnector {
    config: ExtensionConfig,
    container: Option<Container>,
    host: Arc<GooseExtensionHost>,
}
#[async_trait]
impl Connector for GooseConnector {
    async fn connect(
        &self,
        request: &ConnectRequest,
    ) -> ExtensionResult<Option<Box<dyn McpClientTrait>>> {
        let resolved = &self.config;
        if let ExtensionConfig::Builtin { name, .. } | ExtensionConfig::Platform { name, .. } =
            resolved
        {
            if let Some(def) = PLATFORM_EXTENSIONS.get(name_to_key(name).as_str()) {
                let mut context = self.host.context.clone();
                context.extension_manager = Some(request.manager.clone());
                return Ok((def.client_factory)(context));
            }
        }
        let mut context =
            request.connect_context(None, self.host.connection_working_dir(&request.working_dir));
        let client: Box<dyn McpClientTrait> = match resolved {
            ExtensionConfig::Builtin { name, timeout, .. } => {
                context.timeout = std::time::Duration::from_secs(
                    timeout.unwrap_or_else(|| self.host.default_timeout()),
                );
                super::builtin::connect(name, self.container.as_ref(), context, self.host.as_ref())
                    .await?
            }
            ExtensionConfig::Platform { name, .. } => {
                super::builtin::connect(name, self.container.as_ref(), context, self.host.as_ref())
                    .await?
            }
            ExtensionConfig::External(_) => unreachable!("external configs use the GDK connector"),
        };
        Ok(Some(client))
    }
}

struct GoosePlacement {
    container: Option<Container>,
    host: Arc<GooseExtensionHost>,
}
impl crate::agents::extension_runtime::StdioCommand for GoosePlacement {
    fn command(
        &self,
        program: &str,
        args: &[String],
        env: std::collections::HashMap<String, String>,
    ) -> ExtensionResult<tokio::process::Command> {
        self.host
            .stdio_command(program, args, env, self.container.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::extension_manager::ExtensionManager;

    #[test]
    fn saved_configs_round_trip_without_changing_the_session_schema() {
        for value in [
            serde_json::json!({"type":"builtin", "name":"todo", "description":"", "display_name":null, "timeout":30, "bundled":true}),
            serde_json::json!({"type":"platform", "name":"developer", "description":"", "display_name":null, "bundled":true}),
            serde_json::json!({"type":"stdio", "name":"echo", "description":"", "cmd":"python3", "args":[], "timeout":30}),
            serde_json::json!({"type":"streamable_http", "name":"web", "description":"", "uri":"https://example.com/mcp", "timeout":30}),
        ] {
            let config: ExtensionConfig = serde_json::from_value(value).unwrap();
            let saved = serde_json::to_value(&config).unwrap();
            let manager = ExtensionManager::with_data_dir(std::env::temp_dir());
            let registration = registration(config.clone(), None, manager.host());
            assert_eq!(registration.metadata(), &saved);
            let restored: ExtensionConfig =
                serde_json::from_value(registration.metadata().clone()).unwrap();
            assert_eq!(restored, config);
            if let Some(external) = config.external() {
                assert_eq!(serde_json::to_value(external).unwrap(), saved);
            }
        }
    }

    #[tokio::test]
    async fn container_changes_external_registration_revision_but_not_in_process_revision() {
        let directory = tempfile::tempdir().unwrap();
        let manager = ExtensionManager::with_data_dir(directory.path().to_path_buf());
        let configs = vec![
            ExtensionConfig::stdio("echo", "python3", "", 30u64),
            ExtensionConfig::default(),
        ];
        let first = manager
            .extension_set(
                "scope",
                None,
                configs.clone(),
                Some(Container::new("first")),
            )
            .unwrap();
        manager.resolve(&first).await;
        let before = manager.manager.scope_lease("scope").await;
        let second = manager
            .extension_set("scope", None, configs, Some(Container::new("second")))
            .unwrap();
        let after = manager.resolve(&second).await;
        // Metadata remains the persisted config; placement identity is a revision.
        let old = before.registrations();
        let new = after.registrations();
        assert_eq!(old[0].metadata(), new[0].metadata());
        assert_ne!(old[0].revision(), new[0].revision());
        assert_eq!(old[1].revision(), new[1].revision());
    }
}
