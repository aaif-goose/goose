//! Renders the `goose-copilot-review` recipe with per-PR params and spawns
//! an agent session in the background. Returns the session id immediately.

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use axum::{extract::State, response::Json, routing::post, Router};
use goose::agents::{Agent, SessionConfig};
use goose::config::paths::Paths;
use goose::config::signup_copilot::CopilotInstallFlow;
use goose::config::{resolve_extensions_for_new_session, Config};
use goose::conversation::message::Message;
use goose::providers::create;
use goose::recipe::template_recipe::render_recipe_content_with_params;
use goose::recipe::Recipe;
use goose::session::session_manager::SessionType;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use crate::routes::errors::ErrorResponse;
use crate::state::AppState;

const DEFAULT_RECIPE_FILENAME: &str = "goose-copilot-review.yaml";
const RECIPE_PATH_ENV: &str = "GOOSE_COPILOT_RECIPE_PATH";
const SWITCHBOARD_URL: &str = "https://goose-copilot-switchboard.example.workers.dev";
const SWITCHBOARD_URL_ENV: &str = "GOOSE_COPILOT_SWITCHBOARD_URL";

#[derive(Debug, Deserialize, ToSchema)]
pub struct CopilotReviewRequest {
    pub github_token: String,
    /// `owner/repo` form, e.g. `block/goose`.
    pub repo: String,
    pub pr_number: u64,
    pub head_sha: String,
    pub pr_url: String,
    /// The recipe updates this Check Run on completion.
    #[serde(default)]
    pub check_run_id: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CopilotReviewResponse {
    pub session_id: String,
}

#[utoipa::path(
    post,
    path = "/copilot/review",
    request_body = CopilotReviewRequest,
    responses(
        (status = 200, description = "Review session spawned", body = CopilotReviewResponse),
        (status = 400, description = "Recipe missing or invalid"),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn review(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CopilotReviewRequest>,
) -> Result<Json<CopilotReviewResponse>, ErrorResponse> {
    let recipe_path = locate_recipe()?;
    let raw = tokio::fs::read_to_string(&recipe_path).await.map_err(|e| {
        ErrorResponse::bad_request(format!(
            "Recipe not found at {}: {e}. Set {RECIPE_PATH_ENV} or place the recipe at the default path.",
            recipe_path.display(),
        ))
    })?;

    let params = build_params(&req, &recipe_path);
    let rendered = render_recipe_content_with_params(&raw, &params)?;
    let recipe: Recipe = serde_yaml::from_str(&rendered)?;

    let prompt_text = recipe_prompt(&recipe)?;
    let session_id = spawn_review(recipe, prompt_text, req).await?;

    Ok(Json(CopilotReviewResponse { session_id }))
}

fn locate_recipe() -> Result<PathBuf, ErrorResponse> {
    if let Ok(custom) = env::var(RECIPE_PATH_ENV) {
        let p = PathBuf::from(custom);
        if p.exists() {
            return Ok(p);
        }
        return Err(ErrorResponse::bad_request(format!(
            "{RECIPE_PATH_ENV} points to {} which does not exist",
            p.display()
        )));
    }
    Ok(Paths::config_dir()
        .join("recipes")
        .join(DEFAULT_RECIPE_FILENAME))
}

fn build_params(req: &CopilotReviewRequest, recipe_path: &Path) -> HashMap<String, String> {
    let mut params = HashMap::new();
    params.insert("github_token".to_string(), req.github_token.clone());
    params.insert("repo".to_string(), req.repo.clone());
    params.insert("pr_number".to_string(), req.pr_number.to_string());
    params.insert("head_sha".to_string(), req.head_sha.clone());
    params.insert("pr_url".to_string(), req.pr_url.clone());
    params.insert(
        "check_run_id".to_string(),
        req.check_run_id.map(|n| n.to_string()).unwrap_or_default(),
    );
    if let Some(parent) = recipe_path.parent() {
        params.insert(
            "recipe_dir".to_string(),
            parent.to_string_lossy().into_owned(),
        );
    }
    params
}

fn recipe_prompt(recipe: &Recipe) -> Result<String, ErrorResponse> {
    recipe
        .prompt
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            recipe
                .instructions
                .as_deref()
                .filter(|s| !s.trim().is_empty())
        })
        .map(|s| s.to_string())
        .ok_or_else(|| {
            ErrorResponse::bad_request(
                "Recipe must define either `prompt` or `instructions`".to_string(),
            )
        })
}

async fn spawn_review(
    recipe: Recipe,
    prompt_text: String,
    req: CopilotReviewRequest,
) -> Result<String> {
    let agent = Agent::new();

    let config = Config::global();
    let provider_name = config.get_goose_provider()?;
    let model_name = config.get_goose_model()?;
    let model_config =
        goose::model::ModelConfig::new(&model_name)?.with_canonical_limits(&provider_name);

    let session = agent
        .config
        .session_manager
        .create_session(
            std::env::current_dir()?,
            format!("Copilot review {} #{}", req.repo, req.pr_number),
            SessionType::Scheduled,
            agent.config.goose_mode,
        )
        .await?;

    let extensions = resolve_extensions_for_new_session(recipe.extensions.as_deref(), None);
    for ext in &extensions {
        agent.add_extension(ext.clone(), &session.id).await?;
    }

    let agent_provider = create(&provider_name, model_config, extensions).await?;
    agent.update_provider(agent_provider, &session.id).await?;

    let session_id = session.id.clone();

    tokio::spawn(async move {
        let user_message = Message::user().with_text(&prompt_text);

        let session_config = SessionConfig {
            id: session_id.clone(),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };

        let cancel_token = CancellationToken::new();
        match agent
            .reply(user_message, session_config, Some(cancel_token))
            .await
        {
            Ok(stream) => {
                use futures::StreamExt;
                let mut stream = std::pin::pin!(stream);
                while stream.next().await.is_some() {}
            }
            Err(e) => {
                tracing::error!("[copilot] review session {} failed: {}", session_id, e);
            }
        }

        if let Err(e) = agent
            .config
            .session_manager
            .update(&session_id)
            .recipe(Some(recipe))
            .apply()
            .await
        {
            tracing::warn!("[copilot] failed to persist recipe on session: {}", e);
        }
    });

    Ok(session.id)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CopilotSetupResponse {
    pub installation_id: u64,
}

#[utoipa::path(
    post,
    path = "/copilot/setup",
    responses(
        (status = 200, description = "Goose Copilot connected", body = CopilotSetupResponse),
        (status = 408, description = "Install timed out"),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn setup(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CopilotSetupResponse>, ErrorResponse> {
    let mut flow = CopilotInstallFlow::new();
    let callback = flow
        .complete_flow()
        .await
        .map_err(|e| ErrorResponse::internal(e.to_string()))?;

    let tunnel_info = state.tunnel_manager.get_info().await;
    if tunnel_info.state != crate::tunnel::TunnelState::Running {
        state
            .tunnel_manager
            .start()
            .await
            .map_err(|e| ErrorResponse::internal(format!("tunnel start failed: {e}")))?;
    }
    let tunnel_info = state.tunnel_manager.get_info().await;
    let agent_id = extract_agent_id(&tunnel_info.url)
        .ok_or_else(|| ErrorResponse::internal("tunnel URL is missing the agent id".to_string()))?;

    let switchboard = switchboard_url();
    let body = serde_json::json!({
        "installation_id": callback.installation_id,
        "oauth_code": callback.oauth_code,
        "agent_id": agent_id,
        "tunnel_secret": tunnel_info.secret,
        "tunnel_url": tunnel_info.url,
    });
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{}/copilot/register", switchboard))
        .json(&body)
        .send()
        .await
        .map_err(|e| ErrorResponse::internal(format!("switchboard unreachable: {e}")))?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        return Err(ErrorResponse::internal(format!(
            "switchboard rejected registration: {status} {detail}"
        )));
    }

    Ok(Json(CopilotSetupResponse {
        installation_id: callback.installation_id,
    }))
}

fn switchboard_url() -> String {
    env::var(SWITCHBOARD_URL_ENV).unwrap_or_else(|_| SWITCHBOARD_URL.to_string())
}

fn extract_agent_id(tunnel_url: &str) -> Option<String> {
    tunnel_url
        .rsplit_once("/tunnel/")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or("").to_string())
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/copilot/review", post(review))
        .route("/copilot/setup", post(setup))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_req() -> CopilotReviewRequest {
        CopilotReviewRequest {
            github_token: "ghs_test".to_string(),
            repo: "block/goose".to_string(),
            pr_number: 42,
            head_sha: "abc123".to_string(),
            pr_url: "https://github.com/block/goose/pull/42".to_string(),
            check_run_id: Some(99),
        }
    }

    #[test]
    fn build_params_maps_required_fields() {
        let path = PathBuf::from("/tmp/recipes/r.yaml");
        let params = build_params(&sample_req(), &path);

        assert_eq!(params.get("github_token").unwrap(), "ghs_test");
        assert_eq!(params.get("repo").unwrap(), "block/goose");
        assert_eq!(params.get("pr_number").unwrap(), "42");
        assert_eq!(params.get("head_sha").unwrap(), "abc123");
        assert_eq!(
            params.get("pr_url").unwrap(),
            "https://github.com/block/goose/pull/42"
        );
        assert_eq!(params.get("check_run_id").unwrap(), "99");
        assert_eq!(params.get("recipe_dir").unwrap(), "/tmp/recipes");
    }

    #[test]
    fn build_params_handles_missing_check_run_id() {
        let mut req = sample_req();
        req.check_run_id = None;
        let params = build_params(&req, Path::new("/tmp/r.yaml"));
        assert_eq!(params.get("check_run_id").unwrap(), "");
    }

    #[test]
    fn recipe_prompt_prefers_prompt_over_instructions() {
        let recipe: Recipe = serde_yaml::from_str(
            "version: '1.0.0'\ntitle: t\ndescription: d\nprompt: from prompt\ninstructions: from instructions\n",
        )
        .unwrap();
        assert_eq!(recipe_prompt(&recipe).unwrap(), "from prompt");
    }

    #[test]
    fn recipe_prompt_falls_back_to_instructions() {
        let recipe: Recipe = serde_yaml::from_str(
            "version: '1.0.0'\ntitle: t\ndescription: d\ninstructions: from instructions\n",
        )
        .unwrap();
        assert_eq!(recipe_prompt(&recipe).unwrap(), "from instructions");
    }

    #[test]
    fn recipe_prompt_rejects_when_both_missing() {
        let recipe: Recipe =
            serde_yaml::from_str("version: '1.0.0'\ntitle: t\ndescription: d\n").unwrap();
        assert!(recipe_prompt(&recipe).is_err());
    }

    #[test]
    fn locate_recipe_uses_env_override_when_present() {
        let tmp = std::env::temp_dir().join("goose-copilot-locate-test.yaml");
        std::fs::write(&tmp, "stub").unwrap();
        std::env::set_var(RECIPE_PATH_ENV, &tmp);

        let located = locate_recipe().unwrap();
        assert_eq!(located, tmp);

        std::env::remove_var(RECIPE_PATH_ENV);
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn locate_recipe_errors_when_env_path_missing() {
        std::env::set_var(RECIPE_PATH_ENV, "/nonexistent/path/recipe.yaml");
        let result = locate_recipe();
        std::env::remove_var(RECIPE_PATH_ENV);
        assert!(result.is_err());
    }
}
