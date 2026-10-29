use super::*;
use crate::extension::check_unique_keys;
use crate::extension_manager::host::{ExtensionAuth, PlatformExtension, StaticOAuthClientConfig};
use crate::tool_execution::ToolCallContext;
use async_trait::async_trait;
use rmcp::model::{InitializeResult, JsonObject, ListToolsResult};
use rmcp::transport::auth::{CredentialStore, InMemoryCredentialStore};
use rmcp::transport::AuthorizationManager;
use rmcp::ServiceError as Error;
use std::sync::atomic::AtomicUsize;
use tokio::sync::Semaphore;

struct TestHost {
    session: Mutex<ExtensionSession>,
    action_required: Arc<ActionRequiredManager>,
}

impl TestHost {
    fn new() -> Self {
        Self {
            session: Mutex::new(ExtensionSession {
                id: "session".to_string(),
                working_dir: PathBuf::new(),
                container: None,
                extensions: vec![],
                provider_runs_tool_loop: false,
            }),
            action_required: Arc::new(ActionRequiredManager::new()),
        }
    }
}

#[async_trait]
impl ExtensionHost for TestHost {
    async fn session(&self, session_id: &str) -> Result<ExtensionSession> {
        let session = self.session.lock().await;
        anyhow::ensure!(session.id == session_id, "unknown session");
        Ok(session.clone())
    }

    async fn select(&self, _session_id: &str, config: ExtensionConfig) -> ExtensionResult<()> {
        let mut session = self.session.lock().await;
        match session
            .extensions
            .iter_mut()
            .find(|selected| selected.key() == config.key())
        {
            Some(selected) => *selected = config,
            None => session.extensions.push(config),
        }
        check_unique_keys(&session.extensions)
            .map_err(|error| ExtensionError::ConfigError(error.to_string()))
    }

    async fn disable(&self, _session_id: &str, key: &str) -> ExtensionResult<bool> {
        let mut session = self.session.lock().await;
        let selected = session.extensions.len();
        session.extensions.retain(|config| config.key() != key);
        Ok(session.extensions.len() != selected)
    }

    async fn resolve_config(&self, config: ExtensionConfig) -> ExtensionResult<ExtensionConfig> {
        Ok(config)
    }

    fn extension_by_name(&self, _name: &str) -> Option<ExtensionConfig> {
        None
    }
    fn action_required(&self) -> Arc<ActionRequiredManager> {
        Arc::clone(&self.action_required)
    }
    fn auth(&self) -> Arc<dyn ExtensionAuth> {
        Arc::new(NoAuth)
    }

    fn platform_extension(&self, name: &str) -> Option<PlatformExtension> {
        (name == "developer").then_some(PlatformExtension {
            unprefixed_tools: true,
        })
    }
}

struct NoAuth;

#[async_trait]
impl ExtensionAuth for NoAuth {
    fn credential_store(&self, _name: &str) -> Box<dyn CredentialStore> {
        Box::new(InMemoryCredentialStore::new())
    }

    async fn authorize(
        &self,
        _uri: &str,
        _name: &str,
        _static_client: Option<&StaticOAuthClientConfig>,
        _challenge: Option<String>,
    ) -> Result<AuthorizationManager> {
        anyhow::bail!("OAuth is not available in this test")
    }
}

fn manager() -> Arc<ExtensionManager> {
    Arc::new(ExtensionManager::with_host(
        Arc::new(TestHost::new()),
        ClientContext {
            name: "test".to_string(),
            capabilities: GooseMcpClientCapabilities {
                mcpui: false,
                host_info: None,
                elicitation_handler: None,
                protocol_version: None,
            },
        },
    ))
}

fn builtin_config(name: &str) -> ExtensionConfig {
    ExtensionConfig::Builtin {
        name: name.to_string(),
        display_name: None,
        description: String::new(),
        timeout: None,
        bundled: None,
        available_tools: vec![],
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
            Arc::new(BlockingToolsClient {
                calls: AtomicUsize::new(1),
                first_fetch_started: Semaphore::new(0),
                release_first_fetch: Semaphore::new(0),
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
            Arc::new(BlockingToolsClient {
                calls: AtomicUsize::new(1),
                first_fetch_started: Semaphore::new(0),
                release_first_fetch: Semaphore::new(0),
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
