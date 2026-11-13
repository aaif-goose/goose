use super::*;
use crate::agents::extension_runtime::tool_execution::ToolCallContext;
use async_trait::async_trait;
use rmcp::model::{InitializeResult, JsonObject, ListToolsResult};
use rmcp::ServiceError as Error;
use std::sync::atomic::AtomicUsize;
use tokio::sync::Semaphore;

struct TestSelectionStore {
    session: Mutex<ExtensionSession>,
}

impl TestSelectionStore {
    fn new() -> Self {
        Self {
            session: Mutex::new(ExtensionSession {
                id: "session".to_string(),
                working_dir: PathBuf::new(),
                extensions: vec![],
            }),
        }
    }
}

#[async_trait]
impl ExtensionSelectionStore for TestSelectionStore {
    async fn session(&self, session_id: &str) -> Result<ExtensionSession> {
        let session = self.session.lock().await;
        anyhow::ensure!(session.id == session_id, "unknown session");
        Ok(session.clone())
    }

    async fn select(&self, _session_id: &str, config: Registration) -> ExtensionResult<()> {
        let mut session = self.session.lock().await;
        match session
            .extensions
            .iter_mut()
            .find(|selected| selected.key() == config.key())
        {
            Some(selected) => *selected = config,
            None => session.extensions.push(config),
        }
        Ok(())
    }

    async fn disable(&self, _session_id: &str, key: &str) -> ExtensionResult<bool> {
        let mut session = self.session.lock().await;
        let selected = session.extensions.len();
        session.extensions.retain(|config| config.key() != key);
        Ok(session.extensions.len() != selected)
    }
}

fn manager() -> Arc<ExtensionManager> {
    Arc::new(ExtensionManager::with_services(
        {
            let mut services = host::ExtensionServices::new(Arc::new(ActionRequiredManager::new()));
            services.selections = Some(Arc::new(TestSelectionStore::new()));
            services
        },
        ClientContext {
            name: "test".to_string(),
            capabilities: GooseMcpClientCapabilities {
                action_required_elicitation: true,
                ..Default::default()
            },
        },
    ))
}

struct UnavailableConnector;
#[async_trait]
impl Connector for UnavailableConnector {
    async fn connect(
        &self,
        _: &ConnectRequest,
    ) -> ExtensionResult<Option<Box<dyn McpClientTrait>>> {
        Err(ExtensionError::ConfigError(
            "unavailable test connector".to_string(),
        ))
    }
}
fn builtin_config(name: &str) -> Registration {
    let registration = Registration::new(name, Arc::new(UnavailableConnector));
    if name == "developer" {
        registration.trusted().location_independent().unprefixed()
    } else {
        registration
    }
}

struct BlockingToolsClient {
    calls: AtomicUsize,
    first_fetch_started: Semaphore,
    release_first_fetch: Semaphore,
}

#[async_trait::async_trait]
impl McpClientTrait for BlockingToolsClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let name = if call == 0 { "old" } else { "new" };

        if call == 0 {
            self.first_fetch_started.add_permits(1);
            let _permit = self.release_first_fetch.acquire().await.unwrap();
        }

        Ok(ListToolsResult {
            tools: vec![Tool::new(
                name,
                format!("{name} tool list"),
                Arc::new(JsonObject::new()),
            )],
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        _name: &str,
        _arguments: Option<JsonObject>,
        _cancel_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        Ok(CallToolResult::success(vec![]))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        None
    }
}

#[tokio::test]
async fn tool_list_changed_during_fetch_prevents_stale_cache() {
    let extension_manager = manager();
    let tools_client = Arc::new(BlockingToolsClient {
        calls: AtomicUsize::new(0),
        first_fetch_started: Semaphore::new(0),
        release_first_fetch: Semaphore::new(0),
    });
    extension_manager
        .add_client(
            "session",
            builtin_config("dynamic"),
            tools_client.clone(),
            None,
        )
        .await;
    let slot = extension_manager.scopes.lock().await["session"]["dynamic"].clone();
    let tools_version = slot.start().await.unwrap().tools_version.clone();

    let manager = extension_manager;
    let first_fetch = {
        let manager = manager.clone();
        tokio::spawn(async move { manager.scope_lease("session").await.tools().await })
    };

    let _started = tools_client.first_fetch_started.acquire().await.unwrap();
    tools_version.fetch_add(1, Ordering::SeqCst);
    tools_client.release_first_fetch.add_permits(1);

    let stale_result = first_fetch.await.unwrap();
    assert!(stale_result.iter().any(|tool| tool.name == "dynamic__old"));

    let refreshed = manager.scope_lease("session").await.tools().await;
    assert!(refreshed.iter().any(|tool| tool.name == "dynamic__new"));
    assert_eq!(tools_client.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn test_recover_mangled_tool_name() {
    let tools = [("developer__shell", None), ("platform__search", None)];
    assert_eq!(
        recover_mangled_tool_name("developer.shell", tools.iter().copied()).as_deref(),
        Some("developer__shell")
    );
    assert_eq!(
        recover_mangled_tool_name("functions.developer__shell", tools.iter().copied()).as_deref(),
        Some("developer__shell")
    );
    assert_eq!(
        recover_mangled_tool_name("functions.developer.shell", tools.iter().copied()).as_deref(),
        Some("developer__shell")
    );
    assert_eq!(
        recover_mangled_tool_name("developer shell", tools.iter().copied()),
        None
    );
    assert_eq!(
        recover_mangled_tool_name("developer__shell!", tools.iter().copied()),
        None
    );
    assert_eq!(
        recover_mangled_tool_name("nonexistent.tool", tools.iter().copied()),
        None
    );

    let dotted_tool = [("dotted__db.query", None)];
    assert_eq!(
        recover_mangled_tool_name("dotted.db.query", dotted_tool.iter().copied()).as_deref(),
        Some("dotted__db.query")
    );
}

#[test]
fn test_recover_mangled_tool_name_unprefixed_extension() {
    // Platform extensions with unprefixed_tools=true (e.g. "developer")
    // advertise tools with no "__" prefix at all; the owner lives only in
    // metadata. GLM's documented "developer.shell" reproduction (#9486)
    // and emulated "developer__shell" calls must recover via the owner,
    // not the tool's own (absent) prefix.
    let tools = [("shell", Some("developer")), ("write", Some("developer"))];
    assert_eq!(
        recover_mangled_tool_name("developer.shell", tools.iter().copied()).as_deref(),
        Some("shell")
    );
    assert_eq!(
        recover_mangled_tool_name("developer__shell", tools.iter().copied()).as_deref(),
        Some("shell")
    );
    assert_eq!(
        recover_mangled_tool_name("functions.developer.shell", tools.iter().copied()).as_deref(),
        Some("shell")
    );

    // Wrong owner must not match.
    assert_eq!(
        recover_mangled_tool_name("other_extension.shell", tools.iter().copied()),
        None
    );

    // Ambiguity across two different unprefixed extensions that both own
    // a tool matching the same mangled input must refuse, not guess.
    let ambiguous = [("shell", Some("dev_a")), ("shell", Some("dev_b"))];
    assert_eq!(
        recover_mangled_tool_name("dev_a.shell", ambiguous.iter().copied()).as_deref(),
        Some("shell")
    );
}

#[test]
fn test_recover_mangled_tool_name_non_extension_manager_tools() {
    // recipe__final_output and platform__manage_schedule are appended by
    // Agent::list_tools outside the extension manager (see #9486); they
    // use the same "__" convention, so no owner metadata is needed.
    let tools = [
        ("recipe__final_output", None),
        ("platform__manage_schedule", None),
    ];
    assert_eq!(
        recover_mangled_tool_name("recipe.final_output", tools.iter().copied()).as_deref(),
        Some("recipe__final_output")
    );
    assert_eq!(
        recover_mangled_tool_name("platform.manage_schedule", tools.iter().copied()).as_deref(),
        Some("platform__manage_schedule")
    );
}

#[tokio::test]
async fn a_new_working_dir_restarts_only_extensions_that_run_in_it() {
    let manager = manager();
    let developer = builtin_config("developer");
    let external = builtin_config("external");
    let builtin = builtin_config("builtin");
    manager
        .add_client(
            "session",
            external.clone(),
            Arc::new(StaticToolsClient {
                tools: Some(vec!["new"]),
            }),
            None,
        )
        .await;
    let extensions = vec![developer, external, builtin];
    manager
        .resolve(
            &ExtensionSet::new("session", Some(PathBuf::from("old")), extensions.clone()).unwrap(),
        )
        .await;
    let before = manager.scopes.lock().await["session"].clone();

    let new_working_dir = PathBuf::from("new");
    let lease = manager
        .resolve(&ExtensionSet::new("session", Some(new_working_dir.clone()), extensions).unwrap())
        .await;
    let after = manager.scopes.lock().await["session"].clone();
    for key in ["developer", "external"] {
        assert!(
            Arc::ptr_eq(&before[key], &after[key]),
            "{key} was restarted"
        );
    }
    assert!(!Arc::ptr_eq(&before["builtin"], &after["builtin"]));
    assert_eq!(lease.working_dir(), Some(new_working_dir.as_path()));
}

#[tokio::test]
async fn host_backed_selection_keeps_held_leases_unchanged() {
    let manager = manager();
    manager
        .add_client(
            "session",
            builtin_config("dynamic"),
            Arc::new(StaticToolsClient {
                tools: Some(vec!["new"]),
            }),
            None,
        )
        .await;
    let before = manager.current_lease("session").await.unwrap();
    assert_eq!(
        manager.list_extensions("session").await.unwrap(),
        vec!["dynamic"]
    );

    assert!(manager.disable("session", "dynamic").await.unwrap());
    let after = manager.current_lease("session").await.unwrap();
    assert!(after.tools().await.is_empty());
    assert_eq!(before.tools().await[0].name, "dynamic__new");
    let result = before
        .call(
            rmcp::model::CallToolRequestParams::new("dynamic__new"),
            CallRequest::default(),
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .result
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true));
}

#[tokio::test]
async fn explicit_sets_need_no_session_lookup() {
    let manager = Arc::new(ExtensionManager::with_services(
        host::ExtensionServices::new(Arc::new(ActionRequiredManager::new())),
        ClientContext {
            name: "minimal".to_string(),
            capabilities: Default::default(),
        },
    ));
    let registration = Registration::new("custom", StaticConnector::new(Some(vec!["echo"])));
    let lease = manager
        .resolve(&ExtensionSet::new("scope", None, vec![registration]).unwrap())
        .await;
    assert_eq!(tool_names(lease.tools().await), ["custom__echo"]);
    assert!(matches!(
        manager.selection_store(),
        Err(ExtensionError::ConfigError(_))
    ));
}

/// Lists fixed tools, or fails discovery when `tools` is `None`.
struct StaticToolsClient {
    tools: Option<Vec<&'static str>>,
}

#[async_trait::async_trait]
impl McpClientTrait for StaticToolsClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let Some(tools) = &self.tools else {
            return Err(Error::UnexpectedResponse);
        };
        Ok(ListToolsResult {
            tools: tools
                .iter()
                .map(|name| Tool::new(*name, "", Arc::new(JsonObject::new())))
                .collect(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        _name: &str,
        _arguments: Option<JsonObject>,
        _cancel_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        Ok(CallToolResult::success(vec![]))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        None
    }
}

async fn add_static(manager: &ExtensionManager, name: &str, tools: Option<Vec<&'static str>>) {
    manager
        .add_client(
            "session",
            builtin_config(name),
            Arc::new(StaticToolsClient { tools }),
            None,
        )
        .await;
}

fn tool_names(tools: Vec<Tool>) -> Vec<String> {
    tools
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect()
}

#[tokio::test]
async fn failed_discovery_omits_only_the_failed_extension() {
    let manager = manager();
    add_static(&manager, "good", Some(vec!["tool"])).await;
    add_static(&manager, "broken", None).await;
    let lease = manager.scope_lease("session").await;

    assert_eq!(tool_names(lease.tools().await), ["good__tool"]);
    assert_eq!(lease.ready().await.len(), 2);
}

#[tokio::test]
async fn failed_start_is_reported_without_failing_the_catalog() {
    let manager = manager();
    let lease = manager
        .resolve(&ExtensionSet::new("scope", None, vec![builtin_config("missing")]).unwrap())
        .await;
    let results = lease.start().await;
    assert!(results[0].error.is_some());
    assert!(lease.tools().await.is_empty());
}

#[tokio::test]
async fn colliding_tool_names_keep_the_first_extension() {
    // `developer` is unprefixed, so its `other__tool` collides with `other`'s `tool`.
    let manager = manager();
    add_static(&manager, "developer", Some(vec!["other__tool"])).await;
    add_static(&manager, "other", Some(vec!["tool"])).await;
    let lease = manager.scope_lease("session").await;
    let tools = lease.tools().await;
    assert_eq!(tool_names(tools.clone()), ["other__tool"]);
    assert_eq!(get_tool_owner(&tools[0]).as_deref(), Some("developer"));
}

/// Hands out `StaticToolsClient`s, counting connections; declines when `tools` is `None`.
struct StaticConnector {
    tools: Option<Vec<&'static str>>,
    connects: AtomicUsize,
}

impl StaticConnector {
    fn new(tools: Option<Vec<&'static str>>) -> Arc<Self> {
        Arc::new(Self {
            tools,
            connects: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl Connector for StaticConnector {
    async fn connect(
        &self,
        request: &ConnectRequest,
    ) -> ExtensionResult<Option<Box<dyn McpClientTrait>>> {
        assert_eq!(request.scope_id, "scope");
        self.connects.fetch_add(1, Ordering::SeqCst);
        Ok(self.tools.clone().map(|tools| {
            Box::new(StaticToolsClient { tools: Some(tools) }) as Box<dyn McpClientTrait>
        }))
    }
}

#[tokio::test]
async fn host_registrations_connect_through_their_connector() {
    let manager = manager();
    let connector = StaticConnector::new(Some(vec!["echo", "hidden"]));
    let registration = Registration::new("custom", connector.clone())
        .with_available_tools(vec!["echo".to_string()]);
    let lease = manager
        .resolve(&ExtensionSet::new("scope", None, vec![registration.clone()]).unwrap())
        .await;

    assert_eq!(tool_names(lease.tools().await), ["custom__echo"]);
    let result = lease
        .call(
            rmcp::model::CallToolRequestParams::new("custom__echo"),
            CallRequest::default(),
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .result
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true));

    // The same revision reuses the connection; a new one reconnects.
    let same = manager
        .resolve(&ExtensionSet::new("scope", None, vec![registration.clone()]).unwrap())
        .await;
    same.tools().await;
    assert_eq!(connector.connects.load(Ordering::SeqCst), 1);
    let bumped = manager
        .resolve(&ExtensionSet::new("scope", None, vec![registration.with_revision(1)]).unwrap())
        .await;
    bumped.tools().await;
    assert_eq!(connector.connects.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn declining_connectors_contribute_no_tools_without_failing() {
    let manager = manager();
    let lease = manager
        .resolve(
            &ExtensionSet::new(
                "scope",
                None,
                vec![
                    Registration::new("declined", StaticConnector::new(None)),
                    Registration::new("first", StaticConnector::new(Some(vec!["tool"])))
                        .unprefixed(),
                ],
            )
            .unwrap(),
        )
        .await;
    assert_eq!(tool_names(lease.tools().await), ["tool"]);
    assert!(lease.start().await.iter().all(|result| result.success));
}

#[test]
fn registration_keys_must_be_unique_within_a_set() {
    let set = ExtensionSet::new("scope", None, vec![builtin_config("dup")]).unwrap();
    assert!(set
        .with_registration(Registration::new("dup", StaticConnector::new(None)))
        .is_err());
}

#[test]
fn reserved_result_metadata_is_removed_without_touching_other_keys() {
    let mut result = CallToolResult::success(vec![]);
    result.meta = Some(MetaObject(
        serde_json::from_value(serde_json::json!({
            "host_command": {"action": "enable"}, "other": 1
        }))
        .unwrap(),
    ));
    remove_reserved_meta(&mut result, &["host_command"]);
    assert_eq!(
        result.meta.unwrap().0,
        serde_json::from_value::<rmcp::model::JsonObject>(serde_json::json!({"other": 1})).unwrap()
    );

    let mut result = CallToolResult::success(vec![]);
    result.meta = Some(MetaObject(
        serde_json::from_value(serde_json::json!({
            "host_command": true
        }))
        .unwrap(),
    ));
    remove_reserved_meta(&mut result, &["host_command"]);
    assert!(result.meta.is_none());
}

struct ResourceVisibilityPolicy;

#[async_trait]
impl host::ExtensionToolPolicy for ResourceVisibilityPolicy {
    fn exposes_tool(&self, _extension: &str, tool: &str, catalog_has_resources: bool) -> bool {
        tool != "read_resource" || catalog_has_resources
    }
}

#[tokio::test]
async fn resource_visibility_is_independent_of_tool_discovery() {
    for (resource_tools, has_resources) in [
        (None, true),
        (Some(vec![]), true),
        (Some(vec!["hidden"]), true),
        (None, false),
    ] {
        let manager = ExtensionManager::with_services(
            {
                let mut services =
                    host::ExtensionServices::new(Arc::new(ActionRequiredManager::new()));
                services.tool_policy = Arc::new(ResourceVisibilityPolicy);
                services
            },
            ClientContext {
                name: "test".to_string(),
                capabilities: Default::default(),
            },
        );
        manager
            .install_client(
                "session",
                builtin_config("reader"),
                Arc::new(StaticToolsClient {
                    tools: Some(vec!["read_resource"]),
                }),
                None,
            )
            .await;
        let info = has_resources.then(|| {
            InitializeResult::new(
                rmcp::model::ServerCapabilities::builder()
                    .enable_resources()
                    .enable_tools()
                    .build(),
            )
        });
        let resource_config =
            builtin_config("resources").with_available_tools(vec!["not_advertised".into()]);
        manager
            .install_client(
                "session",
                resource_config,
                Arc::new(StaticToolsClient {
                    tools: resource_tools,
                }),
                info,
            )
            .await;
        let tools = manager.scope_lease("session").await.tools().await;
        assert_eq!(
            tools
                .iter()
                .any(|tool| tool.name == "reader__read_resource"),
            has_resources
        );
        assert!(tools
            .iter()
            .all(|tool| !tool.name.starts_with("resources__")));
    }
}
