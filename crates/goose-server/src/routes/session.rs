use crate::routes::errors::ErrorResponse;
use crate::routes::recipe_utils::{apply_recipe_to_agent, build_recipe_with_parameter_values};
use crate::state::AppState;
use axum::extract::{DefaultBodyLimit, State};
use axum::routing::post;
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, get, put},
    Json, Router,
};
use goose::agents::ExtensionConfig;
use goose::conversation::message::{Message as GooseMessage, MessageContent};
use goose::model::ModelConfig;
use goose::providers::create as create_provider;
use goose::recipe::Recipe;
use goose::session::session_manager::{SessionInsights, SessionTag, SessionType, TagCount};
use goose::session::{EnabledExtensionsState, Session};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponse {
    /// List of available session information objects
    sessions: Vec<Session>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionNameRequest {
    /// Updated name for the session (max 200 characters)
    name: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionUserRecipeValuesRequest {
    /// Recipe parameter values entered by the user
    user_recipe_values: HashMap<String, String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateSessionUserRecipeValuesResponse {
    recipe: Recipe,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportSessionRequest {
    json: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ForkRequest {
    timestamp: Option<i64>,
    truncate: bool,
    copy: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ForkResponse {
    session_id: String,
}

const MAX_NAME_LENGTH: usize = 200;

#[utoipa::path(
    get,
    path = "/sessions",
    responses(
        (status = 200, description = "List of available sessions retrieved successfully", body = SessionListResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SessionListResponse>, StatusCode> {
    let sessions = state
        .session_manager()
        .list_sessions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SessionListResponse { sessions }))
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session history retrieved successfully", body = Session),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<Session>, StatusCode> {
    let session = state
        .session_manager()
        .get_session(&session_id, true)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(session))
}
#[utoipa::path(
    get,
    path = "/sessions/insights",
    responses(
        (status = 200, description = "Session insights retrieved successfully", body = SessionInsights),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn get_session_insights(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SessionInsights>, StatusCode> {
    let insights = state
        .session_manager()
        .get_insights()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(insights))
}

#[utoipa::path(
    put,
    path = "/sessions/{session_id}/name",
    request_body = UpdateSessionNameRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session name updated successfully"),
        (status = 400, description = "Bad request - Name too long (max 200 characters)"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn update_session_name(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateSessionNameRequest>,
) -> Result<StatusCode, StatusCode> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if name.len() > MAX_NAME_LENGTH {
        return Err(StatusCode::BAD_REQUEST);
    }

    state
        .session_manager()
        .update(&session_id)
        .user_provided_name(name.to_string())
        .apply()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    put,
    path = "/sessions/{session_id}/user_recipe_values",
    request_body = UpdateSessionUserRecipeValuesRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session user recipe values updated successfully", body = UpdateSessionUserRecipeValuesResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
// Update session user recipe parameter values
async fn update_session_user_recipe_values(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateSessionUserRecipeValuesRequest>,
) -> Result<Json<UpdateSessionUserRecipeValuesResponse>, ErrorResponse> {
    state
        .session_manager()
        .update(&session_id)
        .user_recipe_values(Some(request.user_recipe_values))
        .apply()
        .await
        .map_err(|err| ErrorResponse {
            message: err.to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

    let session = state
        .session_manager()
        .get_session(&session_id, false)
        .await
        .map_err(|err| ErrorResponse {
            message: err.to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;
    let recipe = session.recipe.ok_or_else(|| ErrorResponse {
        message: "Recipe not found".to_string(),
        status: StatusCode::NOT_FOUND,
    })?;

    let user_recipe_values = session.user_recipe_values.unwrap_or_default();
    match build_recipe_with_parameter_values(&recipe, user_recipe_values).await {
        Ok(Some(recipe)) => {
            let agent = state
                .get_agent_for_route(session_id.clone())
                .await
                .map_err(|status| ErrorResponse {
                    message: format!("Failed to get agent: {}", status),
                    status,
                })?;
            if let Some(prompt) = apply_recipe_to_agent(&agent, &recipe, false).await {
                agent
                    .extend_system_prompt("recipe".to_string(), prompt)
                    .await;
            }
            Ok(Json(UpdateSessionUserRecipeValuesResponse { recipe }))
        }
        Ok(None) => Err(ErrorResponse {
            message: "Missing required parameters".to_string(),
            status: StatusCode::BAD_REQUEST,
        }),
        Err(e) => Err(ErrorResponse {
            message: e.to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }),
    }
}

#[utoipa::path(
    delete,
    path = "/sessions/{session_id}",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session deleted successfully"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    state
        .session_manager()
        .delete_session(&session_id)
        .await
        .map_err(|e| {
            if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    // Cancel any in-flight replies before dropping the bus, so spawned
    // agent tasks stop consuming tokens for a deleted session.
    if let Some(bus) = state.get_event_bus(&session_id).await {
        bus.cancel_all_requests().await;
    }
    state.remove_event_bus(&session_id).await;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/export",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session exported successfully", body = String),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn export_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let exported = state
        .session_manager()
        .export_session(&session_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(exported))
}

#[utoipa::path(
    post,
    path = "/sessions/import",
    request_body = ImportSessionRequest,
    responses(
        (status = 200, description = "Session imported successfully", body = Session),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 400, description = "Bad request - Invalid JSON"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn import_session(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ImportSessionRequest>,
) -> Result<Json<Session>, StatusCode> {
    let session = state
        .session_manager()
        .import_session(&request.json, Some(SessionType::User))
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    Ok(Json(session))
}

#[utoipa::path(
    post,
    path = "/sessions/{session_id}/fork",
    request_body = ForkRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session forked successfully", body = ForkResponse),
        (status = 400, description = "Bad request - truncate=true requires timestamp"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn fork_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<ForkRequest>,
) -> Result<Json<ForkResponse>, ErrorResponse> {
    if request.truncate && request.timestamp.is_none() {
        return Err(ErrorResponse {
            message: "truncate=true requires a timestamp".to_string(),
            status: StatusCode::BAD_REQUEST,
        });
    }

    let session_manager = state.session_manager();

    let target_session_id = if request.copy {
        let original = session_manager
            .get_session(&session_id, false)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get session: {}", e);
                #[cfg(feature = "telemetry")]
                goose::posthog::emit_error("session_get_failed", &e.to_string());
                ErrorResponse {
                    message: if e.to_string().contains("not found") {
                        format!("Session {} not found", session_id)
                    } else {
                        format!("Failed to get session: {}", e)
                    },
                    status: if e.to_string().contains("not found") {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    },
                }
            })?;

        let copied = session_manager
            .copy_session(&session_id, original.name)
            .await
            .map_err(|e| {
                tracing::error!("Failed to copy session: {}", e);
                #[cfg(feature = "telemetry")]
                goose::posthog::emit_error("session_copy_failed", &e.to_string());
                ErrorResponse {
                    message: format!("Failed to copy session: {}", e),
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })?;

        copied.id
    } else {
        session_id.clone()
    };

    if request.truncate {
        session_manager
            .truncate_conversation(&target_session_id, request.timestamp.unwrap_or(0))
            .await
            .map_err(|e| {
                tracing::error!("Failed to truncate conversation: {}", e);
                #[cfg(feature = "telemetry")]
                goose::posthog::emit_error("session_truncate_failed", &e.to_string());
                ErrorResponse {
                    message: format!("Failed to truncate conversation: {}", e),
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })?;
    }

    Ok(Json(ForkResponse {
        session_id: target_session_id,
    }))
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionExtensionsResponse {
    extensions: Vec<ExtensionConfig>,
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/extensions",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session extensions retrieved successfully", body = SessionExtensionsResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn get_session_extensions(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionExtensionsResponse>, StatusCode> {
    let session = state
        .session_manager()
        .get_session(&session_id, false)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let extensions = EnabledExtensionsState::extensions_or_default(
        Some(&session.extension_data),
        goose::config::Config::global(),
    );

    Ok(Json(SessionExtensionsResponse { extensions }))
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionTagsResponse {
    session_id: String,
    tags: Vec<SessionTag>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddTagsRequest {
    tags: Vec<String>,
    #[serde(default = "default_tag_source")]
    source: String,
}

fn default_tag_source() -> String {
    "manual".to_string()
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AllTagsResponse {
    tags: Vec<TagCount>,
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/tags",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Tags retrieved successfully", body = SessionTagsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = [])),
    tag = "Session Management"
)]
async fn get_session_tags(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionTagsResponse>, StatusCode> {
    let tags = state
        .session_manager()
        .get_tags_for_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SessionTagsResponse { session_id, tags }))
}

#[utoipa::path(
    put,
    path = "/sessions/{session_id}/tags",
    request_body = AddTagsRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Tags added successfully"),
        (status = 400, description = "Bad request - no tags provided"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = [])),
    tag = "Session Management"
)]
async fn add_session_tags(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<AddTagsRequest>,
) -> Result<StatusCode, StatusCode> {
    if request.tags.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    state
        .session_manager()
        .add_tags_to_session(&session_id, &request.tags, &request.source)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    delete,
    path = "/sessions/{session_id}/tags/{tag}",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session"),
        ("tag" = String, Path, description = "Tag to remove")
    ),
    responses(
        (status = 200, description = "Tag removed successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = [])),
    tag = "Session Management"
)]
async fn remove_session_tag(
    State(state): State<Arc<AppState>>,
    Path((session_id, tag)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    state
        .session_manager()
        .remove_tag_from_session(&session_id, &tag)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    get,
    path = "/sessions/tags",
    responses(
        (status = 200, description = "All unique tags with counts", body = AllTagsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = [])),
    tag = "Session Management"
)]
async fn get_all_tags(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AllTagsResponse>, StatusCode> {
    let tags = state
        .session_manager()
        .get_all_tags_with_counts()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(AllTagsResponse { tags }))
}

const PREDEFINED_TAGS: &str = "python, javascript, typescript, rust, html-css, frontend, backend, fullstack, api, database, debugging, refactoring, testing, devops, deployment, data-analysis, machine-learning, automation, scripting, documentation, code-review, git, setup, configuration, web-scraping, game-dev, cli-tool, file-management, research, writing, general";

fn extract_tags_from_response(text: &str) -> Vec<String> {
    let start = text.find('[');
    let end = text.rfind(']');
    if let (Some(s), Some(e)) = (start, end) {
        if let Ok(tags) = serde_json::from_str::<Vec<String>>(&text[s..=e]) {
            return tags.into_iter().filter(|t| !t.trim().is_empty()).collect();
        }
    }
    vec![]
}

#[utoipa::path(
    post,
    path = "/sessions/{session_id}/tags/auto",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Tags auto-generated and saved", body = SessionTagsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = [])),
    tag = "Session Management"
)]
async fn auto_tag_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionTagsResponse>, StatusCode> {
    let session = state
        .session_manager()
        .get_session(&session_id, true)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let text_snippet: String = session
        .conversation
        .as_ref()
        .map(|conv| {
            conv.messages()
                .iter()
                .take(10)
                .flat_map(|m| m.content.iter())
                .filter_map(|c| {
                    if let MessageContent::Text(t) = c {
                        Some(t.text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    let text_snippet = if text_snippet.len() > 2000 {
        text_snippet[..2000].to_string()
    } else {
        text_snippet
    };

    if text_snippet.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let config = goose::config::Config::global();
    let provider_name = config
        .get_goose_provider()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let model_name = config
        .get_goose_model()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let model_config = ModelConfig::new(&model_name)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .with_canonical_limits(&provider_name);

    let provider = create_provider(&provider_name, model_config.clone(), vec![])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let system = format!(
        "You are a session categorizer. Choose 1-3 relevant tags from this exact list: {}. \
        Return ONLY a JSON array of strings with no explanation. Example: [\"rust\",\"debugging\"]",
        PREDEFINED_TAGS
    );
    let user_msg = GooseMessage::user().with_text(format!(
        "Categorize this session based on its messages:\n\n{}",
        text_snippet
    ));

    let (response, _) = provider
        .complete(&model_config, &session_id, &system, &[user_msg], &[])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response_text = response
        .content
        .iter()
        .filter_map(|c| {
            if let MessageContent::Text(t) = c {
                Some(t.text.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("");

    let tags = extract_tags_from_response(&response_text);
    if tags.is_empty() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    state
        .session_manager()
        .add_tags_to_session(&session_id, &tags, "auto")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let all_tags = state
        .session_manager()
        .get_tags_for_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SessionTagsResponse {
        session_id,
        tags: all_tags,
    }))
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/sessions", get(list_sessions))
        .route("/sessions/search", get(search_sessions))
        .route("/sessions/tags", get(get_all_tags))
        .route("/sessions/{session_id}", get(get_session))
        .route("/sessions/{session_id}", delete(delete_session))
        .route("/sessions/{session_id}/export", get(export_session))
        .route(
            "/sessions/import",
            post(import_session).layer(DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route("/sessions/insights", get(get_session_insights))
        .route("/sessions/{session_id}/name", put(update_session_name))
        .route(
            "/sessions/{session_id}/user_recipe_values",
            put(update_session_user_recipe_values),
        )
        .route("/sessions/{session_id}/fork", post(fork_session))
        .route(
            "/sessions/{session_id}/extensions",
            get(get_session_extensions),
        )
        .route("/sessions/{session_id}/tags", get(get_session_tags))
        .route("/sessions/{session_id}/tags", put(add_session_tags))
        .route("/sessions/{session_id}/tags/auto", post(auto_tag_session))
        .route(
            "/sessions/{session_id}/tags/{tag}",
            delete(remove_session_tag),
        )
        .with_state(state)
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchSessionsQuery {
    /// Search query string (keywords separated by spaces)
    query: String,
    /// Maximum number of results to return (default: 10, max: 50)
    #[serde(default = "default_limit")]
    limit: usize,
    /// Filter results to sessions after this date (ISO 8601 format)
    after_date: Option<String>,
    /// Filter results to sessions before this date (ISO 8601 format)
    before_date: Option<String>,
}

fn default_limit() -> usize {
    10
}

#[utoipa::path(
    get,
    path = "/sessions/search",
    params(
        ("query" = String, Query, description = "Search query string"),
        ("limit" = Option<usize>, Query, description = "Maximum results (default: 10, max: 50)"),
        ("after_date" = Option<String>, Query, description = "Filter after date (ISO 8601)"),
        ("before_date" = Option<String>, Query, description = "Filter before date (ISO 8601)")
    ),
    responses(
        (status = 200, description = "Matching sessions", body = Vec<Session>),
        (status = 400, description = "Bad request - Invalid query"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn search_sessions(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<SearchSessionsQuery>,
) -> Result<Json<Vec<Session>>, StatusCode> {
    let query = params.query.trim();
    if query.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let limit = params.limit.min(50);

    let after_date = params
        .after_date
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let before_date = params
        .before_date
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let search_results = state
        .session_manager()
        .search_chat_history(
            query,
            Some(limit),
            after_date,
            before_date,
            None,
            vec![SessionType::User, SessionType::Scheduled],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get full Session objects for matching session IDs
    let session_ids: Vec<String> = search_results
        .results
        .into_iter()
        .map(|r| r.session_id)
        .collect();

    let all_sessions = state
        .session_manager()
        .list_sessions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let matching_sessions: Vec<Session> = all_sessions
        .into_iter()
        .filter(|s| session_ids.contains(&s.id))
        .collect();

    Ok(Json(matching_sessions))
}
