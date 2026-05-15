//! Langfuse Bridge MCP extension.
//!
//! Read-only bridge into a Langfuse instance. Lets an agent pull traces and
//! their observations alongside eval-bench results — pair the per-trial
//! pass/fail recorded by `run_kpass.py` with the model turn-by-turn that
//! Langfuse captured for the same run.
//!
//! Configuration (same env-var names already consumed by `goose`'s tracing
//! layer, so a profile that has tracing wired in just works):
//!
//!   LANGFUSE_PUBLIC_KEY  /  LANGFUSE_INIT_PROJECT_PUBLIC_KEY   (required)
//!   LANGFUSE_SECRET_KEY  /  LANGFUSE_INIT_PROJECT_SECRET_KEY   (required)
//!   LANGFUSE_URL         /  LANGFUSE_BASE_URL
//!     base URL of the Langfuse instance.
//!     Defaults to https://cloud.langfuse.com.
//!
//! When credentials are missing the extension still starts so the user can
//! enable it in a profile without setup, but every tool call returns an
//! explicit "not configured" error instead of silently no-op'ing.

use base64::Engine;
use reqwest::Client;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorCode, ErrorData, Implementation, InitializeResult,
        ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::env;
use url::Url;

const DEFAULT_LANGFUSE_URL: &str = "https://cloud.langfuse.com";
const DEFAULT_LIST_LIMIT: u32 = 20;
const MAX_LIST_LIMIT: u32 = 100;

/// Parameters for `list_traces`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListTracesParams {
    /// Filter to traces owned by this user id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Filter to traces in this session id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Filter to traces whose `name` exactly matches this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Filter to traces that carry any of these tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// ISO-8601 lower bound on `timestamp`, e.g. 2026-05-15T00:00:00Z.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<String>,
    /// ISO-8601 upper bound on `timestamp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<String>,
    /// Page size. Defaults to 20, clamped to [1, 100].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// 1-indexed page number. Defaults to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
}

/// Parameters for `get_trace`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetTraceParams {
    /// Langfuse trace id (as returned by `list_traces`).
    pub trace_id: String,
}

#[derive(Debug, Clone)]
struct LangfuseCredentials {
    base_url: String,
    public_key: String,
    secret_key: String,
}

impl LangfuseCredentials {
    fn from_env() -> Option<Self> {
        let public_key =
            first_non_empty_env(&["LANGFUSE_PUBLIC_KEY", "LANGFUSE_INIT_PROJECT_PUBLIC_KEY"])?;
        let secret_key =
            first_non_empty_env(&["LANGFUSE_SECRET_KEY", "LANGFUSE_INIT_PROJECT_SECRET_KEY"])?;
        let base_url = first_non_empty_env(&["LANGFUSE_URL", "LANGFUSE_BASE_URL"])
            .unwrap_or_else(|| DEFAULT_LANGFUSE_URL.to_string());
        Some(Self {
            base_url,
            public_key,
            secret_key,
        })
    }

    fn auth_header(&self) -> String {
        let token = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", self.public_key, self.secret_key));
        format!("Basic {token}")
    }
}

fn first_non_empty_env(names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(val) = env::var(name) {
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

/// Build the `GET /api/public/traces` URL with all relevant query params set.
///
/// Split out so the URL shape is testable without an HTTP server. The clamp
/// on `limit` is applied here (Langfuse rejects pages larger than 100).
fn build_list_traces_url(
    base_url: &str,
    params: &ListTracesParams,
) -> Result<Url, url::ParseError> {
    let mut url = Url::parse(base_url)?.join("/api/public/traces")?;
    {
        let mut q = url.query_pairs_mut();
        if let Some(ref user_id) = params.user_id {
            q.append_pair("userId", user_id);
        }
        if let Some(ref session_id) = params.session_id {
            q.append_pair("sessionId", session_id);
        }
        if let Some(ref name) = params.name {
            q.append_pair("name", name);
        }
        for tag in &params.tags {
            q.append_pair("tags", tag);
        }
        if let Some(ref from) = params.from_timestamp {
            q.append_pair("fromTimestamp", from);
        }
        if let Some(ref to) = params.to_timestamp {
            q.append_pair("toTimestamp", to);
        }
        let limit = params
            .limit
            .unwrap_or(DEFAULT_LIST_LIMIT)
            .clamp(1, MAX_LIST_LIMIT);
        q.append_pair("limit", &limit.to_string());
        if let Some(page) = params.page {
            q.append_pair("page", &page.max(1).to_string());
        }
    }
    Ok(url)
}

fn build_get_trace_url(base_url: &str, trace_id: &str) -> Result<Url, url::ParseError> {
    // Strip leading slash from the percent-encoded id to ensure join works as
    // a relative resolution against /api/public/traces/.
    let encoded = urlencoding_encode(trace_id);
    Url::parse(base_url)?.join(&format!("/api/public/traces/{encoded}"))
}

/// Percent-encode a trace id for use as a path segment. We avoid pulling in
/// the `urlencoding` crate — the `url` crate's `Url::join` does its own
/// encoding for path segments, but only for a small reserved set. Since
/// Langfuse trace ids are typically UUID/ULID-shaped this is overkill in
/// practice, but the helper keeps callers honest.
fn urlencoding_encode(s: &str) -> String {
    const SAFE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~";
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if SAFE.contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Langfuse Bridge MCP server.
#[derive(Clone)]
pub struct LangfuseBridgeServer {
    tool_router: ToolRouter<Self>,
    credentials: Option<LangfuseCredentials>,
    http: Client,
}

impl Default for LangfuseBridgeServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl LangfuseBridgeServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            credentials: LangfuseCredentials::from_env(),
            http: Client::new(),
        }
    }

    fn instructions(&self) -> String {
        let configured = self.credentials.is_some();
        let base = if let Some(creds) = &self.credentials {
            format!(" Bridge points at `{}`.", creds.base_url)
        } else {
            String::new()
        };
        format!(
            "Langfuse Bridge — pull traces and observations from a Langfuse \
             instance, e.g. while triaging an eval-bench run. {tools}{base}\n\n\
             Configure with LANGFUSE_PUBLIC_KEY, LANGFUSE_SECRET_KEY, and \
             optionally LANGFUSE_URL (default: https://cloud.langfuse.com). \
             Current status: {status}.",
            tools = "Tools: `list_traces` (filters: userId, sessionId, name, tags, fromTimestamp, toTimestamp, limit, page), `get_trace` (full trace + observations).",
            status = if configured { "configured" } else { "NOT configured — tool calls will return an explicit error until credentials are set" },
        )
    }

    /// List recent Langfuse traces, optionally filtered by user / session /
    /// name / tags / timestamp range. Returns up to `limit` (default 20,
    /// max 100) traces per page.
    #[tool(
        name = "list_traces",
        description = "List recent Langfuse traces with optional userId, sessionId, name, tags, fromTimestamp, toTimestamp, limit, and page filters. Returns the JSON response from GET /api/public/traces."
    )]
    pub async fn list_traces(
        &self,
        params: Parameters<ListTracesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let creds = self.require_credentials()?;
        let url = build_list_traces_url(&creds.base_url, &params.0).map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("invalid LANGFUSE_URL: {e}"),
                None,
            )
        })?;
        let body = self.get_json(&url, creds).await?;
        Ok(CallToolResult::success(vec![
            Content::text(body).with_audience(vec![rmcp::model::Role::Assistant])
        ]))
    }

    /// Fetch a single Langfuse trace by id, including its observations.
    #[tool(
        name = "get_trace",
        description = "Fetch a single Langfuse trace by id, including all of its observations. Returns the JSON response from GET /api/public/traces/{traceId}."
    )]
    pub async fn get_trace(
        &self,
        params: Parameters<GetTraceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let creds = self.require_credentials()?;
        if params.0.trace_id.trim().is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "trace_id must not be empty".to_string(),
                None,
            ));
        }
        let url = build_get_trace_url(&creds.base_url, &params.0.trace_id).map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("invalid LANGFUSE_URL or trace_id: {e}"),
                None,
            )
        })?;
        let body = self.get_json(&url, creds).await?;
        Ok(CallToolResult::success(vec![
            Content::text(body).with_audience(vec![rmcp::model::Role::Assistant])
        ]))
    }

    fn require_credentials(&self) -> Result<&LangfuseCredentials, ErrorData> {
        self.credentials.as_ref().ok_or_else(|| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Langfuse Bridge is not configured: set LANGFUSE_PUBLIC_KEY \
                 and LANGFUSE_SECRET_KEY (and optionally LANGFUSE_URL) in \
                 the environment before enabling this extension."
                    .to_string(),
                None,
            )
        })
    }

    async fn get_json(&self, url: &Url, creds: &LangfuseCredentials) -> Result<String, ErrorData> {
        let response = self
            .http
            .get(url.clone())
            .header("Authorization", creds.auth_header())
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Langfuse request failed: {e}"),
                    None,
                )
            })?;
        let status = response.status();
        let body = response.text().await.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("could not read Langfuse response body: {e}"),
                None,
            )
        })?;
        if !status.is_success() {
            return Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "Langfuse returned HTTP {status} for {url}: {}",
                    truncate(&body, 500)
                ),
                None,
            ));
        }
        Ok(body)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for LangfuseBridgeServer {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "goose-langfuse-bridge",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(self.instructions())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_with(mut f: impl FnMut(&mut ListTracesParams)) -> ListTracesParams {
        let mut p = ListTracesParams::default();
        f(&mut p);
        p
    }

    #[test]
    fn list_url_carries_all_filters_and_defaults_limit() {
        let p = params_with(|p| {
            p.user_id = Some("alice".into());
            p.session_id = Some("s-1".into());
            p.name = Some("recipe-run".into());
            p.tags = vec!["release".into(), "skein".into()];
            p.from_timestamp = Some("2026-05-01T00:00:00Z".into());
            p.to_timestamp = Some("2026-05-15T00:00:00Z".into());
            p.page = Some(2);
        });
        let url = build_list_traces_url("https://cloud.langfuse.com", &p).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q.get("userId").map(String::as_str), Some("alice"));
        assert_eq!(q.get("sessionId").map(String::as_str), Some("s-1"));
        assert_eq!(q.get("name").map(String::as_str), Some("recipe-run"));
        // tags appear twice — last-wins in a HashMap is fine for spot-check; assert presence:
        let raw_query = url.query().unwrap_or("");
        assert!(raw_query.contains("tags=release"));
        assert!(raw_query.contains("tags=skein"));
        assert_eq!(
            q.get("fromTimestamp").map(String::as_str),
            Some("2026-05-01T00:00:00Z")
        );
        assert_eq!(
            q.get("toTimestamp").map(String::as_str),
            Some("2026-05-15T00:00:00Z")
        );
        assert_eq!(q.get("limit").map(String::as_str), Some("20"));
        assert_eq!(q.get("page").map(String::as_str), Some("2"));
    }

    #[test]
    fn list_url_clamps_limit_into_valid_range() {
        let p = params_with(|p| p.limit = Some(9999));
        let url = build_list_traces_url("https://cloud.langfuse.com", &p).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q.get("limit").map(String::as_str), Some("100"));

        let p = params_with(|p| p.limit = Some(0));
        let url = build_list_traces_url("https://cloud.langfuse.com", &p).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q.get("limit").map(String::as_str), Some("1"));
    }

    #[test]
    fn list_url_omits_unset_optional_params() {
        let url = build_list_traces_url("https://cloud.langfuse.com", &ListTracesParams::default())
            .unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q.get("limit").map(String::as_str), Some("20"));
        assert!(!q.contains_key("userId"));
        assert!(!q.contains_key("sessionId"));
        assert!(!q.contains_key("page"));
    }

    #[test]
    fn get_trace_url_percent_encodes_id() {
        let url = build_get_trace_url("https://cloud.langfuse.com", "abc def/ghi").unwrap();
        assert_eq!(url.path(), "/api/public/traces/abc%20def%2Fghi",);
    }

    #[test]
    fn get_trace_url_accepts_typical_ulid() {
        let url = build_get_trace_url("https://cloud.langfuse.com", "01HXYZABCDEFG1234567890ABC")
            .unwrap();
        assert_eq!(url.path(), "/api/public/traces/01HXYZABCDEFG1234567890ABC",);
    }

    #[test]
    fn credentials_pick_first_non_empty_env_name() {
        // Pure helper test — no actual env mutation needed for this.
        std::env::set_var("__LANGFUSE_TEST_A", "");
        std::env::set_var("__LANGFUSE_TEST_B", "found");
        let v = first_non_empty_env(&["__LANGFUSE_TEST_A", "__LANGFUSE_TEST_B"]);
        std::env::remove_var("__LANGFUSE_TEST_A");
        std::env::remove_var("__LANGFUSE_TEST_B");
        assert_eq!(v.as_deref(), Some("found"));
    }

    #[test]
    fn auth_header_is_basic_base64_of_pubkey_colon_secret() {
        let creds = LangfuseCredentials {
            base_url: "https://x".into(),
            public_key: "pk-1".into(),
            secret_key: "sk-1".into(),
        };
        // base64("pk-1:sk-1") = cGstMTpzay0x
        assert_eq!(creds.auth_header(), "Basic cGstMTpzay0x");
    }

    #[test]
    fn server_without_credentials_still_constructs() {
        // Sanity: clearing creds-related env vars must not panic during new().
        // We don't mutate the actual env here (the test process may run in
        // parallel with other tests); we just confirm that the type can be
        // constructed when from_env() returns None.
        let server = LangfuseBridgeServer {
            tool_router: LangfuseBridgeServer::tool_router(),
            credentials: None,
            http: Client::new(),
        };
        let info = server.get_info();
        assert_eq!(info.server_info.name, "goose-langfuse-bridge");
        let instructions = info.instructions.unwrap_or_default();
        assert!(instructions.contains("NOT configured"));
    }

    #[tokio::test]
    async fn list_traces_returns_clear_error_when_unconfigured() {
        let server = LangfuseBridgeServer {
            tool_router: LangfuseBridgeServer::tool_router(),
            credentials: None,
            http: Client::new(),
        };
        let err = server
            .list_traces(Parameters(ListTracesParams::default()))
            .await
            .unwrap_err();
        assert!(err.message.contains("not configured"));
    }

    #[tokio::test]
    async fn get_trace_rejects_empty_id() {
        let server = LangfuseBridgeServer {
            tool_router: LangfuseBridgeServer::tool_router(),
            credentials: Some(LangfuseCredentials {
                base_url: "https://x".into(),
                public_key: "pk".into(),
                secret_key: "sk".into(),
            }),
            http: Client::new(),
        };
        let err = server
            .get_trace(Parameters(GetTraceParams {
                trace_id: "   ".into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("trace_id"));
    }

    #[test]
    fn truncate_appends_ellipsis_only_when_over_max() {
        assert_eq!(truncate("abc", 10), "abc");
        let s = truncate("abcdefghijkl", 5);
        assert_eq!(s, "abcde…");
    }
}
