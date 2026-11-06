//! `search_tools` ranking: BM25F over each deferrable tool's name,
//! description, extension instructions and input parameters, computed per
//! search with no index.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use rmcp::model::{JsonObject, Tool};
use serde::Serialize;
use serde_json::Value;

use crate::agents::extension_manager::get_tool_owner;

pub const DEFAULT_SEARCH_LIMIT: usize = 10;
pub const MAX_SEARCH_LIMIT: usize = 50;
const DESCRIPTION_PREVIEW_CHARS: usize = 200;
const MAX_PARAMETER_TEXT_CHARS: usize = 1024;

/// Lucene's default term-frequency saturation.
const K1: f64 = 1.2;

/// Agents phrase queries as requests ("show me my open pull requests"). These
/// words carry no capability signal but would still match descriptions. Verbs
/// such as "get" and "find" stay: they tell read tools from write tools.
const QUERY_STOPWORDS: &[&str] = &[
    "a", "all", "am", "an", "and", "any", "are", "as", "at", "be", "by", "for", "from", "i", "in",
    "is", "it", "its", "me", "my", "of", "on", "or", "please", "show", "some", "that", "the",
    "these", "this", "those", "to", "what", "which", "who", "with",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchHit {
    pub name: String,
    pub extension: String,
    pub description: String,
    pub loaded: bool,
}

#[derive(Clone, Copy)]
enum Field {
    Name,
    Description,
    /// The owning extension's instructions.
    Summary,
    Parameters,
}

const FIELD_COUNT: usize = 4;

impl Field {
    const ALL: [Field; FIELD_COUNT] = [
        Field::Name,
        Field::Description,
        Field::Summary,
        Field::Parameters,
    ];

    /// BM25F weight and length-normalization strength `b`.
    fn weight_and_b(self) -> (f64, f64) {
        match self {
            Field::Name => (3.0, 0.75),
            Field::Description => (1.0, 0.75),
            // Every tool in an extension shares its summary, so length
            // normalization would only reorder tools within the extension.
            Field::Summary => (0.25, 0.0),
            Field::Parameters => (1.0, 0.5),
        }
    }
}

/// A tool's searchable text, one entry per [`Field`].
type SearchDocument = [String; FIELD_COUNT];

/// Lowercased words, split on anything but letters and digits and on
/// camelCase boundaries.
fn terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut word = String::new();
    let mut after_lowercase = false;
    for c in text.chars() {
        let boundary = !c.is_alphanumeric() || (c.is_uppercase() && after_lowercase);
        if boundary && !word.is_empty() {
            terms.push(std::mem::take(&mut word));
        }
        if c.is_alphanumeric() {
            word.extend(c.to_lowercase());
        }
        after_lowercase = c.is_lowercase() || c.is_numeric();
    }
    if !word.is_empty() {
        terms.push(word);
    }
    terms
}

/// Query terms without filler words, unless filler is all there is.
fn query_terms(query: &str) -> HashSet<String> {
    let all = terms(query);
    let meaningful: HashSet<String> = all
        .iter()
        .filter(|term| !QUERY_STOPWORDS.contains(&term.as_str()))
        .cloned()
        .collect();
    if meaningful.is_empty() {
        all.into_iter().collect()
    } else {
        meaningful
    }
}

fn primitive_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// Names, enum values and descriptions of a tool's top-level input
/// properties. Tools often keep capability words here rather than in the
/// description, such as a `method` enum of `get_diff` and `get_status`. Names
/// and values come first so a long description can't truncate them away.
fn parameter_text(schema: &JsonObject) -> String {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return String::new();
    };
    let mut names_and_values = Vec::new();
    let mut descriptions = Vec::new();
    for (name, spec) in properties {
        names_and_values.push(name.clone());
        if let Some(values) = spec.get("enum").and_then(Value::as_array) {
            names_and_values.extend(values.iter().filter_map(primitive_text));
        }
        if let Some(description) = spec.get("description").and_then(Value::as_str) {
            descriptions.push(description.to_string());
        }
    }
    names_and_values
        .into_iter()
        .chain(descriptions)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_PARAMETER_TEXT_CHARS)
        .collect()
}

fn search_document(tool: &Tool, summaries: &HashMap<String, String>) -> SearchDocument {
    let summary = get_tool_owner(tool)
        .and_then(|owner| summaries.get(&owner).cloned())
        .unwrap_or_default();
    [
        tool.name.to_string(),
        tool.description.as_deref().unwrap_or_default().to_string(),
        summary,
        parameter_text(&tool.input_schema),
    ]
}

/// A field's length in terms and how often each query term appears in it.
struct FieldStats {
    length: usize,
    counts: HashMap<String, usize>,
}

fn field_stats(text: &str, query: &HashSet<String>) -> FieldStats {
    let terms = terms(text);
    let mut counts = HashMap::new();
    for term in terms.iter().filter(|term| query.contains(*term)) {
        *counts.entry(term.clone()).or_default() += 1;
    }
    FieldStats {
        length: terms.len(),
        counts,
    }
}

/// BM25F score of each document for `query`. Each field's term frequency is
/// weighted and length-normalized on its own, then the sum is saturated once,
/// so a term repeated across fields isn't counted as independent evidence.
fn bm25f(documents: &[SearchDocument], query: &HashSet<String>) -> Vec<f64> {
    // Tools in one extension share its summary, so each distinct text is read once.
    let mut stats_by_text: HashMap<&str, FieldStats> = HashMap::new();
    for text in documents.iter().flatten() {
        stats_by_text
            .entry(text.as_str())
            .or_insert_with(|| field_stats(text, query));
    }
    let stats: Vec<[&FieldStats; FIELD_COUNT]> = documents
        .iter()
        .map(|document| {
            document
                .each_ref()
                .map(|text| &stats_by_text[text.as_str()])
        })
        .collect();

    // Averaged over the tools that have the field, as Lucene does, so a field
    // most tools leave empty doesn't penalize the tools that fill it in.
    let average_lengths: [f64; FIELD_COUNT] = std::array::from_fn(|field| {
        let lengths: Vec<usize> = stats
            .iter()
            .map(|fields| fields[field].length)
            .filter(|length| *length > 0)
            .collect();
        if lengths.is_empty() {
            1.0
        } else {
            lengths.iter().sum::<usize>() as f64 / lengths.len() as f64
        }
    });

    let n = documents.len() as f64;
    let idf: HashMap<&str, f64> = query
        .iter()
        .map(|term| {
            let df = stats
                .iter()
                .filter(|fields| fields.iter().any(|field| field.counts.contains_key(term)))
                .count() as f64;
            (term.as_str(), (1.0 + (n - df + 0.5) / (df + 0.5)).ln())
        })
        .collect();

    stats
        .iter()
        .map(|fields| {
            query
                .iter()
                .map(|term| {
                    let tf: f64 = Field::ALL
                        .iter()
                        .map(|&field| {
                            let (weight, b) = field.weight_and_b();
                            let stats = fields[field as usize];
                            let count = stats.counts.get(term).copied().unwrap_or_default();
                            let norm =
                                1.0 - b + b * stats.length as f64 / average_lengths[field as usize];
                            weight * count as f64 / norm
                        })
                        .sum();
                    idf[term.as_str()] * tf * (K1 + 1.0) / (tf + K1)
                })
                .sum()
        })
        .collect()
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

/// Ranks `tools` against `query` by [`bm25f`]. A tool whose full name is the
/// query ranks first; tools that match no query word are left out.
/// `summaries` maps an extension name to its instructions.
pub(crate) fn search_tools(
    query: &str,
    limit: usize,
    tools: &[Tool],
    summaries: &HashMap<String, String>,
    loaded: &[String],
) -> Vec<SearchHit> {
    let query = query.trim();
    let documents: Vec<SearchDocument> = tools
        .iter()
        .map(|tool| search_document(tool, summaries))
        .collect();
    let scores = bm25f(&documents, &query_terms(query));

    let mut ranked: Vec<(bool, f64, &Tool)> = tools
        .iter()
        .zip(scores)
        .map(|(tool, score)| (tool.name.eq_ignore_ascii_case(query), score, tool))
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
    use rmcp::model::MetaObject;
    use serde_json::json;
    use std::sync::Arc;

    fn tool(owner: &str, name: &str, description: &str) -> Tool {
        let mut tool = Tool::new(
            name.to_string(),
            description.to_string(),
            Arc::new(JsonObject::new()),
        );
        let mut meta = JsonObject::new();
        meta.insert("goose_extension".to_string(), json!(owner));
        tool.meta = Some(MetaObject(meta));
        tool
    }

    fn with_schema(mut tool: Tool, schema: Value) -> Tool {
        tool.input_schema = Arc::new(schema.as_object().unwrap().clone());
        tool
    }

    fn search(query: &str, tools: &[Tool]) -> Vec<String> {
        search_tools(query, 10, tools, &HashMap::new(), &[])
            .into_iter()
            .map(|hit| hit.name)
            .collect()
    }

    #[test]
    fn terms_split_on_punctuation_and_camel_case() {
        assert_eq!(
            terms("github__listPullRequests v2"),
            ["github", "list", "pull", "requests", "v2"]
        );
        assert_eq!(
            terms("Get issues, PRs & more"),
            ["get", "issues", "prs", "more"]
        );
    }

    #[test]
    fn filler_words_are_dropped_unless_nothing_else_is_left() {
        assert_eq!(
            query_terms("show me my open pull requests"),
            HashSet::from([
                "open".to_string(),
                "pull".to_string(),
                "requests".to_string()
            ])
        );
        assert_eq!(
            query_terms("who am i"),
            HashSet::from(["who".to_string(), "am".to_string(), "i".to_string()])
        );
    }

    #[test]
    fn an_exact_name_ranks_first() {
        let tools = vec![
            tool(
                "linear",
                "linear__get_issue_comments",
                "Get a Linear issue's comments, or get the issue itself and its linked issue",
            ),
            tool("linear", "linear__get_issue", "Fetch one record"),
        ];
        assert_eq!(
            search("linear__get_issue", &tools),
            ["linear__get_issue", "linear__get_issue_comments"]
        );
    }

    #[test]
    fn name_matches_outrank_description_matches() {
        let tools = vec![
            tool(
                "github",
                "github__get_file",
                "Read a file at a pull request head",
            ),
            tool("github", "github__create_pull_request", "Open a new PR"),
            tool(
                "slack",
                "slack__post_message",
                "Post a message to a channel",
            ),
        ];
        assert_eq!(
            search("pull request", &tools),
            ["github__create_pull_request", "github__get_file"]
        );
    }

    #[test]
    fn rare_words_outweigh_common_ones() {
        let tools = vec![
            tool("jira", "jira__get_issue", "Get an issue"),
            tool("jira", "jira__get_sprint", "Get a sprint"),
            tool("jira", "jira__get_board", "Get a board"),
        ];
        let hits = search("get sprint", &tools);
        assert_eq!(hits[0], "jira__get_sprint");
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn parameter_names_and_enum_values_are_searched() {
        let tools = vec![
            with_schema(
                tool(
                    "github",
                    "github__pull_request_read",
                    "Get information on a specific pull request",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "method": { "type": "string", "enum": ["get", "get_diff", "get_status"] },
                        "pullNumber": { "type": "number" },
                    },
                }),
            ),
            tool("github", "github__list_pull_requests", "List pull requests"),
            tool(
                "github",
                "github__update_pull_request",
                "Update a pull request",
            ),
        ];
        assert_eq!(
            search("pull request diff", &tools)[0],
            "github__pull_request_read"
        );
    }

    #[test]
    fn parameter_text_puts_names_and_values_before_descriptions() {
        let schema = json!({
            "properties": {
                "state": { "enum": ["open", "closed", 3, true, null], "description": "Filter by state" },
                "query": { "description": "x".repeat(MAX_PARAMETER_TEXT_CHARS) },
            },
        });
        let text = parameter_text(schema.as_object().unwrap());
        assert!(text.starts_with("state open closed 3 true query Filter by state x"));
        assert_eq!(text.chars().count(), MAX_PARAMETER_TEXT_CHARS);
        assert_eq!(parameter_text(&JsonObject::new()), "");
    }

    #[test]
    fn the_extension_summary_breaks_ties_toward_the_named_extension() {
        let tools = vec![
            tool("jira", "jira__get_issue", "Get an issue"),
            tool("linear", "linear__get_issue", "Get an issue"),
        ];
        let summaries = HashMap::from([
            ("jira".to_string(), "Atlassian Jira.".to_string()),
            (
                "linear".to_string(),
                "The engineering team's tracker.".to_string(),
            ),
        ]);
        let hits = search_tools("get tracker issue", 10, &tools, &summaries, &[]);
        assert_eq!(hits[0].name, "linear__get_issue");
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn search_leaves_out_tools_that_match_nothing_and_respects_the_limit() {
        let tools = vec![
            tool("drive", "drive__list_files", "List files"),
            tool("drive", "drive__read_file", "Read a file"),
            tool("drive", "drive__share", "Share a document"),
        ];
        assert!(search("calendar event", &tools).is_empty());
        assert!(search("  ", &tools).is_empty());
        assert_eq!(search("file", &tools), ["drive__read_file"]);
        assert_eq!(
            search_tools("drive", 2, &tools, &HashMap::new(), &[]).len(),
            2
        );
    }

    #[test]
    fn hits_carry_the_extension_a_description_preview_and_whether_they_are_loaded() {
        let long = "x".repeat(DESCRIPTION_PREVIEW_CHARS + 10);
        let tools = vec![
            tool(
                "linear",
                "linear__get_issue",
                "\n  Get an issue by key.  \nArgs: key, the issue key.",
            ),
            tool("linear", "linear__list_issue", &long),
        ];
        let hits = search_tools(
            "issue",
            10,
            &tools,
            &HashMap::new(),
            &["linear__list_issue".to_string()],
        );
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
