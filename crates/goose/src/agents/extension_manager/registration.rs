use super::goose_host::GooseExtensionHost;
use crate::agents::container::Container;
use crate::agents::extension_config::{name_to_key, ExtensionConfig, ExtensionResult};
use crate::agents::extension_runtime::host::ExtensionEnvironment;
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
    let mut registration = match &config {
        ExtensionConfig::External(transport) => Registration::from_config(
            transport.clone(),
            Some(Arc::new(GoosePlacement {
                container: container.clone(),
                host,
            })),
        ),
        _ => Registration::new(
            config.name(),
            Arc::new(GooseConnector {
                config: config.clone(),
                container: container.clone(),
                host,
            }),
        ),
    }
    .with_identity(
        serde_json::to_value(&(
            &config,
            if def.is_some() {
                None
            } else {
                container.as_ref().map(|c| c.id())
            },
        ))
        .expect("config serializes"),
    );
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
