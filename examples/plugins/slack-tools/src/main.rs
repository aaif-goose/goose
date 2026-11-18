use anyhow::Result;
use goose_plugin::PluginRuntime;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, ErrorCode, ErrorData, Implementation, InitializeResult,
        ServerCapabilities, ServerConfig,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct SlackTools {
    tool_router: ToolRouter<Self>,
    token: String,
    default_channel: String,
    http: reqwest::Client,
}

impl SlackTools {
    fn new() -> Self {
        let token = std::env::var("SLACK_TOKEN")
            .or_else(|_| std::env::var("GOOSE_PLUGIN_SLACK_TOOLS_TOKEN"))
            .unwrap_or_default();
        let default_channel = std::env::var("GOOSE_PLUGIN_SLACK_TOOLS_DEFAULT_CHANNEL")
            .unwrap_or_else(|_| "general".to_string());
        SlackTools {
            tool_router: Self::tool_router(),
            token,
            default_channel,
            http: reqwest::Client::builder()
                .user_agent("goose-slack-tools/0.1.0")
                .build()
                .unwrap(),
        }
    }

    async fn slack_get(&self, path: &str) -> std::result::Result<serde_json::Value, String> {
        if self.token.is_empty() {
            return Err(
                "Slack token not set. Configure SLACK_TOKEN or plugin token.".to_string(),
            );
        }
        let url = format!("https://slack.com/api{path}");
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        if !body["ok"].as_bool().unwrap_or(false) {
            let err = body["error"].as_str().unwrap_or("unknown_error");
            return Err(format!("Slack API error: {err}"));
        }
        Ok(body)
    }

    async fn slack_post(
        &self,
        path: &str,
        payload: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String> {
        if self.token.is_empty() {
            return Err("Slack token not set.".to_string());
        }
        let url = format!("https://slack.com/api{path}");
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        if !body["ok"].as_bool().unwrap_or(false) {
            let err = body["error"].as_str().unwrap_or("unknown_error");
            return Err(format!("Slack API error: {err}"));
        }
        Ok(body)
    }

    fn text_result(text: String) -> CallToolResult {
        CallToolResult::success(vec![ContentBlock::text(text)])
    }

    fn err_result(msg: String) -> ErrorData {
        ErrorData::new(ErrorCode::INTERNAL_ERROR, msg, None)
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct PostMessageInput {
    #[schemars(description = "Channel ID or name (e.g. C012AB3CD or #general). Omit to use default.")]
    channel: Option<String>,
    #[schemars(description = "Message text (Markdown supported)")]
    text: String,
    #[schemars(description = "Optional thread timestamp to reply in a thread")]
    thread_ts: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ListChannelsInput {
    #[schemars(description = "Maximum number of channels to return (default 20, max 100)")]
    limit: Option<u8>,
    #[schemars(description = "Exclude archived channels (default true)")]
    exclude_archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct GetChannelHistoryInput {
    #[schemars(description = "Channel ID (e.g. C012AB3CD). Omit to use default channel.")]
    channel: Option<String>,
    #[schemars(description = "Number of messages to return (default 10, max 50)")]
    limit: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchMessagesInput {
    #[schemars(description = "Search query, e.g. 'deploy failed in:#engineering'")]
    query: String,
    #[schemars(description = "Maximum results to return (default 10, max 25)")]
    limit: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct GetUserInfoInput {
    #[schemars(description = "Slack user ID (e.g. U012AB3CD)")]
    user_id: String,
}

#[tool_router(router = tool_router)]
impl SlackTools {
    #[tool(description = "Post a message to a Slack channel")]
    async fn post_message(
        &self,
        params: Parameters<PostMessageInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let channel = p
            .channel
            .as_deref()
            .unwrap_or(&self.default_channel)
            .to_string();
        let mut payload = serde_json::json!({ "channel": channel, "text": p.text });
        if let Some(ts) = p.thread_ts {
            payload["thread_ts"] = serde_json::Value::String(ts);
        }
        match self.slack_post("/chat.postMessage", payload).await {
            Ok(resp) => {
                let ts = resp["ts"].as_str().unwrap_or("");
                Ok(Self::text_result(format!(
                    "Message sent to {channel} (ts: {ts})"
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "List Slack channels in the workspace")]
    async fn list_channels(
        &self,
        params: Parameters<ListChannelsInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let limit = p.limit.unwrap_or(20).min(100);
        let exclude = p.exclude_archived.unwrap_or(true);
        let path = format!(
            "/conversations.list?limit={limit}&exclude_archived={exclude}&types=public_channel,private_channel"
        );
        match self.slack_get(&path).await {
            Ok(data) => {
                let channels = data["channels"].as_array().cloned().unwrap_or_default();
                if channels.is_empty() {
                    return Ok(Self::text_result("No channels found.".to_string()));
                }
                let lines: Vec<String> = channels
                    .iter()
                    .map(|ch| {
                        let id = ch["id"].as_str().unwrap_or("");
                        let name = ch["name"].as_str().unwrap_or("");
                        let members = ch["num_members"].as_u64().unwrap_or(0);
                        let private = if ch["is_private"].as_bool().unwrap_or(false) {
                            " [private]"
                        } else {
                            ""
                        };
                        format!("{id} #{name}{private} ({members} members)")
                    })
                    .collect();
                Ok(Self::text_result(format!(
                    "{} channels:\n{}",
                    channels.len(),
                    lines.join("\n")
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Get recent messages from a Slack channel")]
    async fn get_channel_history(
        &self,
        params: Parameters<GetChannelHistoryInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let channel = p
            .channel
            .as_deref()
            .unwrap_or(&self.default_channel)
            .to_string();
        let limit = p.limit.unwrap_or(10).min(50);
        let path = format!("/conversations.history?channel={channel}&limit={limit}");
        match self.slack_get(&path).await {
            Ok(data) => {
                let messages = data["messages"].as_array().cloned().unwrap_or_default();
                if messages.is_empty() {
                    return Ok(Self::text_result(format!("No messages in {channel}.")));
                }
                let lines: Vec<String> = messages
                    .iter()
                    .rev()
                    .map(|msg| {
                        let user = msg["user"].as_str().unwrap_or("bot");
                        let text = msg["text"].as_str().unwrap_or("").trim();
                        let ts = msg["ts"].as_str().unwrap_or("");
                        format!("[{ts}] <@{user}>: {text}")
                    })
                    .collect();
                Ok(Self::text_result(format!(
                    "Last {} messages in {channel}:\n{}",
                    messages.len(),
                    lines.join("\n")
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Search messages across Slack (requires search:read scope)")]
    async fn search_messages(
        &self,
        params: Parameters<SearchMessagesInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let limit = p.limit.unwrap_or(10).min(25);
        let path = format!(
            "/search.messages?query={}&count={limit}",
            urlencoding::encode(&p.query)
        );
        match self.slack_get(&path).await {
            Ok(data) => {
                let total = data["messages"]["total"].as_u64().unwrap_or(0);
                let matches = data["messages"]["matches"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if matches.is_empty() {
                    return Ok(Self::text_result(format!(
                        "No results for '{}'",
                        p.query
                    )));
                }
                let lines: Vec<String> = matches
                    .iter()
                    .map(|m| {
                        let channel = m["channel"]["name"].as_str().unwrap_or("unknown");
                        let user = m["username"].as_str().unwrap_or("unknown");
                        let text = m["text"].as_str().unwrap_or("").trim();
                        let ts = m["ts"].as_str().unwrap_or("");
                        format!("[#{channel} {ts}] {user}: {text}")
                    })
                    .collect();
                Ok(Self::text_result(format!(
                    "{total} results (showing {}):\n{}",
                    matches.len(),
                    lines.join("\n")
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Get information about a Slack user by their user ID")]
    async fn get_user_info(
        &self,
        params: Parameters<GetUserInfoInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let path = format!("/users.info?user={}", p.user_id);
        match self.slack_get(&path).await {
            Ok(data) => {
                let user = &data["user"];
                let name = user["real_name"].as_str().unwrap_or("");
                let display = user["profile"]["display_name"].as_str().unwrap_or("");
                let email = user["profile"]["email"].as_str().unwrap_or("(not available)");
                let title = user["profile"]["title"].as_str().unwrap_or("");
                let tz = user["tz"].as_str().unwrap_or("");
                let is_bot = user["is_bot"].as_bool().unwrap_or(false);
                Ok(Self::text_result(format!(
                    "User: {name} (@{display})\nEmail: {email}\nTitle: {title}\nTimezone: {tz}\nBot: {is_bot}"
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SlackTools {
    fn get_info(&self) -> ServerConfig {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("slack-tools", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Slack tools: post_message, list_channels, get_channel_history, search_messages, get_user_info. Set SLACK_TOKEN (xoxb- or xoxp-) or configure via plugin settings.".to_string(),
            )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    PluginRuntime::run(SlackTools::new()).await
}
