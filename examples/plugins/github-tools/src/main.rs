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
struct GitHubTools {
    tool_router: ToolRouter<Self>,
    token: String,
    default_owner: String,
    http: reqwest::Client,
}

impl GitHubTools {
    fn new() -> Self {
        let token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GOOSE_PLUGIN_GITHUB_TOOLS_TOKEN"))
            .unwrap_or_default();
        let default_owner = std::env::var("GOOSE_PLUGIN_GITHUB_TOOLS_DEFAULT_OWNER")
            .unwrap_or_else(|_| "aaif-goose".to_string());
        GitHubTools {
            tool_router: Self::tool_router(),
            token,
            default_owner,
            http: reqwest::Client::builder()
                .user_agent("goose-github-tools/0.1.0")
                .build()
                .unwrap(),
        }
    }

    async fn gh_get(&self, path: &str) -> std::result::Result<serde_json::Value, String> {
        let url = format!("https://api.github.com{path}");
        let mut req = self.http.get(&url);
        if !self.token.is_empty() {
            req = req.bearer_auth(&self.token);
        }
        req = req.header("Accept", "application/vnd.github+json");
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let body = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| e.to_string())?;
        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(format!("GitHub API {status}: {msg}"));
        }
        Ok(body)
    }

    async fn gh_post(
        &self,
        path: &str,
        payload: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String> {
        let url = format!("https://api.github.com{path}");
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        let body = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| e.to_string())?;
        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(format!("GitHub API {status}: {msg}"));
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
struct ListPrsInput {
    #[schemars(description = "Repository name, e.g. goose")]
    repo: String,
    #[schemars(description = "Owner or org. Omit to use the plugin's configured default.")]
    owner: Option<String>,
    #[schemars(description = "Filter by state: open, closed, or all. Default: open")]
    state: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct GetPrInput {
    #[schemars(description = "Repository name")]
    repo: String,
    owner: Option<String>,
    #[schemars(description = "Pull request number")]
    number: u64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct PostCommentInput {
    #[schemars(description = "Repository name")]
    repo: String,
    owner: Option<String>,
    #[schemars(description = "Issue or PR number")]
    number: u64,
    #[schemars(description = "Comment body text (Markdown supported)")]
    body: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct GetCiStatusInput {
    #[schemars(description = "Repository name")]
    repo: String,
    owner: Option<String>,
    #[schemars(description = "Git ref: branch name, tag, or commit SHA")]
    git_ref: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchIssuesInput {
    #[schemars(description = "Repository name")]
    repo: String,
    owner: Option<String>,
    #[schemars(description = "Search query, e.g. 'plugin system label:feature'")]
    query: String,
    #[schemars(description = "Maximum results (default 10, max 30)")]
    limit: Option<u8>,
}

#[tool_router(router = tool_router)]
impl GitHubTools {
    #[tool(description = "List pull requests in a GitHub repository")]
    async fn list_prs(
        &self,
        params: Parameters<ListPrsInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let owner = p.owner.as_deref().unwrap_or(&self.default_owner);
        let state = p.state.as_deref().unwrap_or("open");
        let path = format!(
            "/repos/{}/{}/pulls?state={}&per_page=20",
            owner, p.repo, state
        );
        match self.gh_get(&path).await {
            Ok(data) => {
                let prs = data.as_array().cloned().unwrap_or_default();
                if prs.is_empty() {
                    return Ok(Self::text_result(format!(
                        "No {state} PRs in {owner}/{}",
                        p.repo
                    )));
                }
                let lines: Vec<String> = prs
                    .iter()
                    .map(|pr| {
                        let num = pr["number"].as_u64().unwrap_or(0);
                        let title = pr["title"].as_str().unwrap_or("");
                        let user = pr["user"]["login"].as_str().unwrap_or("");
                        let d = if pr["draft"].as_bool().unwrap_or(false) {
                            " [DRAFT]"
                        } else {
                            ""
                        };
                        format!("#{num}{d} {title} (@{user})")
                    })
                    .collect();
                Ok(Self::text_result(format!(
                    "{} {state} PRs in {owner}/{}:\n{}",
                    prs.len(),
                    p.repo,
                    lines.join("\n")
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Get details of a specific pull request including diff stats and description")]
    async fn get_pr(
        &self,
        params: Parameters<GetPrInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let owner = p.owner.as_deref().unwrap_or(&self.default_owner);
        let path = format!("/repos/{}/{}/pulls/{}", owner, p.repo, p.number);
        match self.gh_get(&path).await {
            Ok(pr) => {
                let title = pr["title"].as_str().unwrap_or("");
                let state = pr["state"].as_str().unwrap_or("");
                let body = pr["body"].as_str().unwrap_or("").trim();
                let additions = pr["additions"].as_u64().unwrap_or(0);
                let deletions = pr["deletions"].as_u64().unwrap_or(0);
                let changed_files = pr["changed_files"].as_u64().unwrap_or(0);
                let author = pr["user"]["login"].as_str().unwrap_or("");
                let base = pr["base"]["ref"].as_str().unwrap_or("");
                let head = pr["head"]["ref"].as_str().unwrap_or("");
                let url = pr["html_url"].as_str().unwrap_or("");
                Ok(Self::text_result(format!(
                    "PR #{} — {title}\nState: {state} | @{author} | {head} → {base}\nDiff: +{additions} -{deletions} in {changed_files} files\nURL: {url}\n\n{}",
                    p.number,
                    if body.is_empty() { "(no description)" } else { body }
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Post a comment on a GitHub issue or pull request")]
    async fn post_comment(
        &self,
        params: Parameters<PostCommentInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        if self.token.is_empty() {
            return Err(Self::err_result(
                "GitHub token required to post comments".to_string(),
            ));
        }
        let p = params.0;
        let owner = p.owner.as_deref().unwrap_or(&self.default_owner);
        let path = format!("/repos/{}/{}/issues/{}/comments", owner, p.repo, p.number);
        match self
            .gh_post(&path, serde_json::json!({ "body": p.body }))
            .await
        {
            Ok(comment) => {
                let url = comment["html_url"].as_str().unwrap_or("");
                Ok(Self::text_result(format!("Comment posted: {url}")))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Get CI check run status for a branch, tag, or commit SHA")]
    async fn get_ci_status(
        &self,
        params: Parameters<GetCiStatusInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let owner = p.owner.as_deref().unwrap_or(&self.default_owner);
        let path = format!(
            "/repos/{}/{}/commits/{}/check-runs",
            owner, p.repo, p.git_ref
        );
        match self.gh_get(&path).await {
            Ok(data) => {
                let runs = data["check_runs"].as_array().cloned().unwrap_or_default();
                if runs.is_empty() {
                    return Ok(Self::text_result(format!(
                        "No CI checks for {owner}/{} @ {}",
                        p.repo, p.git_ref
                    )));
                }
                let lines: Vec<String> = runs
                    .iter()
                    .map(|run| {
                        let name = run["name"].as_str().unwrap_or("");
                        let status = run["status"].as_str().unwrap_or("");
                        let conclusion = run["conclusion"].as_str().unwrap_or("in_progress");
                        let icon = match conclusion {
                            "success" => "✓",
                            "failure" | "cancelled" => "✗",
                            "skipped" => "–",
                            _ => "○",
                        };
                        format!("{icon} {name}: {status}/{conclusion}")
                    })
                    .collect();
                Ok(Self::text_result(format!(
                    "CI for {owner}/{} @ {}:\n{}",
                    p.repo,
                    p.git_ref,
                    lines.join("\n")
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }

    #[tool(description = "Search issues and pull requests in a repository")]
    async fn search_issues(
        &self,
        params: Parameters<SearchIssuesInput>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let p = params.0;
        let owner = p.owner.as_deref().unwrap_or(&self.default_owner);
        let limit = p.limit.unwrap_or(10).min(30);
        let q = format!("repo:{}/{} {}", owner, p.repo, p.query);
        let path = format!(
            "/search/issues?q={}&per_page={limit}",
            urlencoding::encode(&q)
        );
        match self.gh_get(&path).await {
            Ok(data) => {
                let total = data["total_count"].as_u64().unwrap_or(0);
                let items = data["items"].as_array().cloned().unwrap_or_default();
                if items.is_empty() {
                    return Ok(Self::text_result(format!(
                        "No results for '{}'",
                        p.query
                    )));
                }
                let lines: Vec<String> = items
                    .iter()
                    .map(|item| {
                        let num = item["number"].as_u64().unwrap_or(0);
                        let title = item["title"].as_str().unwrap_or("");
                        let state = item["state"].as_str().unwrap_or("");
                        let kind = if item["pull_request"].is_object() {
                            "PR"
                        } else {
                            "Issue"
                        };
                        format!("{kind} #{num} [{state}] {title}")
                    })
                    .collect();
                Ok(Self::text_result(format!(
                    "{total} results (showing {}):\n{}",
                    items.len(),
                    lines.join("\n")
                )))
            }
            Err(e) => Err(Self::err_result(e)),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for GitHubTools {
    fn get_info(&self) -> ServerConfig {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("github-tools", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "GitHub tools: list_prs, get_pr, post_comment, get_ci_status, search_issues. Set GITHUB_TOKEN for write access and higher rate limits (60 req/hr unauthenticated, 5000 authenticated).".to_string(),
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

    PluginRuntime::run(GitHubTools::new()).await
}
