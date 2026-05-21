//! Copilot HTTP routes — thin handlers over `goose::copilot`.

use std::sync::Arc;

use axum::{
    extract::State,
    response::Json,
    routing::{get, post},
    Router,
};
use goose::config::signup_copilot::CopilotInstallFlow;
use goose::config::Config;
use goose::copilot::{
    cached_installation_id, disconnect_install, extract_agent_id, fetch_analytics,
    fetch_oauth_client_id, fetch_repos, forward_routing_prefs, load_prefs, register_installation,
    replace_comment_reaction, report_analytics_event, resolve_install_credentials,
    run_comment_reply, run_review, save_prefs, AnalyticsEvent, CopilotAnalytics,
    CopilotCommentRequest, CopilotDisconnectResponse, CopilotPrefs, CopilotPrefsRequest,
    CopilotPrefsResponse, CopilotReposResponse, CopilotReviewRequest, CopilotReviewResponse,
    CopilotSetupResponse, CopilotStatusResponse, RegisterInstallRequest, TunnelSnapshot,
    INSTALLATION_ID_CONFIG_KEY,
};

use crate::routes::errors::ErrorResponse;
use crate::state::AppState;

async fn tunnel_snapshot(state: &AppState) -> TunnelSnapshot {
    let info = state.tunnel_manager.get_info().await;
    TunnelSnapshot {
        url: info.url,
        secret: info.secret,
    }
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
    let oauth_client_id = fetch_oauth_client_id()
        .await
        .map_err(|e| ErrorResponse::internal(format!("oauth-config lookup failed: {e}")))?;

    let mut flow = CopilotInstallFlow::new().with_oauth_client_id(oauth_client_id);
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

    let installation_id = register_installation(RegisterInstallRequest {
        oauth_code: callback.oauth_code,
        agent_id,
        tunnel_secret: tunnel_info.secret,
        tunnel_url: tunnel_info.url,
    })
    .await
    .map_err(|e| ErrorResponse::internal(e.to_string()))?;

    let _ = Config::global().set_param(
        INSTALLATION_ID_CONFIG_KEY,
        serde_json::json!(installation_id),
    );

    Ok(Json(CopilotSetupResponse { installation_id }))
}

#[utoipa::path(
    get,
    path = "/copilot/status",
    responses(
        (status = 200, description = "Cached GitHub App installation id", body = CopilotStatusResponse),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn get_status() -> Json<CopilotStatusResponse> {
    Json(CopilotStatusResponse {
        installation_id: cached_installation_id(Config::global()),
    })
}

#[utoipa::path(
    delete,
    path = "/copilot/setup",
    responses(
        (status = 200, description = "Local install cleared and switchboard registration removed", body = CopilotDisconnectResponse),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn disconnect(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CopilotDisconnectResponse>, ErrorResponse> {
    disconnect_install(tunnel_snapshot(&state).await)
        .await
        .map_err(|e| ErrorResponse::internal(e.to_string()))?;
    Ok(Json(CopilotDisconnectResponse { disconnected: true }))
}

#[utoipa::path(
    post,
    path = "/copilot/review",
    request_body = CopilotReviewRequest,
    responses(
        (status = 200, description = "Review accepted, running in background", body = CopilotReviewResponse),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn review(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CopilotReviewRequest>,
) -> Result<Json<CopilotReviewResponse>, ErrorResponse> {
    let pr_label = format!("{} #{}", req.repo, req.pr_number);
    let tunnel = tunnel_snapshot(&state).await;
    tokio::spawn(async move {
        let result = run_review(req.clone()).await;
        match &result {
            Ok(_) => report_analytics_event(tunnel, AnalyticsEvent::PrReviewed).await,
            Err(e) => tracing::error!("[copilot] review {} failed: {:#}", pr_label, e),
        }
        if let Some(id) = req.comment_id {
            let reaction = if result.is_ok() { "+1" } else { "confused" };
            let _ = replace_comment_reaction(&req.repo, id, reaction, &req.github_token).await;
        }
    });
    Ok(Json(CopilotReviewResponse { accepted: true }))
}

#[utoipa::path(
    post,
    path = "/copilot/comment",
    request_body = CopilotCommentRequest,
    responses(
        (status = 200, description = "Comment accepted, replying in background", body = CopilotReviewResponse),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn comment(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CopilotCommentRequest>,
) -> Result<Json<CopilotReviewResponse>, ErrorResponse> {
    let pr_label = format!("{} #{}", req.repo, req.pr_number);
    let tunnel = tunnel_snapshot(&state).await;
    tokio::spawn(async move {
        let result = run_comment_reply(req.clone()).await;
        match &result {
            Ok(commit_pushed) => {
                if !req.is_pr {
                    report_analytics_event(tunnel.clone(), AnalyticsEvent::IssueHandled).await;
                }
                if *commit_pushed {
                    report_analytics_event(tunnel, AnalyticsEvent::CommitPushed).await;
                }
            }
            Err(e) => tracing::error!("[copilot] comment {} failed: {:#}", pr_label, e),
        }
        if let Some(id) = req.comment_id {
            let reaction = if result.is_ok() { "+1" } else { "confused" };
            let _ = replace_comment_reaction(&req.repo, id, reaction, &req.github_token).await;
        }
    });
    Ok(Json(CopilotReviewResponse { accepted: true }))
}

#[utoipa::path(
    get,
    path = "/copilot/prefs",
    responses(
        (status = 200, description = "Current Copilot preferences", body = CopilotPrefs),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn get_prefs() -> Result<Json<CopilotPrefs>, ErrorResponse> {
    Ok(Json(load_prefs()))
}

#[utoipa::path(
    put,
    path = "/copilot/prefs",
    request_body = CopilotPrefsRequest,
    responses(
        (status = 200, description = "Preferences saved", body = CopilotPrefsResponse),
        (status = 400, description = "Validation error"),
        (status = 500, description = "Internal error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn put_prefs(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CopilotPrefsRequest>,
) -> Result<Json<CopilotPrefsResponse>, ErrorResponse> {
    req.prefs
        .validate()
        .map_err(|e| ErrorResponse::bad_request(e.to_string()))?;

    save_prefs(&req.prefs).map_err(|e| ErrorResponse::internal(e.to_string()))?;

    let (switchboard_synced, switchboard_error) =
        match resolve_install_credentials(tunnel_snapshot(&state).await).await {
            Ok(creds) => match forward_routing_prefs(&creds, &req.prefs.routing_subset()).await {
                Ok(()) => (true, None),
                Err(e) => (false, Some(e.to_string())),
            },
            Err(e) => (false, Some(e.to_string())),
        };

    Ok(Json(CopilotPrefsResponse {
        prefs: req.prefs,
        switchboard_synced,
        switchboard_error,
    }))
}

#[utoipa::path(
    get,
    path = "/copilot/repos",
    responses(
        (status = 200, description = "Repos accessible to the installation", body = CopilotReposResponse),
        (status = 412, description = "Setup not completed"),
        (status = 502, description = "Switchboard / GitHub error"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn get_repos(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CopilotReposResponse>, ErrorResponse> {
    let creds = resolve_install_credentials(tunnel_snapshot(&state).await)
        .await
        .map_err(|e| ErrorResponse {
            message: e.to_string(),
            status: axum::http::StatusCode::PRECONDITION_FAILED,
        })?;

    let body = fetch_repos(&creds)
        .await
        .map_err(|e| ErrorResponse::internal(e.to_string()))?;
    Ok(Json(body))
}

#[utoipa::path(
    get,
    path = "/copilot/analytics",
    responses(
        (status = 200, description = "Per-install analytics rollups", body = CopilotAnalytics),
        (status = 412, description = "Setup not completed"),
    ),
    tag = "copilot"
)]
#[axum::debug_handler]
async fn get_analytics(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CopilotAnalytics>, ErrorResponse> {
    let creds = resolve_install_credentials(tunnel_snapshot(&state).await)
        .await
        .map_err(|e| ErrorResponse {
            message: e.to_string(),
            status: axum::http::StatusCode::PRECONDITION_FAILED,
        })?;
    let body = fetch_analytics(&creds)
        .await
        .map_err(|e| ErrorResponse::internal(e.to_string()))?;
    Ok(Json(body))
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/copilot/review", post(review))
        .route("/copilot/setup", post(setup).delete(disconnect))
        .route("/copilot/status", get(get_status))
        .route("/copilot/comment", post(comment))
        .route("/copilot/prefs", get(get_prefs).put(put_prefs))
        .route("/copilot/repos", get(get_repos))
        .route("/copilot/analytics", get(get_analytics))
        .with_state(state)
}
