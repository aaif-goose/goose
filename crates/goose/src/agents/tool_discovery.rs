//! Progressive tool discovery. A session that defers tools lists the tools of
//! its MCP extensions by name instead of sending their schemas. The model finds
//! tools with `search_tools` and loads the ones it needs with `load_tools`. A
//! load is recorded in the session's [`ToolDiscoveryState`], not in the
//! conversation, so compaction and resume keep it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

use rmcp::model::Tool;
use serde::Serialize;
use serde_json::Value;

use crate::agents::extension_manager::{get_tool_owner, recover_mangled_tool_name, ExtensionLease};
use crate::session::{Session, ToolDiscoveryState};

/// A tool sets `_meta["goose/alwaysLoad"]: true` to be sent up front in a
/// session that defers tools.
pub const ALWAYS_LOAD_META_KEY: &str = "goose/alwaysLoad";
pub const LOAD_TOOLS_TOOL_NAME: &str = "load_tools";
pub const SEARCH_TOOLS_TOOL_NAME: &str = "search_tools";
pub const DEFAULT_SEARCH_LIMIT: usize = 10;
pub const MAX_SEARCH_LIMIT: usize = 50;
const DESCRIPTION_PREVIEW_CHARS: usize = 200;
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

/// Unloaded tool names grouped by the extension that owns them.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchHit {
    pub name: String,
    pub extension: String,
    pub description: String,
    pub loaded: bool,
}

const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;
/// Name terms count double: a query word in a tool's name says more about
/// what it does than the same word in its description.
const NAME_WEIGHT: f64 = 2.0;

/// Lowercased words, split on punctuation and camelCase, with a plural `s`
/// dropped so "issues" matches "issue".
fn terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut word = String::new();
    let mut after_lowercase = false;
    for c in text.chars() {
        if !c.is_alphanumeric() || (c.is_uppercase() && after_lowercase) {
            push_term(&mut terms, std::mem::take(&mut word));
        }
        if c.is_alphanumeric() {
            word.extend(c.to_lowercase());
        }
        after_lowercase = c.is_lowercase() || c.is_ascii_digit();
    }
    push_term(&mut terms, word);
    terms
}

fn push_term(terms: &mut Vec<String>, mut word: String) {
    if word.is_empty() {
        return;
    }
    if word.chars().count() > 2 && word.ends_with('s') && !word.ends_with("ss") {
        word.pop();
    }
    terms.push(word);
}

/// One field of every tool, scored with BM25 against a query.
struct Bm25Field {
    docs: Vec<Vec<String>>,
    avg_len: f64,
    idf: HashMap<String, f64>,
}

impl Bm25Field {
    fn new(docs: Vec<Vec<String>>, query: &[String]) -> Self {
        let n = docs.len() as f64;
        let total_len: usize = docs.iter().map(Vec::len).sum();
        let avg_len = (total_len as f64 / n.max(1.0)).max(1.0);
        let idf = query
            .iter()
            .map(|term| {
                let df = docs.iter().filter(|doc| doc.contains(term)).count() as f64;
                (term.clone(), (1.0 + (n - df + 0.5) / (df + 0.5)).ln())
            })
            .collect();
        Self { docs, avg_len, idf }
    }

    fn score(&self, doc: usize) -> f64 {
        let doc = &self.docs[doc];
        let len_norm = 1.0 - BM25_B + BM25_B * doc.len() as f64 / self.avg_len;
        self.idf
            .iter()
            .map(|(term, idf)| {
                let tf = doc.iter().filter(|word| *word == term).count() as f64;
                idf * tf * (BM25_K1 + 1.0) / (tf + BM25_K1 * len_norm)
            })
            .sum()
    }
}

fn description_preview(description: &str) -> String {
    let first_line = description
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if first_line.chars().count() <= DESCRIPTION_PREVIEW_CHARS {
        return first_line.to_string();
    }
    let preview: String = first_line.chars().take(DESCRIPTION_PREVIEW_CHARS).collect();
    format!("{}…", preview.trim_end())
}

/// Ranks `tools` against `query` with BM25 over each tool's name and
/// description. A tool whose full name is the query ranks first; tools that
/// match no query word are left out.
pub(crate) fn search_tools(
    query: &str,
    limit: usize,
    tools: &[Tool],
    loaded: &[String],
) -> Vec<SearchHit> {
    let query = query.trim();
    let mut query_terms = terms(query);
    query_terms.sort();
    query_terms.dedup();
    let names = Bm25Field::new(
        tools.iter().map(|tool| terms(&tool.name)).collect(),
        &query_terms,
    );
    let descriptions = Bm25Field::new(
        tools
            .iter()
            .map(|tool| terms(tool.description.as_deref().unwrap_or_default()))
            .collect(),
        &query_terms,
    );

    let mut ranked: Vec<(bool, f64, &Tool)> = tools
        .iter()
        .enumerate()
        .map(|(i, tool)| {
            let exact = tool.name.eq_ignore_ascii_case(query);
            let score = NAME_WEIGHT * names.score(i) + descriptions.score(i);
            (exact, score, tool)
        })
        .filter(|(exact, score, _)| *exact || *score > 0.0)
        .collect();
    ranked.sort_by(|(a_exact, a_score, a), (b_exact, b_score, b)| {
        b_exact
            .cmp(a_exact)
            .then(b_score.partial_cmp(a_score).unwrap_or(Ordering::Equal))
            .then_with(|| a.name.cmp(&b.name))
    });

    ranked
        .into_iter()
        .take(limit)
        .map(|(_, _, tool)| SearchHit {
            name: tool.name.to_string(),
            extension: get_tool_owner(tool).unwrap_or_default(),
            description: description_preview(tool.description.as_deref().unwrap_or_default()),
            loaded: loaded.iter().any(|name| name == &*tool.name),
        })
        .collect()
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

    fn described(owner: &str, name: &str, description: &str) -> Tool {
        let mut tool = tool(owner, name);
        tool.description = Some(description.to_string().into());
        tool
    }

    fn hit_names(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|hit| hit.name.as_str()).collect()
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

    #[test]
    fn terms_split_names_and_drop_plurals() {
        assert_eq!(
            terms("github__listPullRequests v2"),
            names(&["github", "list", "pull", "request", "v2"])
        );
        assert_eq!(
            terms("Get issues, PRs & class"),
            names(&["get", "issue", "pr", "class"])
        );
    }

    #[test]
    fn an_exact_name_ranks_first() {
        let tools = vec![
            described(
                "linear",
                "linear__get_issue_comments",
                "Get a Linear issue's comments, or get the issue itself and its linked issues",
            ),
            described("linear", "linear__get_issue", "Fetch one record"),
        ];
        let hits = search_tools("linear__get_issue", 10, &tools, &[]);
        assert_eq!(
            hit_names(&hits),
            ["linear__get_issue", "linear__get_issue_comments"]
        );
    }

    #[test]
    fn name_matches_outrank_description_matches() {
        let tools = vec![
            described(
                "github",
                "github__get_file",
                "Read a file at a pull request's head",
            ),
            described("github", "github__create_pull_request", "Open a new PR"),
            described(
                "slack",
                "slack__post_message",
                "Post a message to a channel",
            ),
        ];
        let hits = search_tools("pull requests", 10, &tools, &[]);
        assert_eq!(
            hit_names(&hits),
            ["github__create_pull_request", "github__get_file"]
        );
    }

    #[test]
    fn rare_words_outweigh_common_ones() {
        let tools = vec![
            described("jira", "jira__get_issue", "Get an issue"),
            described("jira", "jira__get_sprint", "Get a sprint"),
            described("jira", "jira__get_board", "Get a board"),
        ];
        let hits = search_tools("get sprint", 10, &tools, &[]);
        assert_eq!(hit_names(&hits)[0], "jira__get_sprint");
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn search_leaves_out_tools_that_match_nothing_and_respects_the_limit() {
        let tools = vec![
            described("drive", "drive__list_files", "List files"),
            described("drive", "drive__read_file", "Read a file"),
            described("drive", "drive__share", "Share a document"),
        ];
        assert!(search_tools("calendar event", 10, &tools, &[]).is_empty());
        assert!(search_tools("  ", 10, &tools, &[]).is_empty());
        assert_eq!(
            hit_names(&search_tools("file", 10, &tools, &[])),
            ["drive__list_files", "drive__read_file"]
        );
        assert_eq!(search_tools("file", 1, &tools, &[]).len(), 1);
    }

    #[test]
    fn hits_carry_the_extension_a_description_preview_and_whether_they_are_loaded() {
        let long = "x".repeat(DESCRIPTION_PREVIEW_CHARS + 10);
        let tools = vec![
            described(
                "linear",
                "linear__get_issue",
                "\n  Get an issue by key.  \nArgs: key, the issue key.",
            ),
            described("linear", "linear__list_issues", &long),
        ];
        let hits = search_tools("issue", 10, &tools, &names(&["linear__list_issues"]));
        assert_eq!(
            hits[0],
            SearchHit {
                name: "linear__get_issue".to_string(),
                extension: "linear".to_string(),
                description: "Get an issue by key.".to_string(),
                loaded: false,
            }
        );
        assert!(hits[1].loaded);
        assert_eq!(
            hits[1].description,
            format!("{}…", "x".repeat(DESCRIPTION_PREVIEW_CHARS))
        );
    }
}
