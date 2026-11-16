use std::path::Path;
use std::sync::{Arc, Mutex};

use futures::FutureExt;
use rmcp::model::{CallToolRequestParams, ErrorCode, ErrorData, MetaObject, Tool};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use crate::agents::extension::{ExtensionConfig, ExtensionInfo};
use crate::agents::extension_manager::{get_tool_owner, recover_mangled_tool_name};
use crate::agents::extension_manager::{CallRequest, ExtensionLoadResult};
use crate::agents::platform_extensions::PlatformExtension;
#[cfg(test)]
use crate::agents::platform_extensions::PLATFORM_EXTENSIONS;
use crate::agents::tool_execution::{ToolCallContext, ToolCallNotificationEmitter, ToolCallResult};
use crate::agents::StateMachineServices;
use crate::config::extensions::name_to_key;

#[derive(Clone)]
pub(crate) struct PlatformEntry {
    pub(crate) key: String,
    pub(crate) config: ExtensionConfig,
    pub(crate) client: Arc<dyn PlatformExtension>,
    pub(crate) unprefixed: bool,
}

#[derive(Clone)]
pub(crate) struct PlatformTool {
    tool: Tool,
    server_name: String,
    entry: usize,
}

pub(crate) struct PlatformTools {
    pub(crate) entries: Vec<PlatformEntry>,
    pub(crate) catalog: OnceCell<Vec<PlatformTool>>,
    pub(crate) session_id: String,
    pub(crate) working_dir: Option<std::path::PathBuf>,
}

/// The platform instances and MCP lease advertised to an inference request.
#[derive(Clone)]
pub struct ToolSnapshot {
    pub mcp: crate::agents::extension_manager::McpLease,
    pub(crate) platform: Arc<PlatformTools>,
}

impl ToolSnapshot {
    pub(crate) async fn for_session(
        current: &Mutex<Option<Arc<Self>>>,
        services: &Arc<StateMachineServices>,
        session_id: &str,
    ) -> anyhow::Result<Arc<Self>> {
        let tools = current.lock().expect("tool snapshot unavailable").clone();
        if let Some(tools) = tools.filter(|tools| tools.scope_id() == session_id) {
            return Ok(tools);
        }
        let tools = Arc::new(services.current_tools(session_id).await?);
        *current.lock().expect("tool snapshot unavailable") = Some(Arc::clone(&tools));
        Ok(tools)
    }

    pub(crate) fn new(
        mcp: crate::agents::extension_manager::McpLease,
        platform: Arc<PlatformTools>,
    ) -> Self {
        Self { mcp, platform }
    }

    pub fn scope_id(&self) -> &str {
        self.mcp.scope_id()
    }

    pub fn working_dir(&self) -> Option<&Path> {
        self.mcp.working_dir()
    }

    async fn platform_catalog(&self) -> &Vec<PlatformTool> {
        self.platform
            .catalog
            .get_or_init(|| async {
                let has_resources = self
                    .mcp
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
                self.platform.build_catalog(has_resources).await
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
        self.platform = Arc::new(PlatformTools {
            entries: vec![PlatformEntry {
                key,
                config,
                client,
                unprefixed: definition.unprefixed_tools,
            }],
            catalog: OnceCell::new(),
            session_id: self.scope_id().to_string(),
            working_dir: self.working_dir().map(Path::to_path_buf),
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
            self.mcp
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
            .chain(self.mcp.configs())
            .collect()
    }

    pub fn is_enabled(&self, extension: &str) -> bool {
        let key = name_to_key(extension);
        self.platform.entries.iter().any(|entry| entry.key == key) || self.mcp.is_enabled(extension)
    }

    pub fn enabled_names(&self) -> Vec<String> {
        self.platform
            .entries
            .iter()
            .map(|entry| entry.key.clone())
            .chain(
                self.mcp
                    .configs()
                    .iter()
                    .map(|registration| registration.key()),
            )
            .collect()
    }

    pub async fn start(&self) -> Vec<ExtensionLoadResult> {
        let mut results = self.mcp.start().await;
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
        let mut infos = self.mcp.instructions().await;
        infos.extend(self.platform.instructions().await);
        infos
    }

    pub async fn moim(&self) -> Vec<String> {
        let mut parts = Vec::new();
        for entry in &self.platform.entries {
            if let Some(part) = entry.client.get_moim(self.scope_id()).await {
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
        services: &Arc<StateMachineServices>,
    ) -> Result<ToolCallResult, ErrorData> {
        let catalog = self.platform_catalog().await;
        let item = catalog
            .iter()
            .find(|item| item.tool.name == tool_call.name)
            .or_else(|| {
                let owners: Vec<_> = catalog
                    .iter()
                    .map(|item| (item.tool.name.as_ref(), get_tool_owner(&item.tool)))
                    .collect();
                let name = recover_mangled_tool_name(
                    &tool_call.name,
                    owners.iter().map(|(name, owner)| (*name, owner.as_deref())),
                )?;
                catalog.iter().find(|item| item.tool.name.as_ref() == name)
            });
        let result = match item {
            Some(item) => self.platform.call(
                item,
                tool_call,
                request,
                cancellation_token,
                Arc::new(self.clone()),
                Arc::clone(services),
            ),
            None => {
                self.mcp
                    .call(tool_call, request, cancellation_token)
                    .await?
            }
        };
        Ok(services.applying_mutation(result, self.scope_id()))
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
                .list_tools(self.scope_id())
                .await
                .map(rmcp::model::ListToolsResult::with_all_items)
                .map_err(|error| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, error.to_string(), None)
                });
        }
        self.mcp
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
        self.mcp.read_resource(uri, extension, cancellation).await
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
        self.mcp
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
        let mut resources = self.mcp.ui_resources().await;
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

impl PlatformTools {
    async fn build_catalog(&self, has_resources: bool) -> Vec<PlatformTool> {
        let mut catalog = Vec::new();
        let mut names = std::collections::HashSet::new();
        for (index, entry) in self.entries.iter().enumerate() {
            let Ok(tools) = entry.client.list_tools(&self.session_id).await else {
                continue;
            };
            for mut tool in tools {
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
                    && matches!(server_name.as_str(), "list_resources" | "read_resource")
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
                if crate::agents::tool_schema_normalize::normalize_input_schema(&mut schema) {
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
        }
        catalog
    }

    pub(crate) async fn instructions(&self) -> Vec<ExtensionInfo> {
        let mut infos = Vec::new();
        let directory = self.working_dir.as_deref().unwrap_or(Path::new(""));
        for entry in &self.entries {
            let instructions = entry
                .client
                .get_instructions(&self.session_id, directory)
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

    fn call(
        &self,
        item: &PlatformTool,
        tool_call: CallToolRequestParams,
        request: CallRequest,
        cancellation_token: CancellationToken,
        snapshot: Arc<ToolSnapshot>,
        services: Arc<StateMachineServices>,
    ) -> ToolCallResult {
        let client = Arc::clone(&self.entries[item.entry].client);
        let (sender, receiver) = tokio::sync::mpsc::channel(32);
        let emitter = request
            .notification_emitter
            .unwrap_or_else(|| ToolCallNotificationEmitter::new(sender));
        let context = ToolCallContext::new(
            self.session_id.clone(),
            self.working_dir.clone(),
            request.tool_call_id,
        )
        .with_container(request.container)
        .with_tool_snapshot(snapshot)
        .with_services(services)
        .with_notification_emitter(emitter);
        let server_name = item.server_name.clone();
        ToolCallResult {
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
        }
    }

    pub(crate) async fn prepare(
        &self,
        snapshot: &Arc<ToolSnapshot>,
        requests: Vec<crate::conversation::message::ToolRequest>,
        session: &crate::session::Session,
        cancellation: CancellationToken,
        services: &Arc<StateMachineServices>,
    ) -> Vec<(crate::conversation::message::ToolRequest, ToolCallResult)> {
        let catalog = snapshot.platform_catalog().await;
        requests
            .into_iter()
            .filter_map(|request| {
                let call = request.tool_call.as_ref().ok()?;
                let item = catalog.iter().find(|item| item.tool.name == call.name)?;
                let result = self.call(
                    item,
                    call.clone(),
                    CallRequest::new(request.id.clone()).with_container(session.container.clone()),
                    cancellation.clone(),
                    snapshot.clone(),
                    Arc::clone(services),
                );
                Some((request, services.applying_mutation(result, &session.id)))
            })
            .collect()
    }
}
