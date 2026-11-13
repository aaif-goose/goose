use super::goose_host::GooseExtensionHost;
use crate::agents::container::Container;
use crate::agents::extension_config::{ExtensionConfig, ExtensionResult};
use async_trait::async_trait;
use goose_agent::extension_manager::host::ExtensionEnvironment;
use goose_agent::extension_manager::{ConnectRequest, Connector, Registration};
use goose_agent::mcp_client::McpClientTrait;
use std::sync::Arc;

pub(super) fn registration(
    config: ExtensionConfig,
    container: Option<Container>,
    host: Arc<GooseExtensionHost>,
) -> Registration {
    let available_tools = match &config {
        ExtensionConfig::External(transport) => {
            return Registration::from_config(
                transport.clone(),
                Some(Arc::new(GoosePlacement { container, host })),
            );
        }
        ExtensionConfig::Builtin {
            available_tools, ..
        }
        | ExtensionConfig::Platform {
            available_tools, ..
        } => available_tools.clone(),
    };
    Registration::new(
        config.name(),
        Arc::new(GooseConnector {
            config,
            container,
            host,
        }),
    )
    .with_available_tools(available_tools)
}

struct GooseConnector {
    config: ExtensionConfig,
    container: Option<Container>,
    host: Arc<GooseExtensionHost>,
}
impl PartialEq for GooseConnector {
    fn eq(&self, other: &Self) -> bool {
        self.config == other.config
            && self.container == other.container
            && Arc::ptr_eq(&self.host.session_manager, &other.host.session_manager)
    }
}

#[async_trait]
impl Connector for GooseConnector {
    async fn connect(&self, request: &ConnectRequest) -> ExtensionResult<Box<dyn McpClientTrait>> {
        let resolved = &self.config;
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
        Ok(client)
    }
}

struct GoosePlacement {
    container: Option<Container>,
    host: Arc<GooseExtensionHost>,
}
impl PartialEq for GoosePlacement {
    fn eq(&self, other: &Self) -> bool {
        self.container == other.container
            && Arc::ptr_eq(&self.host.session_manager, &other.host.session_manager)
    }
}

impl goose_agent::extension_manager::StdioCommand for GoosePlacement {
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
