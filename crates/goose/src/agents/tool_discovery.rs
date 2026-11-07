//! Progressive tool discovery. A session that defers tools lists the tools of
//! its MCP extensions by name instead of sending their schemas, and the model
//! loads the ones it needs with `load_tools`. A load is recorded in the
//! session's [`ToolDiscoveryState`], not in the conversation, so compaction
//! and resume keep it.

use std::collections::{BTreeMap, HashSet};

use rmcp::model::Tool;
use serde::Serialize;
use serde_json::Value;

use crate::agents::extension_manager::{get_tool_owner, recover_mangled_tool_name, ExtensionLease};
use crate::session::{Session, ToolDiscoveryState};

/// A tool sets `_meta["goose/alwaysLoad"]: true` to be sent up front in a
/// session that defers tools.
pub const ALWAYS_LOAD_META_KEY: &str = "goose/alwaysLoad";
pub const LOAD_TOOLS_TOOL_NAME: &str = "load_tools";
const ALL_TOOLS_SUFFIX: &str = "__*";
/// OpenAI rejects requests with more tools than this, so a load never takes a
/// session past it.
pub const MAX_ADVERTISED_TOOLS: usize = 128;

pub fn always_loads(tool: &Tool) -> bool {
    tool.meta
        .as_ref()
        .and_then(|meta| meta.0.get(ALWAYS_LOAD_META_KEY))
        .and_then(Value::as_bool)
        == Some(true)
}

/// Code mode already keeps extension schemas out of the request, behind its
/// own catalog, so tools are not deferred while it is on.
fn code_mode_enabled(lease: &ExtensionLease) -> bool {
    #[cfg(feature = "code-mode")]
    {
        lease.is_enabled(crate::agents::platform_extensions::code_execution::EXTENSION_NAME)
    }
    #[cfg(not(feature = "code-mode"))]
    {
        let _ = lease;
        false
    }
}

pub(crate) fn defers_tools(session: &Session, lease: &ExtensionLease) -> bool {
    ToolDiscoveryState::of(session).defer_tools && !code_mode_enabled(lease)
}

/// The tools the model sees only by name: deferrable and not loaded yet.
/// Empty for a session that doesn't defer tools.
pub(crate) async fn unloaded_tools(session: &Session, lease: &ExtensionLease) -> Vec<Tool> {
    if !defers_tools(session, lease) {
        return Vec::new();
    }
    let state = ToolDiscoveryState::of(session);
    lease
        .deferrable_tools()
        .await
        .into_iter()
        .filter(|tool| !state.loaded_tools.iter().any(|name| name == &*tool.name))
        .collect()
}

/// The order a deferring session's loaded tools follow the rest in, so a load
/// appends to the tools sent instead of reordering them. Empty for a session
/// that doesn't defer tools.
pub(crate) fn admission_order(session: &Session, lease: &ExtensionLease) -> Vec<String> {
    if !defers_tools(session, lease) {
        return Vec::new();
    }
    ToolDiscoveryState::of(session).loaded_tools
}

/// Tool names grouped by the extension that owns them.
pub(crate) fn names_by_extension(tools: &[Tool]) -> BTreeMap<String, Vec<String>> {
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for tool in tools {
        if let Some(owner) = get_tool_owner(tool) {
            grouped
                .entry(owner)
                .or_default()
                .push(tool.name.to_string());
        }
    }
    for names in grouped.values_mut() {
        names.sort();
    }
    grouped
}

/// The tools a deferring session sends only once loaded, loaded or not.
pub(crate) async fn deferred_tools(session: &Session, lease: &ExtensionLease) -> Vec<Tool> {
    if !defers_tools(session, lease) {
        return Vec::new();
    }
    lease.deferrable_tools().await
}

/// The deferred tool a call names, including the mangled forms some models
/// emit, such as `github.create_pr` or `functions.github__create_pr`. The
/// provider layer only recovers names it advertised, which deferred tools
/// weren't.
pub(crate) fn deferred_tool_named(emitted: &str, deferred: &[Tool]) -> Option<String> {
    if deferred.iter().any(|tool| tool.name == emitted) {
        return Some(emitted.to_string());
    }
    let owners: Vec<_> = deferred
        .iter()
        .map(|tool| (tool.name.as_ref(), get_tool_owner(tool)))
        .collect();
    recover_mangled_tool_name(
        emitted,
        owners.iter().map(|(name, owner)| (*name, owner.as_deref())),
    )
}

/// Answers a call to a deferred tool that wasn't sent with the response that
/// called it. `loaded` means a `load_tools` call in the same response loaded it.
pub(crate) fn not_loaded_message(tool_name: &str, loaded: bool) -> String {
    if loaded {
        format!(
            "Tool '{tool_name}' did not run because it was loaded in the same response. \
             Call it again in your next response."
        )
    } else {
        format!(
            "Tool '{tool_name}' is not loaded. Call {LOAD_TOOLS_TOOL_NAME} with tool_names \
             [\"{tool_name}\"], then call it in your next response."
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    UnknownTool,
    ToolLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rejected {
    pub name: String,
    pub reason: RejectReason,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct LoadOutcome {
    pub loaded: Vec<String>,
    pub rejected: Vec<Rejected>,
}

/// Resolves a `load_tools` request against the session's tools.
///
/// `available` is every extension tool the session can call, `deferrable` the
/// subset sent only when loaded, `loaded` the names loaded so far, and
/// `advertised` how many tools the next request carries without this load,
/// operation tools such as `load_tools` included. Returns what to tell the
/// model and the names to add to the loaded set. A load that would take the
/// request past [`MAX_ADVERTISED_TOOLS`] loads nothing.
pub(crate) fn resolve_load(
    requested: &[String],
    available: &[Tool],
    deferrable: &[Tool],
    loaded: &[String],
    advertised: usize,
) -> (LoadOutcome, Vec<String>) {
    let deferrable_names: HashSet<&str> = deferrable.iter().map(|tool| &*tool.name).collect();
    let is_loaded = |name: &str| loaded.iter().any(|loaded| loaded == name);
    let mut outcome = LoadOutcome::default();
    let mut newly_loaded: Vec<String> = Vec::new();

    for request in requested {
        let matches: Vec<&str> = match request.strip_suffix(ALL_TOOLS_SUFFIX) {
            Some(extension) => available
                .iter()
                .filter(|tool| get_tool_owner(tool).as_deref() == Some(extension))
                .map(|tool| &*tool.name)
                .collect(),
            None => available
                .iter()
                .filter(|tool| tool.name == request.as_str())
                .map(|tool| &*tool.name)
                .collect(),
        };
        if matches.is_empty() {
            outcome.rejected.push(Rejected {
                name: request.clone(),
                reason: RejectReason::UnknownTool,
            });
            continue;
        }
        for name in matches {
            if deferrable_names.contains(name)
                && !is_loaded(name)
                && !newly_loaded.iter().any(|loaded| loaded == name)
            {
                newly_loaded.push(name.to_string());
            }
            if !outcome.loaded.iter().any(|loaded| loaded == name) {
                outcome.loaded.push(name.to_string());
            }
        }
    }

    if advertised + newly_loaded.len() > MAX_ADVERTISED_TOOLS {
        let rejected = std::mem::take(&mut outcome.loaded)
            .into_iter()
            .map(|name| Rejected {
                name,
                reason: RejectReason::ToolLimit,
            });
        outcome.rejected.extend(rejected);
        return (outcome, Vec::new());
    }
    (outcome, newly_loaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{JsonObject, MetaObject};
    use serde_json::json;
    use std::sync::Arc;

    fn tool(owner: &str, name: &str) -> Tool {
        let mut tool = Tool::new(name.to_string(), "", Arc::new(JsonObject::new()));
        let mut meta = JsonObject::new();
        meta.insert("goose_extension".to_string(), json!(owner));
        tool.meta = Some(MetaObject(meta));
        tool
    }

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn always_load_reads_the_goose_meta_key() {
        let mut eager = tool("buzz", "buzz__send");
        eager
            .meta
            .as_mut()
            .unwrap()
            .0
            .insert(ALWAYS_LOAD_META_KEY.to_string(), json!(true));
        let mut searched = tool("buzz", "buzz__watch_add");
        searched
            .meta
            .as_mut()
            .unwrap()
            .0
            .insert(ALWAYS_LOAD_META_KEY.to_string(), json!(false));
        let mut other_vendor = tool("buzz", "buzz__react");
        other_vendor
            .meta
            .as_mut()
            .unwrap()
            .0
            .insert("anthropic/alwaysLoad".to_string(), json!(true));

        assert!(always_loads(&eager));
        assert!(!always_loads(&searched));
        assert!(!always_loads(&other_vendor));
        assert!(!always_loads(&tool("buzz", "buzz__read")));
    }

    #[test]
    fn loads_exact_names_and_whole_extensions() {
        let available = vec![
            tool("github", "github__create_pr"),
            tool("github", "github__list_prs"),
            tool("linear", "linear__get_issue"),
        ];
        let (outcome, newly) = resolve_load(
            &names(&["linear__get_issue", "github__*"]),
            &available,
            &available,
            &[],
            0,
        );
        assert_eq!(
            outcome.loaded,
            names(&["linear__get_issue", "github__create_pr", "github__list_prs"])
        );
        assert!(outcome.rejected.is_empty());
        assert_eq!(newly, outcome.loaded);
    }

    #[test]
    fn already_available_tools_load_without_changing_state() {
        let eager = tool("developer", "shell");
        let deferred = tool("linear", "linear__get_issue");
        let available = vec![eager, deferred.clone()];
        let (outcome, newly) = resolve_load(
            &names(&["shell", "linear__get_issue"]),
            &available,
            &[deferred],
            &names(&["linear__get_issue"]),
            2,
        );
        assert_eq!(outcome.loaded, names(&["shell", "linear__get_issue"]));
        assert!(newly.is_empty());
    }

    #[test]
    fn unknown_names_are_rejected_and_the_rest_load() {
        let available = vec![tool("linear", "linear__get_issue")];
        let (outcome, newly) = resolve_load(
            &names(&["linear__get_issue", "linear__nope", "slack__*"]),
            &available,
            &available,
            &[],
            0,
        );
        assert_eq!(outcome.loaded, names(&["linear__get_issue"]));
        assert_eq!(
            outcome.rejected,
            vec![
                Rejected {
                    name: "linear__nope".to_string(),
                    reason: RejectReason::UnknownTool,
                },
                Rejected {
                    name: "slack__*".to_string(),
                    reason: RejectReason::UnknownTool,
                },
            ]
        );
        assert_eq!(newly, names(&["linear__get_issue"]));
    }

    #[test]
    fn a_load_past_the_tool_limit_loads_nothing() {
        let deferred = vec![
            tool("github", "github__create_pr"),
            tool("github", "github__list_prs"),
        ];

        let (outcome, newly) = resolve_load(
            &names(&["github__*"]),
            &deferred,
            &deferred,
            &[],
            MAX_ADVERTISED_TOOLS - 1,
        );
        assert!(outcome.loaded.is_empty());
        assert_eq!(
            outcome.rejected,
            vec![
                Rejected {
                    name: "github__create_pr".to_string(),
                    reason: RejectReason::ToolLimit,
                },
                Rejected {
                    name: "github__list_prs".to_string(),
                    reason: RejectReason::ToolLimit,
                },
            ]
        );
        assert!(newly.is_empty());

        let (outcome, newly) = resolve_load(
            &names(&["github__create_pr"]),
            &deferred,
            &deferred,
            &[],
            MAX_ADVERTISED_TOOLS - 1,
        );
        assert_eq!(outcome.loaded, names(&["github__create_pr"]));
        assert_eq!(newly, names(&["github__create_pr"]));
    }

    #[test]
    fn deferred_tool_named_recovers_mangled_names() {
        let deferred = vec![tool("github", "github__create_pr")];

        for emitted in [
            "github__create_pr",
            "github.create_pr",
            "functions.github__create_pr",
        ] {
            assert_eq!(
                deferred_tool_named(emitted, &deferred).as_deref(),
                Some("github__create_pr"),
                "{emitted}"
            );
        }
        assert_eq!(deferred_tool_named("github.merge_pr", &deferred), None);
    }
}
