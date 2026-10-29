use super::javascript::{self, ToolCallback, ToolFuture};
use crate::agents::platform_extensions::PlatformExtensionContext;
use anyhow::Result;
use async_trait::async_trait;
use goose_agent::extension_manager::{
    get_tool_resource_uri, is_tool_owned_by_extension, CallRequest, ExtensionLease,
};
use goose_agent::mcp_client::{Error, McpClientTrait};
use goose_agent::tool_execution::ToolCallContext;
use goose_agent::tool_visibility::is_tool_visible_to_model;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, Implementation, InitializeResult,
    JsonObject, ListToolsResult, Role, ServerCapabilities, Tool as McpTool, ToolAnnotations,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "code_execution";

const INSTRUCTIONS: &str = "Discover exact Goose tool names with list_functions and inspect unfamiliar parameters with get_function_details. Write JavaScript defining async function run() and execute it with execute_javascript. Call tools using await tools.call(\"extension__tool_name\", { ... }); no imports or generated namespaces are needed. Await every tool call and batch independent operations with Promise.all. Return or log only information needed for the next step. JavaScript contexts are isolated, with no filesystem, shell, network, packages, or environment access; use discovered tools for authorized external operations.";

pub struct CodeExecutionClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ToolGraphNode {
    /// Exact Goose tool name, such as shell or extension__tool_name.
    tool: String,
    /// Brief description of what this call does.
    description: String,
    /// Indices of nodes this depends on (empty if independent).
    #[serde(default)]
    depends_on: Vec<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteWithToolGraph {
    /// JavaScript defining async function run(). Await tools.call(name, arguments).
    code: String,
    /// Optional DAG of tool calls for desktop display.
    #[serde(default)]
    tool_graph: Vec<ToolGraphNode>,
}

#[derive(Deserialize, JsonSchema)]
struct GetFunctionDetailsInput {
    /// Exact tool names returned by list_functions.
    functions: Vec<String>,
}

fn callable_catalog(tools: Vec<McpTool>) -> Vec<McpTool> {
    tools
        .into_iter()
        .filter(|tool| {
            get_tool_resource_uri(tool).is_none()
                && is_tool_visible_to_model(tool)
                && !is_tool_owned_by_extension(tool, EXTENSION_NAME)
                && !is_summon_delegate(tool)
        })
        .collect()
}

fn function_details(catalog: &[McpTool], names: &[String]) -> Result<Value, String> {
    names
        .iter()
        .map(|name| {
            let tool = catalog
                .iter()
                .find(|tool| tool.name == *name)
                .ok_or_else(|| unknown_tool(name))?;
            Ok(json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema,
                "output_schema": tool.output_schema,
            }))
        })
        .collect::<Result<Vec<_>, String>>()
        .map(Value::Array)
}

fn unknown_tool(name: &str) -> String {
    format!("Unknown tool {name:?}; use list_functions and get_function_details to discover callable tools in the current snapshot")
}

impl CodeExecutionClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(EXTENSION_NAME, "1.0.0").with_title("Code Mode"))
            .with_instructions(INSTRUCTIONS);
        Ok(Self { info, context })
    }

    async fn handle_execute_javascript(
        &self,
        ctx: &ToolCallContext,
        arguments: Option<JsonObject>,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<ContentBlock>, String> {
        let input: ExecuteWithToolGraph = parse_arguments(arguments)?;
        let catalog =
            callable_catalog(dispatching_lease(ctx).tools_excluding(EXTENSION_NAME).await);
        let manager = self
            .context
            .extension_manager
            .as_ref()
            .and_then(|w| w.upgrade())
            .ok_or("Extension manager not available")?;
        let dispatch = cancellation_token.child_token();
        let callback: ToolCallback = Arc::new({
            let ctx = ctx.clone();
            let dispatch = dispatch.clone();
            let rt = tokio::runtime::Handle::current();
            move |name, args| {
                if !catalog.iter().any(|tool| tool.name == name) {
                    return Box::pin(async move { Err(unknown_tool(&name)) });
                }
                if dispatch.is_cancelled() {
                    return Box::pin(async { Err("Execution cancelled".into()) });
                }
                dispatch_tool(
                    ctx.clone(),
                    name,
                    manager.clone(),
                    dispatch.clone(),
                    rt.clone(),
                    args,
                )
            }
        });
        let output = javascript::execute(
            input.code,
            callback,
            execution_timeout(),
            cancellation_token,
            dispatch,
        )
        .await?;
        Ok(vec![ContentBlock::text(output)])
    }
}

fn parse_arguments<T: serde::de::DeserializeOwned>(
    arguments: Option<JsonObject>,
) -> Result<T, String> {
    serde_json::from_value(Value::Object(arguments.ok_or("Missing arguments")?))
        .map_err(|e| format!("Invalid arguments: {e}; inspect the tool's input schema"))
}

fn is_summon_delegate(tool: &McpTool) -> bool {
    tool.name == "delegate" && is_tool_owned_by_extension(tool, super::summon::EXTENSION_NAME)
}

fn execution_timeout() -> Duration {
    let secs = crate::config::Config::global()
        .get_goose_default_extension_timeout()
        .unwrap_or(goose_agent::extension::DEFAULT_EXTENSION_TIMEOUT);
    Duration::from_secs(secs)
}

fn dispatching_lease(ctx: &ToolCallContext) -> &ExtensionLease {
    ctx.extension_lease()
        .expect("platform tool calls are dispatched through a lease")
}

fn dispatch_tool(
    ctx: ToolCallContext,
    full_name: String,
    manager: Arc<goose_agent::extension_manager::ExtensionManager>,
    cancellation_token: CancellationToken,
    rt: tokio::runtime::Handle,
    args: Value,
) -> ToolFuture {
    Box::pin(async move {
        let arguments = args
            .as_object()
            .cloned()
            .ok_or("Tool arguments must be a JSON object")?;
        let tool_call = CallToolRequestParams::new(full_name).with_arguments(arguments);
        let handle = rt.spawn(async move {
            let execution = async {
                let dispatch_result = dispatching_lease(&ctx)
                    .call(
                        tool_call,
                        CallRequest::from(&ctx),
                        cancellation_token.clone(),
                    )
                    .await;
                match dispatch_result {
                    Ok(dispatch_result) => match manager
                        .applying_mutation(dispatch_result, &ctx.session_id)
                        .result
                        .await
                    {
                        Ok(result) if result.is_error == Some(true) => {
                            Err(callback_result_to_value(&result).to_string())
                        }
                        Ok(result) => Ok(callback_result_to_value(&result)),
                        Err(e) => Err(format!("Tool error: {}", e.message)),
                    },
                    Err(e) => Err(format!("Dispatch error: {e}")),
                }
            };
            tokio::pin!(execution);
            tokio::select! {
                biased;
                result = &mut execution => result,
                _ = cancellation_token.cancelled() => {
                    // Cleanup runs on Goose's runtime even if the Boa worker cannot yield.
                    tokio::time::timeout(javascript::DRAIN_TIMEOUT, &mut execution)
                        .await
                        .unwrap_or_else(|_| Err("Execution cancelled".into()))
                }
            }
        });
        struct AbortOnDrop(tokio::task::AbortHandle);
        impl Drop for AbortOnDrop {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _guard = AbortOnDrop(handle.abort_handle());
        handle
            .await
            .unwrap_or_else(|e| Err(format!("Callback task failed: {e}")))
    })
}

fn callback_result_to_value(result: &CallToolResult) -> Value {
    let text = result
        .content
        .iter()
        .filter_map(|content| match content {
            ContentBlock::Text(text)
                if text
                    .annotations
                    .as_ref()
                    .and_then(|annotations| annotations.audience.as_ref())
                    .is_none_or(|audiences| {
                        audiences.is_empty() || audiences.contains(&Role::Assistant)
                    }) =>
            {
                Some(text.text.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

#[async_trait]
impl McpClientTrait for CodeExecutionClient {
    async fn list_tools(
        &self,
        _: &str,
        _: Option<String>,
        _: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        fn schema<T: JsonSchema>() -> JsonObject {
            serde_json::to_value(schema_for!(T))
                .expect("valid schema")
                .as_object()
                .expect("object schema")
                .clone()
        }
        let read_only =
            ToolAnnotations::from_raw(None, Some(true), Some(false), Some(true), Some(false));
        Ok(ListToolsResult {
            tools: vec![
                McpTool::new("list_functions", "List callable tools with their exact Goose names and short descriptions. Inspect unfamiliar parameters with get_function_details before calling tools.", json!({"type":"object","properties":{}}).as_object().unwrap().clone()).annotate(read_only.clone()),
                McpTool::new("get_function_details", "Get descriptions and original JSON input/output schemas for exact tool names returned by list_functions. Schemas are data, including recursive references.", schema::<GetFunctionDetailsInput>()).annotate(read_only),
                McpTool::new("execute_javascript", format!("Execute JavaScript and return captured console output, the JSON result, or an error. {INSTRUCTIONS}"), schema::<ExecuteWithToolGraph>()).annotate(ToolAnnotations::from_raw(Some("Execute JavaScript".into()), Some(false), Some(true), Some(false), Some(true))),
            ],
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let result = match name {
            "list_functions" => {
                let catalog =
                    callable_catalog(dispatching_lease(ctx).tools_excluding(EXTENSION_NAME).await);
                let functions: Vec<_> = catalog.iter().map(|tool| json!({
                    "name": tool.name,
                    "description": tool.description.as_deref().unwrap_or("").lines().next().unwrap_or("").chars().take(240).collect::<String>()
                })).collect();
                Ok(vec![ContentBlock::text(json!(functions).to_string())])
            }
            "get_function_details" => {
                let catalog =
                    callable_catalog(dispatching_lease(ctx).tools_excluding(EXTENSION_NAME).await);
                parse_arguments::<GetFunctionDetailsInput>(arguments)
                    .and_then(|input| function_details(&catalog, &input.functions))
                    .map(|details| vec![ContentBlock::text(details.to_string())])
            }
            "execute_javascript" => {
                self.handle_execute_javascript(ctx, arguments, cancellation_token)
                    .await
            }
            _ => Err(format!("Unknown tool: {name}")),
        };
        Ok(match result {
            Ok(content) => CallToolResult::success(content),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(javascript::limit_error(
                format!("Error: {error}"),
            ))]),
        })
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn get_moim(&self, _: &str, tools: &[McpTool]) -> Option<String> {
        Some(format!("{} callable tools are available through execute_javascript in this snapshot. {INSTRUCTIONS}", callable_catalog(tools.to_vec()).len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::extension_manager::ExtensionManager;
    use goose_agent::extension::ExtensionConfig;
    use rmcp::model::{Annotations, EmbeddedResource, MetaObject, ResourceContents, TextContent};

    #[test]
    fn callback_result_ignores_hidden_structured_content_and_meta() {
        let mut result = CallToolResult::success(vec![ContentBlock::text("visible text")]);
        result.structured_content = Some(json!({"hidden": "structured secret"}));
        result.meta = Some(MetaObject(
            json!({"hidden": "metadata secret"})
                .as_object()
                .unwrap()
                .clone(),
        ));

        assert_eq!(
            callback_result_to_value(&result),
            Value::String("visible text".to_string())
        );
    }

    #[test]
    fn callback_result_uses_assistant_visible_text_and_preserves_json_parsing() {
        let user_only = ContentBlock::Text(
            TextContent::new(r#"false,"hidden":"user secret","ignored":"#)
                .with_annotations(Annotations::default().with_audience(vec![Role::User])),
        );
        let assistant_only = ContentBlock::Text(
            TextContent::new("true}")
                .with_annotations(Annotations::default().with_audience(vec![Role::Assistant])),
        );
        let resource = ResourceContents::TextResourceContents {
            uri: "file:///hidden.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
            text: "resource secret".to_string(),
            meta: None,
        };
        let result = CallToolResult::success(vec![
            ContentBlock::text(r#"{"visible":"#),
            user_only,
            ContentBlock::image("image secret", "image/png"),
            ContentBlock::Resource(EmbeddedResource::new(resource)),
            assistant_only,
        ]);

        assert_eq!(callback_result_to_value(&result), json!({"visible": true}));
    }

    struct VisibilityClient;

    struct PendingClient {
        started: Arc<tokio::sync::Notify>,
        released: Arc<tokio::sync::Notify>,
    }

    #[async_trait]
    impl McpClientTrait for PendingClient {
        async fn list_tools(
            &self,
            session_id: &str,
            next_cursor: Option<String>,
            cancellation_token: CancellationToken,
        ) -> Result<ListToolsResult, Error> {
            VisibilityClient
                .list_tools(session_id, next_cursor, cancellation_token)
                .await
        }

        async fn call_tool(
            &self,
            _: &ToolCallContext,
            _: &str,
            _: Option<JsonObject>,
            _: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            struct Released(Arc<tokio::sync::Notify>);
            impl Drop for Released {
                fn drop(&mut self) {
                    self.0.notify_one();
                }
            }
            let _guard = Released(self.released.clone());
            self.started.notify_one();
            std::future::pending().await
        }

        fn get_info(&self) -> Option<&InitializeResult> {
            None
        }
    }

    #[tokio::test]
    async fn cancelled_dispatch_releases_tools_without_polling_the_worker_future() {
        use futures::FutureExt;

        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(temp.path().join("manager")));
        let session = manager
            .get_context()
            .session_manager
            .create_session(
                temp.path().to_path_buf(),
                "blocked-worker".into(),
                crate::session::session_manager::SessionType::Hidden,
                crate::config::GooseMode::default(),
            )
            .await
            .unwrap();
        let started = Arc::new(tokio::sync::Notify::new());
        let released = Arc::new(tokio::sync::Notify::new());
        manager
            .add_test_client(
                &session.id,
                ExtensionConfig::Builtin {
                    name: "pending".into(),
                    description: String::new(),
                    display_name: None,
                    timeout: None,
                    bundled: None,
                    available_tools: vec![],
                },
                Arc::new(PendingClient {
                    started: started.clone(),
                    released: released.clone(),
                }),
                None,
            )
            .await;
        let lease = Arc::new(manager.scope_lease(&session.id).await);
        let ctx = ToolCallContext::new(session.id, None, None).with_extension_lease(lease);
        let dispatch = CancellationToken::new();
        let mut callback = dispatch_tool(
            ctx,
            "pending__ordinary".into(),
            manager.runtime().clone(),
            dispatch.clone(),
            tokio::runtime::Handle::current(),
            json!({}),
        );
        assert!(callback.as_mut().now_or_never().is_none());
        tokio::time::timeout(Duration::from_secs(2), started.notified())
            .await
            .unwrap();
        dispatch.cancel();
        tokio::time::timeout(Duration::from_secs(2), released.notified())
            .await
            .unwrap();
        // The Boa worker could be blocked here: callback has not been polled again.
        drop(callback);
    }

    #[async_trait]
    impl McpClientTrait for VisibilityClient {
        async fn list_tools(
            &self,
            _session_id: &str,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListToolsResult, Error> {
            let app_only = McpTool::new(
                "app_only".to_string(),
                "App-only tool".to_string(),
                JsonObject::new(),
            )
            .with_meta(MetaObject(
                json!({ "ui": { "visibility": ["app"] } })
                    .as_object()
                    .unwrap()
                    .clone(),
            ));
            let model_visible = McpTool::new(
                "model_visible".to_string(),
                "Model-visible tool".to_string(),
                JsonObject::new(),
            )
            .with_output_schema::<ToolGraphNode>()
            .with_meta(MetaObject(
                json!({ "ui": { "visibility": ["model"] } })
                    .as_object()
                    .unwrap()
                    .clone(),
            ));
            let ordinary = McpTool::new(
                "ordinary".to_string(),
                "Ordinary tool".to_string(),
                JsonObject::new(),
            );

            Ok(ListToolsResult {
                tools: vec![app_only, model_visible, ordinary],
                ..Default::default()
            })
        }

        async fn call_tool(
            &self,
            _ctx: &ToolCallContext,
            _name: &str,
            arguments: Option<JsonObject>,
            _cancellation_token: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            if arguments
                .as_ref()
                .is_some_and(|args| args.get("fail") == Some(&json!(true)))
            {
                return Ok(CallToolResult::error(vec![ContentBlock::text(
                    "permission denied",
                )]));
            }
            let mut result = CallToolResult::success(vec![
                ContentBlock::text(
                    json!({"summary": "visible", "arguments": arguments}).to_string(),
                ),
                ContentBlock::Text(
                    TextContent::new("hidden audience secret")
                        .with_annotations(Annotations::default().with_audience(vec![Role::User])),
                ),
            ]);
            result.structured_content = Some(json!({"hidden": "structured secret"}));
            Ok(result)
        }

        fn get_info(&self) -> Option<&InitializeResult> {
            None
        }
    }

    #[tokio::test]
    async fn catalog_excludes_tools_hidden_from_model() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(temp.path().join("manager")));
        manager
            .install_test_client(
                "test-session",
                ExtensionConfig::Builtin {
                    name: "visibility".to_string(),
                    description: "Visibility test tools".to_string(),
                    display_name: None,
                    timeout: None,
                    bundled: None,
                    available_tools: vec![],
                },
                Arc::new(VisibilityClient),
                None,
            )
            .await;

        let lease = manager.scope_lease("test-session").await;
        let configs = callable_catalog(lease.tools_excluding(EXTENSION_NAME).await);
        let names = configs
            .iter()
            .map(|config| config.name.as_ref())
            .collect::<Vec<_>>();

        assert!(!names.contains(&"visibility__app_only"));
        assert!(names.contains(&"visibility__model_visible"));
        assert!(names.contains(&"visibility__ordinary"));
        assert!(configs.iter().any(|config| config.output_schema.is_some()));
    }

    #[tokio::test]
    async fn catalog_leaves_delegate_to_direct_tool_calls() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(temp.path().join("manager")));
        let session = manager
            .get_context()
            .session_manager
            .create_session(
                temp.path().to_path_buf(),
                "code-mode-delegate".to_string(),
                crate::session::session_manager::SessionType::Hidden,
                crate::config::GooseMode::default(),
            )
            .await
            .unwrap();
        manager
            .enable(
                &session.id,
                ExtensionConfig::Platform {
                    name: super::super::summon::EXTENSION_NAME.to_string(),
                    description: String::new(),
                    display_name: None,
                    bundled: None,
                    available_tools: Vec::new(),
                },
            )
            .await
            .unwrap();

        let lease = manager.current_lease(&session.id).await.unwrap();
        let configs = callable_catalog(lease.tools_excluding(EXTENSION_NAME).await);
        let names = configs
            .iter()
            .map(|config| config.name.as_ref())
            .collect::<Vec<_>>();

        assert!(names.contains(&"load"));
        assert!(!names.contains(&"delegate"));
    }

    #[tokio::test]
    async fn callback_uses_lease_and_applies_extension_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(temp.path().join("manager")));
        let session = manager
            .get_context()
            .session_manager
            .create_session(
                temp.path().to_path_buf(),
                "code-mode-mutation".to_string(),
                crate::session::session_manager::SessionType::Hidden,
                crate::config::GooseMode::default(),
            )
            .await
            .unwrap();
        manager
            .enable(
                &session.id,
                ExtensionConfig::Platform {
                    name: "extensionmanager".to_string(),
                    description: String::new(),
                    display_name: None,
                    bundled: None,
                    available_tools: Vec::new(),
                },
            )
            .await
            .unwrap();
        let lease = Arc::new(manager.current_lease(&session.id).await.unwrap());
        manager
            .disable(&session.id, "extensionmanager")
            .await
            .unwrap();
        let tool_name = lease
            .tools()
            .await
            .into_iter()
            .find(|tool| tool.name.ends_with("manage_extensions"))
            .unwrap()
            .name
            .to_string();
        let ctx = ToolCallContext::new(
            session.id.clone(),
            Some(session.working_dir.clone()),
            Some("manage".into()),
        )
        .with_extension_lease(lease);
        let mut context = manager.get_context().clone();
        context.extension_manager = Some(Arc::downgrade(manager.runtime()));
        let client = CodeExecutionClient::new(context).unwrap();
        let code = format!("async function run() {{ return await tools.call({}, {{action: 'enable', extension_name: 'todo'}}); }}", json!(tool_name));
        let result = client
            .call_tool(
                &ctx,
                "execute_javascript",
                Some(json!({"code": code}).as_object().unwrap().clone()),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true), "{result:?}");

        assert!(manager
            .list_extensions(&session.id)
            .await
            .unwrap()
            .contains(&"todo".to_string()));
        let stored_session = manager
            .get_context()
            .session_manager
            .get_session(&session.id, false)
            .await
            .unwrap();
        let stored_extensions = crate::session::EnabledExtensionsState::from_extension_data(
            &stored_session.extension_data,
        )
        .unwrap();
        assert!(stored_extensions
            .extensions
            .iter()
            .any(|config| config.key() == "todo"));
    }
    #[tokio::test]
    async fn discovery_and_execution_share_the_lease_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(temp.path().join("manager")));
        let session = manager
            .get_context()
            .session_manager
            .create_session(
                temp.path().to_path_buf(),
                "snapshot-session".into(),
                crate::session::session_manager::SessionType::Hidden,
                crate::config::GooseMode::default(),
            )
            .await
            .unwrap();
        manager
            .add_test_client(
                &session.id,
                ExtensionConfig::Builtin {
                    name: "visibility".into(),
                    description: String::new(),
                    display_name: None,
                    timeout: None,
                    bundled: None,
                    available_tools: vec![],
                },
                Arc::new(VisibilityClient),
                None,
            )
            .await;
        let lease = Arc::new(manager.scope_lease(&session.id).await);
        let ctx = ToolCallContext::new(session.id.clone(), None, None)
            .with_extension_lease(lease.clone());
        let mut context = manager.get_context().clone();
        context.extension_manager = Some(Arc::downgrade(manager.runtime()));
        let client = CodeExecutionClient::new(context).unwrap();
        let listed = client
            .call_tool(&ctx, "list_functions", None, CancellationToken::new())
            .await
            .unwrap();
        let listed = callback_result_to_value(&listed);
        assert!(listed
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "visibility__ordinary"));
        assert!(!listed.to_string().contains("app_only"));
        manager.disable(&session.id, "visibility").await.unwrap();
        let details = client
            .call_tool(
                &ctx,
                "get_function_details",
                Some(
                    json!({"functions": ["visibility__model_visible"]})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let details = callback_result_to_value(&details);
        assert!(details[0]["output_schema"].is_object());
        let result = client.call_tool(&ctx, "execute_javascript", Some(json!({"code": r#"async function run() {
            const first = await tools.call("visibility__ordinary", {query: "first"});
            const batch = await Promise.all([tools.call("visibility__ordinary", {query: first.summary}), tools.call("visibility__model_visible", {})]);
            try { await tools.call("visibility__ordinary", {fail: true}); } catch (e) { console.log(e.message); }
            try { await tools.call("visibility__app_only", {}); } catch (e) { console.log(e.message); }
            return batch;
        }"#}).as_object().unwrap().clone()), CancellationToken::new()).await.unwrap();
        assert_ne!(result.is_error, Some(true), "{result:?}");
        let output = callback_result_to_value(&result).to_string();
        assert!(output.contains("visible"));
        assert!(output.contains("permission denied"));
        assert!(output.contains("list_functions"));
        assert!(!output.contains("secret"));
        assert!(callable_catalog(
            manager
                .current_lease(&session.id)
                .await
                .unwrap()
                .tools()
                .await
        )
        .iter()
        .all(|tool| !tool.name.starts_with("visibility__")));
    }

    #[test]
    fn recursive_schemas_and_names_are_preserved_as_data() {
        let recursive = json!({"$ref": "#/$defs/Any", "$defs": {"Any": {"anyOf": [{"type": "string"}, {"type": "object", "additionalProperties": {"$ref": "#/$defs/Any"}}]}}});
        let mut tool = McpTool::new(
            "Unusual-name__snake_case",
            "A description",
            JsonObject::new(),
        );
        tool.output_schema = Some(Arc::new(recursive.as_object().unwrap().clone()));
        let details = function_details(&[tool], &["Unusual-name__snake_case".into()]).unwrap();
        assert_eq!(details[0]["output_schema"], recursive);
        assert_eq!(details[0]["name"], "Unusual-name__snake_case");
    }

    #[tokio::test]
    async fn moim_uses_supplied_snapshot_and_advertises_only_three_tools() {
        let temp = tempfile::tempdir().unwrap();
        let manager = ExtensionManager::with_data_dir(temp.path().join("manager"));
        let client = CodeExecutionClient::new(manager.get_context().clone()).unwrap();
        let tool = McpTool::new("leased__tool", "Leased tool", JsonObject::new());
        assert!(client
            .get_moim("session", &[tool])
            .await
            .unwrap()
            .contains("1 callable tools"));
        assert!(client
            .get_moim("session", &[])
            .await
            .unwrap()
            .contains("0 callable tools"));
        let names = client
            .list_tools("session", None, CancellationToken::new())
            .await
            .unwrap()
            .tools
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "list_functions",
                "get_function_details",
                "execute_javascript"
            ]
        );
    }
    #[test]
    fn catalog_excludes_resources_code_mode_and_delegation() {
        let make = |name, meta: Value| {
            McpTool::new(name, "description", JsonObject::new())
                .with_meta(MetaObject(meta.as_object().unwrap().clone()))
        };
        let tools = vec![
            make("shell", json!({"goose_extension": "developer"})),
            make(
                "execute_javascript",
                json!({"goose_extension": "code_execution"}),
            ),
            make("delegate", json!({"goose_extension": "summon"})),
            make(
                "resource__view",
                json!({"ui": {"resourceUri": "ui://example"}}),
            ),
            make("hidden__tool", json!({"ui": {"visibility": ["app"]}})),
        ];
        let names = callable_catalog(tools)
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, ["shell"]);
    }
}
