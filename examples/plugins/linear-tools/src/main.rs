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

const LINEAR_API: &str = "https://api.linear.app/graphql";

#[derive(Clone)]
struct LinearTools {
    tool_router: ToolRouter<Self>,
    api_key: String,
    default_team_id: String,
    http: reqwest::Client,
}

impl LinearTools {
    fn new() -> Self {
        let api_key = std::env::var("LINEAR_API_KEY")
            .or_else(|_| std::env::var("GOOSE_PLUGIN_LINEAR_TOOLS_API_KEY"))
            .unwrap_or_default();
        let default_team_id = std::env::var("GOOSE_PLUGIN_LINEAR_TOOLS_TEAM_ID")
            .unwrap_or_default();
        LinearTools {
            tool_router: Self::tool_router(),
            api_key,
            default_team_id,
            http: reqwest::Client::builder()
                .user_agent("goose-linear-tools/0.1.0")
                .build()
                .unwrap(),
        }
    }

    async fn gql(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String> {
        if self.api_key.is_empty() {
            return Err("LINEAR_API_KEY not set. Configure it via plugin settings.".to_string());
        }
        let payload = serde_json::json!({ "query": query, "variables": variables });
        let resp = self
            .http
            .post(LINEAR_API)
            .header("Authorization", &self.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        if let Some(errors) = body["errors"].as_array() {
            if !errors.is_empty() {
                let msgs: Vec<&str> = errors
                    .iter()
                    .filter_map(|e| e["message"].as_str())
                    .collect();
                return Err(format!("Linear API errors: {}", msgs.join("; ")));
            }
        }
        Ok(body["data"].clone())
    }

    fn text_result(text: String) -> CallToolResult {
        CallToolResult::success(vec![ContentBlock::text(text)])
    }

    fn err_result(msg: String) -> ErrorData {
        ErrorData::new(ErrorCode::INTERNAL_ERROR, msg, None)
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ListIssuesInput {
    #[schemars(description = "Team ID to list issues for. Omit to use the configured default.")]
    team_id: Option<String>,
    #[schemars(description = "Filter by state name, e.g. 'In Progress', 'Todo', 'Done'")]
    state: Option<String>,
    #[schemars(description = "Filter by assignee name or email")]
    assignee: Option<String>,
    #[schemars(description = "Maximum issues to return (default 20, max 50)")]
    limit: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct GetIssueInput {
    #[schemars(description = "Issue identifier, e.g. ENG-123")]
    identifier: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct CreateIssueInput {
    #[schemars(description = "Issue title")]
    title: String,
    #[schemars(description = "Issue description in Markdown")]
    description: Option<String>,
    #[schemars(description = "Team ID. Omit to use the configured default.")]
    team_id: Option<String>,
    #[schemars(description = "Priority: 0=none, 1=urgent, 2=high, 3=medium, 4=low")]
    priority: Option<u8>,
    #[schemars(description = "Label names to attach, e.g. ['bug', 'frontend']")]
    labels: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct UpdateIssueInput {
    #[schemars(description = "Issue identifier, e.g. ENG-123")]
    identifier: String,
    #[schemars(description = "New state name, e.g. 'In Progress'")]
    state: Option<String>,
    #[schemars(description = "New title")]
    title: Option<String>,
    #[schemars(description = "New priority: 0=none, 1=urgent, 2=high, 3=medium, 4=low")]
    priority: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchIssuesInput {
    #[schemars(description = "Search query text")]
    query: String,
    #[schemars(description = "Maximum results (default 10, max 25)")]
    limit: Option<u8>,
}

#[tool_router(router = tool_router)]
impl LinearTools {
    #[tool(description = "List issues for a team in Linear")]
    async fn list_issues(
        &self,
        params: Parameters<ListIssuesInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let team_id = p
            .team_id
            .or_else(|| {
                if self.default_team_id.is_empty() {
                    None
                } else {
                    Some(self.default_team_id.clone())
                }
            })
            .ok_or_else(|| Self::err_result("team_id required (or configure default)".to_string()))?;
        let limit = p.limit.unwrap_or(20).min(50) as i64;

        let mut filters = format!("team: {{ id: {{ eq: \"{}\" }} }}", team_id);
        if let Some(state) = &p.state {
            filters.push_str(&format!(", state: {{ name: {{ eq: \"{}\" }} }}", state));
        }
        if let Some(assignee) = &p.assignee {
            filters.push_str(&format!(", assignee: {{ name: {{ containsIgnoreCase: \"{}\" }} }}", assignee));
        }

        let query = format!(
            r#"query {{ issues(filter: {{ {filters} }}, first: {limit}) {{
              nodes {{ identifier title state {{ name }} assignee {{ name }} priority createdAt }}
            }} }}"#
        );

        match self.gql(&query, serde_json::json!({})).await {
            Ok(data) => {
                let issues = data["issues"]["nodes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if issues.is_empty() {
                    return Ok(Self::text_result("No issues found.".to_string()));
                }
                let lines: Vec<String> = issues
                    .iter()
                    .map(|issue| {
                        let id = issue["identifier"].as_str().unwrap_or("");
                        let title = issue["title"].as_str().unwrap_or("");
                        let state = issue["state"]["name"].as_str().unwrap_or("Unknown");
                        let assignee = issue["assignee"]["name"]
                            .as_str()
                            .unwrap_or("Unassigned");
                        let priority = match issue["priority"].as_u64().unwrap_or(0) {
                            1 => "🔴 Urgent",
                            2 => "🟠 High",
                            3 => "🟡 Medium",
                            4 => "🔵 Low",
                            _ => "⚪ None",
                        };
                        format!("{id} [{state}] {priority} {title} (@{assignee})")
                    })
                    .collect();
                Ok(Self::text_result(format!(
                    "{} issues:\n{}",
                    issues.len(),
                    lines.join("\n")
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Get details of a specific Linear issue by identifier")]
    async fn get_issue(
        &self,
        params: Parameters<GetIssueInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let query = format!(
            r#"query {{ issue(id: "{}") {{
              identifier title description state {{ name }} assignee {{ name }}
              priority createdAt updatedAt url
              comments {{ nodes {{ body createdAt user {{ name }} }} }}
            }} }}"#,
            p.identifier
        );
        match self.gql(&query, serde_json::json!({})).await {
            Ok(data) => {
                let issue = &data["issue"];
                let id = issue["identifier"].as_str().unwrap_or("");
                let title = issue["title"].as_str().unwrap_or("");
                let state = issue["state"]["name"].as_str().unwrap_or("Unknown");
                let assignee = issue["assignee"]["name"]
                    .as_str()
                    .unwrap_or("Unassigned");
                let description = issue["description"].as_str().unwrap_or("").trim();
                let url = issue["url"].as_str().unwrap_or("");
                let priority = match issue["priority"].as_u64().unwrap_or(0) {
                    1 => "Urgent",
                    2 => "High",
                    3 => "Medium",
                    4 => "Low",
                    _ => "None",
                };
                let comments = issue["comments"]["nodes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let comment_text = if comments.is_empty() {
                    "No comments".to_string()
                } else {
                    comments
                        .iter()
                        .map(|c| {
                            let user = c["user"]["name"].as_str().unwrap_or("unknown");
                            let body = c["body"].as_str().unwrap_or("").trim();
                            format!("  [{user}]: {body}")
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                Ok(Self::text_result(format!(
                    "{id}: {title}\nState: {state} | Assignee: {assignee} | Priority: {priority}\nURL: {url}\n\n{description}\n\nComments:\n{comment_text}"
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Create a new Linear issue")]
    async fn create_issue(
        &self,
        params: Parameters<CreateIssueInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let team_id = p
            .team_id
            .or_else(|| {
                if self.default_team_id.is_empty() {
                    None
                } else {
                    Some(self.default_team_id.clone())
                }
            })
            .ok_or_else(|| Self::err_result("team_id required".to_string()))?;

        let mut input = serde_json::json!({
            "teamId": team_id,
            "title": p.title,
        });
        if let Some(desc) = p.description {
            input["description"] = serde_json::Value::String(desc);
        }
        if let Some(priority) = p.priority {
            input["priority"] = serde_json::Value::Number(priority.into());
        }

        let query = r#"mutation CreateIssue($input: IssueCreateInput!) {
            issueCreate(input: $input) {
                success
                issue { identifier title url }
            }
        }"#;
        match self
            .gql(query, serde_json::json!({ "input": input }))
            .await
        {
            Ok(data) => {
                let issue = &data["issueCreate"]["issue"];
                let id = issue["identifier"].as_str().unwrap_or("");
                let title = issue["title"].as_str().unwrap_or("");
                let url = issue["url"].as_str().unwrap_or("");
                Ok(Self::text_result(format!(
                    "Created {id}: {title}\n{url}"
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Update a Linear issue's state, title, or priority")]
    async fn update_issue(
        &self,
        params: Parameters<UpdateIssueInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;

        let id_query = format!(
            r#"query {{ issue(id: "{}") {{ id }} }}"#,
            p.identifier
        );
        let id_data = self
            .gql(&id_query, serde_json::json!({}))
            .await
            .map_err(|e| Self::err_result(e))?;
        let internal_id = id_data["issue"]["id"]
            .as_str()
            .ok_or_else(|| Self::err_result(format!("Issue '{}' not found", p.identifier)))?
            .to_string();

        let mut input = serde_json::json!({});
        if let Some(title) = p.title {
            input["title"] = serde_json::Value::String(title);
        }
        if let Some(priority) = p.priority {
            input["priority"] = serde_json::Value::Number(priority.into());
        }
        if let Some(state_name) = p.state {
            let state_query = format!(
                r#"query {{ workflowStates(filter: {{ name: {{ eq: "{state_name}" }} }}) {{
                    nodes {{ id name }}
                }} }}"#
            );
            let state_data = self
                .gql(&state_query, serde_json::json!({}))
                .await
                .map_err(|e| Self::err_result(e))?;
            if let Some(state_id) = state_data["workflowStates"]["nodes"][0]["id"].as_str() {
                input["stateId"] = serde_json::Value::String(state_id.to_string());
            }
        }

        let query = r#"mutation UpdateIssue($id: String!, $input: IssueUpdateInput!) {
            issueUpdate(id: $id, input: $input) {
                success
                issue { identifier title state { name } }
            }
        }"#;
        match self
            .gql(
                query,
                serde_json::json!({ "id": internal_id, "input": input }),
            )
            .await
        {
            Ok(data) => {
                let issue = &data["issueUpdate"]["issue"];
                let id = issue["identifier"].as_str().unwrap_or("");
                let title = issue["title"].as_str().unwrap_or("");
                let state = issue["state"]["name"].as_str().unwrap_or("");
                Ok(Self::text_result(format!(
                    "Updated {id}: {title} (state: {state})"
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Search Linear issues by text query")]
    async fn search_issues(
        &self,
        params: Parameters<SearchIssuesInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let limit = p.limit.unwrap_or(10).min(25) as i64;
        let query = format!(
            r#"query {{ issueSearch(query: "{}", first: {limit}) {{
              nodes {{ identifier title state {{ name }} assignee {{ name }} url }}
            }} }}"#,
            p.query.replace('"', "\\\"")
        );
        match self.gql(&query, serde_json::json!({})).await {
            Ok(data) => {
                let issues = data["issueSearch"]["nodes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if issues.is_empty() {
                    return Ok(Self::text_result(format!(
                        "No results for '{}'",
                        p.query
                    )));
                }
                let lines: Vec<String> = issues
                    .iter()
                    .map(|issue| {
                        let id = issue["identifier"].as_str().unwrap_or("");
                        let title = issue["title"].as_str().unwrap_or("");
                        let state = issue["state"]["name"].as_str().unwrap_or("Unknown");
                        let assignee = issue["assignee"]["name"]
                            .as_str()
                            .unwrap_or("Unassigned");
                        format!("{id} [{state}] {title} (@{assignee})")
                    })
                    .collect();
                Ok(Self::text_result(format!(
                    "{} results:\n{}",
                    issues.len(),
                    lines.join("\n")
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for LinearTools {
    fn get_info(&self) -> ServerConfig {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("linear-tools", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Linear issue tracker tools: list_issues, get_issue, create_issue, update_issue, search_issues. Requires LINEAR_API_KEY (lin_api_...).".to_string(),
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

    PluginRuntime::run(LinearTools::new()).await
}
