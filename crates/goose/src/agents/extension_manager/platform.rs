use std::path::Path;
use std::sync::Arc;

use futures::FutureExt;
use rmcp::model::{CallToolRequestParams, ErrorCode, ErrorData, MetaObject, Tool};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use super::{get_tool_owner, recover_mangled_tool_name};
use super::{CallRequest, ExtensionLoadResult, ExtensionManager};
use crate::agents::extension::{ExtensionConfig, ExtensionInfo};
use crate::agents::platform_extensions::{PlatformExtension, PLATFORM_EXTENSIONS};
use crate::agents::tool_execution::{ToolCallContext, ToolCallNotificationEmitter, ToolCallResult};
use crate::config::extensions::name_to_key;

pub(super) type PlatformClients =
    std::collections::HashMap<(String, String), Arc<dyn PlatformExtension>>;

pub(super) fn is_platform(config: &ExtensionConfig) -> bool {
    matches!(
        config,
        ExtensionConfig::Builtin { .. } | ExtensionConfig::Platform { .. }
    ) && PLATFORM_EXTENSIONS.contains_key(name_to_key(&config.name()).as_str())
}

struct PlatformEntry {
    key: String,
    config: ExtensionConfig,
    client: Arc<dyn PlatformExtension>,
    unprefixed: bool,
}

#[derive(Clone)]
struct PlatformTool {
    tool: Tool,
    server_name: String,
    entry: usize,
}

pub(super) struct PlatformSnapshot {
    entries: Vec<PlatformEntry>,
    tools: OnceCell<Vec<PlatformTool>>,
    #[cfg(feature = "code-mode")]
    manager: Arc<ExtensionManager>,
}

impl PlatformSnapshot {
    pub(super) fn new(
        manager: &Arc<ExtensionManager>,
        scope: &str,
        configs: &[ExtensionConfig],
    ) -> Arc<Self> {
        let manager = Arc::clone(manager);
        let context = manager.get_context().clone();
        let selected: std::collections::HashSet<_> = configs
            .iter()
            .filter(|config| is_platform(config))
            .map(ExtensionConfig::key)
            .collect();
        manager
            .platform_clients
            .lock()
            .expect("platform clients unavailable")
            .retain(|(session, key), _| session != scope || selected.contains(key));
        let entries = configs
            .iter()
            .filter(|config| is_platform(config))
            .filter_map(|config| {
                let key = name_to_key(&config.name());
                let definition = PLATFORM_EXTENSIONS.get(key.as_str())?;
                let cache_key = (scope.to_string(), key.clone());
                let mut clients = manager
                    .platform_clients
                    .lock()
                    .expect("platform clients unavailable");
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
        Arc::new(Self {
            entries,
            tools: OnceCell::new(),
            #[cfg(feature = "code-mode")]
            manager,
        })
    }
}

/// Goose's advertised tool snapshot. MCP connections remain in the runtime lease;
/// platform tools are owned by Goose and never registered with that runtime.
#[derive(Clone)]
pub struct ExtensionLease {
    pub(crate) runtime: super::lease::McpLease,
    pub(super) platform: Arc<PlatformSnapshot>,
}

impl std::ops::Deref for ExtensionLease {
    type Target = super::lease::McpLease;
    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

impl ExtensionLease {
    #[cfg(feature = "code-mode")]
    pub(crate) fn manager(&self) -> Arc<ExtensionManager> {
        Arc::clone(&self.platform.manager)
    }

    async fn platform_catalog(&self) -> &Vec<PlatformTool> {
        self.platform
            .tools
            .get_or_init(|| async {
                let has_resources = self
                    .runtime
                    .instructions()
                    .await
                    .iter()
                    .any(|info| info.has_resources)
                    || self.platform.entries.iter().any(|entry| {
                        entry
                            .client
                            .get_info()
                            .is_some_and(|info| info.capabilities.resources.is_some())
                    });
                let mut catalog = Vec::new();
                let mut names = std::collections::HashSet::new();
                for (index, entry) in self.platform.entries.iter().enumerate() {
                    let mut cursor = None;
                    loop {
                        let Ok(page) = entry
                            .client
                            .list_tools(self.scope_id(), cursor, CancellationToken::new())
                            .await
                        else {
                            break;
                        };
                        for mut tool in page.tools {
                            let server_name = tool.name.to_string();
                            let available = match &entry.config {
                                ExtensionConfig::Builtin {
                                    available_tools, ..
                                }
                                | ExtensionConfig::Platform {
                                    available_tools, ..
                                } => available_tools,
                                _ => unreachable!(),
                            };
                            if !available.is_empty() && !available.contains(&server_name) {
                                continue;
                            }
                            if entry.key == "extensionmanager"
                                && !has_resources
                                && matches!(
                                    server_name.as_str(),
                                    "list_resources" | "read_resource"
                                )
                            {
                                continue;
                            }
                            if !entry.unprefixed {
                                tool.name = format!("{}__{}", entry.key, server_name).into();
                            }
                            let mut meta = tool.meta.take().map(|meta| meta.0).unwrap_or_default();
                            meta.insert("goose_extension".to_string(), entry.key.clone().into());
                            tool.meta = Some(MetaObject(meta));
                            let mut schema = (*tool.input_schema).clone();
                            if crate::agents::tool_schema_normalize::normalize_input_schema(
                                &mut schema,
                            ) {
                                tool.input_schema = Arc::new(schema);
                            }
                            if names.insert(tool.name.to_string()) {
                                catalog.push(PlatformTool {
                                    tool,
                                    server_name,
                                    entry: index,
                                });
                            }
                        }
                        cursor = page.next_cursor;
                        if cursor.is_none() {
                            break;
                        }
                    }
                }
                catalog
            })
            .await
    }

    #[cfg(test)]
    pub(crate) fn with_platform_client(
        mut self,
        config: ExtensionConfig,
        client: Arc<dyn PlatformExtension>,
    ) -> Self {
        let key = config.key();
        let definition = PLATFORM_EXTENSIONS
            .get(key.as_str())
            .expect("registered platform extension");
        self.platform = Arc::new(PlatformSnapshot {
            entries: vec![PlatformEntry {
                key,
                config,
                client,
                unprefixed: definition.unprefixed_tools,
            }],
            tools: OnceCell::new(),
            #[cfg(feature = "code-mode")]
            manager: Arc::clone(&self.platform.manager),
        });
        self
    }

    pub async fn platform_tools(&self) -> Vec<Tool> {
        self.platform_catalog()
            .await
            .iter()
            .map(|item| item.tool.clone())
            .collect()
    }

    pub async fn tools(&self) -> Vec<Tool> {
        let mut tools = self.platform_tools().await;
        let mut names: std::collections::HashSet<_> =
            tools.iter().map(|tool| tool.name.to_string()).collect();
        tools.extend(
            self.runtime
                .tools()
                .await
                .into_iter()
                .filter(|tool| names.insert(tool.name.to_string())),
        );
        tools
    }

    pub async fn tools_for(&self, extension: &str) -> Vec<Tool> {
        let key = name_to_key(extension);
        self.tools()
            .await
            .into_iter()
            .filter(|tool| get_tool_owner(tool).as_deref() == Some(&key))
            .collect()
    }

    pub async fn tools_excluding(&self, extension: &str) -> Vec<Tool> {
        let key = name_to_key(extension);
        self.tools()
            .await
            .into_iter()
            .filter(|tool| get_tool_owner(tool).as_deref() != Some(&key))
            .collect()
    }

    pub fn configs(&self) -> Vec<ExtensionConfig> {
        self.platform
            .entries
            .iter()
            .map(|entry| entry.config.clone())
            .chain(self.runtime.configs())
            .collect()
    }

    pub async fn untrusted_tools(&self) -> Vec<Tool> {
        self.runtime.tools().await
    }

    pub fn is_enabled(&self, extension: &str) -> bool {
        let key = name_to_key(extension);
        self.platform.entries.iter().any(|entry| entry.key == key)
            || self.runtime.is_enabled(extension)
    }

    pub fn enabled_names(&self) -> Vec<String> {
        self.platform
            .entries
            .iter()
            .map(|entry| entry.key.clone())
            .chain(
                self.runtime
                    .configs()
                    .iter()
                    .map(|registration| registration.key()),
            )
            .collect()
    }

    pub async fn start(&self) -> Vec<ExtensionLoadResult> {
        let mut results = self.runtime.start().await;
        results.extend(
            self.platform
                .entries
                .iter()
                .map(|entry| ExtensionLoadResult {
                    name: entry.config.name(),
                    success: true,
                    error: None,
                }),
        );
        results
    }

    pub async fn instructions(&self) -> Vec<ExtensionInfo> {
        let mut infos = self.runtime.instructions().await;
        let directory = self
            .working_dir()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        for entry in &self.platform.entries {
            let instructions = entry
                .client
                .get_instructions(self.scope_id(), &directory)
                .await
                .unwrap_or_default();
            infos.push(ExtensionInfo::new(
                &entry.key,
                &instructions.replace("{{WORKING_DIR}}", &directory.to_string_lossy()),
                entry
                    .client
                    .get_info()
                    .is_some_and(|info| info.capabilities.resources.is_some()),
            ));
        }
        infos
    }

    pub async fn moim(&self) -> Vec<String> {
        let mut parts = Vec::new();
        for entry in &self.platform.entries {
            let tools = self.tools_excluding(&entry.key).await;
            if let Some(part) = entry.client.get_moim(self.scope_id(), &tools).await {
                parts.push(part);
            }
        }
        parts
    }

    pub async fn call(
        &self,
        tool_call: CallToolRequestParams,
        request: CallRequest,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ErrorData> {
        let catalog = self.platform_catalog().await;
        let name = recover_mangled_tool_name(
            &tool_call.name,
            catalog
                .iter()
                .map(|item| (item.tool.name.as_ref(), get_tool_owner(&item.tool)))
                .collect::<Vec<_>>()
                .iter()
                .map(|(name, owner)| (*name, owner.as_deref())),
        );
        let item = catalog.iter().find(|item| {
            item.tool.name == tool_call.name || name.as_deref() == Some(item.tool.name.as_ref())
        });
        let Some(item) = item else {
            return self
                .runtime
                .call(tool_call, request, cancellation_token)
                .await;
        };
        let client = Arc::clone(&self.platform.entries[item.entry].client);
        let (sender, receiver) = tokio::sync::mpsc::channel(32);
        let emitter = request
            .notification_emitter
            .unwrap_or_else(|| ToolCallNotificationEmitter::new(sender));
        let context = ToolCallContext::new(
            self.scope_id().to_string(),
            self.working_dir().map(Path::to_path_buf),
            request.tool_call_id,
        )
        .with_container(request.container)
        .with_extension_lease(Arc::new(self.clone()))
        .with_notification_emitter(emitter);
        let server_name = item.server_name.clone();
        Ok(ToolCallResult {
            result: Box::new(
                async move {
                    client
                        .call_tool(
                            &context,
                            &server_name,
                            tool_call.arguments,
                            cancellation_token,
                        )
                        .await
                        .map_err(|error| match error {
                            rmcp::ServiceError::McpError(error) => error,
                            error => {
                                ErrorData::new(ErrorCode::INTERNAL_ERROR, error.to_string(), None)
                            }
                        })
                }
                .boxed(),
            ),
            notification_stream: Some(Box::new(tokio_stream::wrappers::ReceiverStream::new(
                receiver,
            ))),
            action_required_stream: None,
        })
    }

    pub async fn list_tools_from_extension(
        &self,
        extension: &str,
        cancellation: CancellationToken,
    ) -> Result<rmcp::model::ListToolsResult, ErrorData> {
        if let Some(entry) = self
            .platform
            .entries
            .iter()
            .find(|entry| entry.key == name_to_key(extension))
        {
            return entry
                .client
                .list_tools(self.scope_id(), None, cancellation)
                .await
                .map_err(|error| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, error.to_string(), None)
                });
        }
        self.runtime
            .list_tools_from_extension(extension, cancellation)
            .await
    }

    pub async fn read_resource(
        &self,
        uri: &str,
        extension: &str,
        cancellation: CancellationToken,
    ) -> Result<rmcp::model::ReadResourceResult, ErrorData> {
        if let Some(entry) = self
            .platform
            .entries
            .iter()
            .find(|entry| entry.key == name_to_key(extension))
        {
            return entry
                .client
                .read_resource(self.scope_id(), uri, cancellation)
                .await
                .map_err(|error| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, error.to_string(), None)
                });
        }
        self.runtime
            .read_resource(uri, extension, cancellation)
            .await
    }

    pub async fn list_resources_result_from_extension(
        &self,
        extension: &str,
        cancellation: CancellationToken,
    ) -> Result<rmcp::model::ListResourcesResult, ErrorData> {
        if let Some(entry) = self
            .platform
            .entries
            .iter()
            .find(|entry| entry.key == name_to_key(extension))
        {
            return entry
                .client
                .list_resources(self.scope_id(), None, cancellation)
                .await
                .map_err(|error| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, error.to_string(), None)
                });
        }
        self.runtime
            .list_resources_result_from_extension(extension, cancellation)
            .await
    }

    pub async fn read_resource_tool(
        &self,
        params: serde_json::Value,
        cancellation: CancellationToken,
    ) -> Result<Vec<rmcp::model::ContentBlock>, ErrorData> {
        let required = |name: &str| {
            params
                .get(name)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    ErrorData::invalid_params(
                        format!("The parameter {name} must be a string"),
                        None,
                    )
                })
        };
        let uri = required("uri")?;
        let extension = required("extension_name")?;
        Ok(self
            .read_resource(uri, extension, cancellation)
            .await?
            .contents
            .into_iter()
            .filter_map(|content| match content {
                rmcp::model::ResourceContents::TextResourceContents { text, .. } => {
                    Some(rmcp::model::ContentBlock::text(format!("{uri}\n\n{text}")))
                }
                _ => None,
            })
            .collect())
    }

    pub async fn list_resources(
        &self,
        params: serde_json::Value,
        cancellation: CancellationToken,
    ) -> Result<Vec<rmcp::model::ContentBlock>, ErrorData> {
        let names = match params
            .get("extension_name")
            .and_then(serde_json::Value::as_str)
        {
            Some(name) => vec![name.to_string()],
            None => self
                .instructions()
                .await
                .into_iter()
                .filter(|info| info.has_resources)
                .map(|info| info.name)
                .collect(),
        };
        let mut content = Vec::new();
        for name in names {
            let result = self
                .list_resources_result_from_extension(&name, cancellation.clone())
                .await;
            match result {
                Ok(result) => content.push(rmcp::model::ContentBlock::text(
                    result
                        .resources
                        .into_iter()
                        .map(|resource| {
                            format!("{name} - {}, uri: ({})", resource.name, resource.uri)
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                )),
                Err(error) if params.get("extension_name").is_some() => return Err(error),
                Err(_) => {}
            }
        }
        Ok(content)
    }

    pub async fn ui_resources(&self) -> Vec<(String, rmcp::model::Resource)> {
        let mut resources = self.runtime.ui_resources().await;
        for entry in &self.platform.entries {
            if let Ok(result) = entry
                .client
                .list_resources(self.scope_id(), None, CancellationToken::new())
                .await
            {
                resources.extend(
                    result
                        .resources
                        .into_iter()
                        .filter(|resource| resource.uri.starts_with("ui://"))
                        .map(|resource| (entry.key.clone(), resource)),
                );
            }
        }
        resources
    }
}
