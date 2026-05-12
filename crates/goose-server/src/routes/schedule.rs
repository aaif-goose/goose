use std::sync::Arc;
use tokio::fs;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::routes::errors::ErrorResponse;
use crate::routes::recipe_utils::validate_recipe;
use crate::state::AppState;
use goose::recipe::Recipe;
use goose::scheduler::{get_default_scheduled_recipes_dir, ScheduledJob};

fn validate_schedule_id(id: &str) -> Result<(), ErrorResponse> {
    let is_valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ');

    if !is_valid {
        return Err(ErrorResponse::bad_request(
            "Schedule name must use only alphanumeric characters, hyphens, underscores, or spaces"
                .to_string(),
        ));
    }
    Ok(())
}

#[derive(Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateScheduleRequest {
    id: String,
    recipe: Recipe,
    cron: String,
}

#[derive(Deserialize, Serialize, utoipa::ToSchema)]
pub struct UpdateScheduleRequest {
    cron: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ListSchedulesResponse {
    jobs: Vec<ScheduledJob>,
}

// Response for the kill endpoint
#[derive(Serialize, utoipa::ToSchema)]
pub struct KillJobResponse {
    message: String,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InspectJobResponse {
    session_id: Option<String>,
    process_start_time: Option<String>,
    running_duration_seconds: Option<i64>,
}

// Response for the run_now endpoint
#[derive(Serialize, utoipa::ToSchema)]
pub struct RunNowResponse {
    session_id: String,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct SessionsQuery {
    limit: usize,
}

// Struct for the frontend session list
#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionDisplayInfo {
    id: String,
    name: String,
    created_at: String,
    working_dir: String,
    schedule_id: Option<String>,
    message_count: usize,
    total_tokens: Option<i32>,
    input_tokens: Option<i32>,
    output_tokens: Option<i32>,
    accumulated_total_tokens: Option<i32>,
    accumulated_input_tokens: Option<i32>,
    accumulated_output_tokens: Option<i32>,
}

#[utoipa::path(
    post,
    path = "/schedule/create",
    request_body = CreateScheduleRequest,
    responses(
        (status = 200, description = "Scheduled job created successfully", body = ScheduledJob),
        (status = 400, description = "Invalid cron expression or recipe file"),
        (status = 409, description = "Job ID already exists"),
        (status = 500, description = "Internal server error")
    ),
    tag = "schedule"
)]
#[axum::debug_handler]
async fn create_schedule(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateScheduleRequest>,
) -> Result<Json<ScheduledJob>, ErrorResponse> {
    let id = req.id.trim().to_string();
    validate_schedule_id(&id)?;

    if req.recipe.check_for_security_warnings() {
        return Err(ErrorResponse::bad_request(
            "This recipe contains hidden characters that could be malicious. Please remove them before trying to save.".to_string(),
        ));
    }
    if let Err(err) = validate_recipe(&req.recipe) {
        return Err(ErrorResponse {
            message: err.message,
            status: err.status,
        });
    }
    let scheduled_recipes_dir = get_default_scheduled_recipes_dir().map_err(|e| {
        ErrorResponse::internal(format!("Failed to get scheduled recipes directory: {}", e))
    })?;

    let recipe_path = scheduled_recipes_dir.join(format!("{}.yaml", id));
    let yaml_content = req
        .recipe
        .to_yaml()
        .map_err(|e| ErrorResponse::internal(format!("Failed to convert recipe to YAML: {}", e)))?;
    fs::write(&recipe_path, yaml_content)
        .await
        .map_err(|e| ErrorResponse::internal(format!("Failed to save recipe file: {}", e)))?;

    let job = ScheduledJob {
        id,
        source: recipe_path.to_string_lossy().into_owned(),
        cron: req.cron,
        last_run: None,
        currently_running: false,
        paused: false,
        current_session_id: None,
        process_start_time: None,
    };

    let scheduler = state.scheduler();
    scheduler
        .add_scheduled_job(job.clone(), false)
        .await
        .map_err(|e| match e {
            goose::scheduler::SchedulerError::CronParseError(msg) => {
                ErrorResponse::bad_request(format!("Invalid cron expression: {}", msg))
            }
            goose::scheduler::SchedulerError::RecipeLoadError(msg) => {
                ErrorResponse::bad_request(format!("Recipe load error: {}", msg))
            }
            goose::scheduler::SchedulerError::JobIdExists(msg) => ErrorResponse {
                message: format!("Job ID already exists: {}", msg),
                status: StatusCode::CONFLICT,
            },
            _ => ErrorResponse::internal(format!("Error creating schedule: {}", e)),
        })?;

    Ok(Json(job))
}

#[utoipa::path(
    get,
    path = "/schedule/list",
    responses(
        (status = 200, description = "A list of scheduled jobs", body = ListSchedulesResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "schedule"
)]
#[axum::debug_handler]
async fn list_schedules(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ListSchedulesResponse>, ErrorResponse> {
    let scheduler = state.scheduler();

    let jobs = scheduler.list_scheduled_jobs().await;
    Ok(Json(ListSchedulesResponse { jobs }))
}

#[utoipa::path(
    delete,
    path = "/schedule/delete/{id}",
    params(
        ("id" = String, Path, description = "ID of the schedule to delete")
    ),
    responses(
        (status = 204, description = "Scheduled job removed successfully"),
        (status = 404, description = "Scheduled job not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "schedule"
)]
#[axum::debug_handler]
async fn delete_schedule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    let scheduler = state.scheduler();
    scheduler
        .remove_scheduled_job(&id, false)
        .await
        .map_err(|e| match e {
            goose::scheduler::SchedulerError::JobNotFound(msg) => {
                ErrorResponse::not_found(format!("Schedule not found: {}", msg))
            }
            _ => ErrorResponse::internal(format!("Error removing schedule: {}", e)),
        })?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/schedule/{id}/run_now",
    params(
        ("id" = String, Path, description = "ID of the schedule to run")
    ),
    responses(
        (status = 200, description = "Scheduled job triggered successfully, returns new session ID", body = RunNowResponse),
        (status = 404, description = "Scheduled job not found"),
        (status = 500, description = "Internal server error when trying to run the job")
    ),
    tag = "schedule"
)]
#[axum::debug_handler]
async fn run_now_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RunNowResponse>, ErrorResponse> {
    let scheduler = state.scheduler();

    let (recipe_display_name, recipe_version_opt) = if let Some(job) = scheduler
        .list_scheduled_jobs()
        .await
        .into_iter()
        .find(|job| job.id == id)
    {
        let recipe_display_name = std::path::Path::new(&job.source)
            .file_name()
            .and_then(|name| name.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| id.clone());

        let recipe_version_opt =
            tokio::fs::read_to_string(&job.source)
                .await
                .ok()
                .and_then(|content: String| {
                    goose::recipe::template_recipe::parse_recipe_content(
                        &content,
                        Some(
                            std::path::Path::new(&job.source)
                                .parent()
                                .unwrap_or_else(|| std::path::Path::new(""))
                                .to_string_lossy()
                                .to_string(),
                        ),
                    )
                    .ok()
                    .map(|(r, _)| r.version)
                });

        (recipe_display_name, recipe_version_opt)
    } else {
        (id.clone(), None)
    };

    let recipe_version_tag = recipe_version_opt.as_deref().unwrap_or("");
    tracing::info!(
        monotonic_counter.goose.recipe_runs = 1,
        recipe_name = %recipe_display_name,
        recipe_version = %recipe_version_tag,
        session_type = "schedule",
        interface = "server",
        "Recipe execution started"
    );

    match scheduler.run_now(&id).await {
        Ok(session_id) => Ok(Json(RunNowResponse { session_id })),
        Err(e) => match e {
            goose::scheduler::SchedulerError::JobNotFound(msg) => Err(ErrorResponse::not_found(
                format!("Schedule not found: {}", msg),
            )),
            goose::scheduler::SchedulerError::AnyhowError(ref err) => {
                // Check if this is a cancellation error
                if err.to_string().contains("was successfully cancelled") {
                    // Return a special session_id to indicate cancellation
                    Ok(Json(RunNowResponse {
                        session_id: "CANCELLED".to_string(),
                    }))
                } else {
                    Err(ErrorResponse::internal(format!(
                        "Error running schedule: {}",
                        err
                    )))
                }
            }
            _ => Err(ErrorResponse::internal(format!(
                "Error running schedule: {}",
                e
            ))),
        },
    }
}

#[utoipa::path(
    get,
    path = "/schedule/{id}/sessions",
    params(
        ("id" = String, Path, description = "ID of the schedule"),
        SessionsQuery // This will automatically pick up 'limit' as a query parameter
    ),
    responses(
        (status = 200, description = "A list of session display info", body = Vec<SessionDisplayInfo>),
        (status = 500, description = "Internal server error")
    ),
    tag = "schedule"
)]
#[axum::debug_handler]
async fn sessions_handler(
    State(state): State<Arc<AppState>>,
    Path(schedule_id_param): Path<String>, // Renamed to avoid confusion with session_id
    Query(query_params): Query<SessionsQuery>,
) -> Result<Json<Vec<SessionDisplayInfo>>, ErrorResponse> {
    let scheduler = state.scheduler();

    let session_tuples = scheduler
        .sessions(&schedule_id_param, query_params.limit)
        .await
        .map_err(|e| ErrorResponse::internal(format!("Error fetching sessions: {}", e)))?;

    let mut display_infos = Vec::new();
    for (session_name, session) in session_tuples {
        display_infos.push(SessionDisplayInfo {
            id: session_name.clone(),
            name: session.name,
            created_at: session.created_at.to_rfc3339(),
            working_dir: session.working_dir.to_string_lossy().into_owned(),
            schedule_id: session.schedule_id,
            message_count: session.message_count,
            total_tokens: session.total_tokens,
            input_tokens: session.input_tokens,
            output_tokens: session.output_tokens,
            accumulated_total_tokens: session.accumulated_total_tokens,
            accumulated_input_tokens: session.accumulated_input_tokens,
            accumulated_output_tokens: session.accumulated_output_tokens,
        });
    }
    Ok(Json(display_infos))
}

#[utoipa::path(
    post,
    path = "/schedule/{id}/pause",
    params(
        ("id" = String, Path, description = "ID of the schedule to pause")
    ),
    responses(
        (status = 204, description = "Scheduled job paused successfully"),
        (status = 404, description = "Scheduled job not found"),
        (status = 400, description = "Cannot pause a currently running job"),
        (status = 500, description = "Internal server error")
    ),
    tag = "schedule"
)]
#[axum::debug_handler]
async fn pause_schedule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    let scheduler = state.scheduler();

    scheduler.pause_schedule(&id).await.map_err(|e| match e {
        goose::scheduler::SchedulerError::JobNotFound(msg) => {
            ErrorResponse::not_found(format!("Schedule not found: {}", msg))
        }
        goose::scheduler::SchedulerError::AnyhowError(err) => {
            ErrorResponse::bad_request(format!("Cannot pause schedule: {}", err))
        }
        _ => ErrorResponse::internal(format!("Error pausing schedule: {}", e)),
    })?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/schedule/{id}/unpause",
    params(
        ("id" = String, Path, description = "ID of the schedule to unpause")
    ),
    responses(
        (status = 204, description = "Scheduled job unpaused successfully"),
        (status = 404, description = "Scheduled job not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "schedule"
)]
#[axum::debug_handler]
async fn unpause_schedule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    let scheduler = state.scheduler();

    scheduler.unpause_schedule(&id).await.map_err(|e| match e {
        goose::scheduler::SchedulerError::JobNotFound(msg) => {
            ErrorResponse::not_found(format!("Schedule not found: {}", msg))
        }
        _ => ErrorResponse::internal(format!("Error unpausing schedule: {}", e)),
    })?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/schedule/{id}",
    params(
        ("id" = String, Path, description = "ID of the schedule to update")
    ),
    request_body = UpdateScheduleRequest,
    responses(
        (status = 200, description = "Scheduled job updated successfully", body = ScheduledJob),
        (status = 404, description = "Scheduled job not found"),
        (status = 400, description = "Cannot update a currently running job or invalid request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "schedule"
)]
#[axum::debug_handler]
async fn update_schedule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateScheduleRequest>,
) -> Result<Json<ScheduledJob>, ErrorResponse> {
    let scheduler = state.scheduler();

    scheduler
        .update_schedule(&id, req.cron)
        .await
        .map_err(|e| match e {
            goose::scheduler::SchedulerError::JobNotFound(msg) => {
                ErrorResponse::not_found(format!("Schedule not found: {}", msg))
            }
            goose::scheduler::SchedulerError::AnyhowError(err) => {
                ErrorResponse::bad_request(format!("Cannot update schedule: {}", err))
            }
            goose::scheduler::SchedulerError::CronParseError(msg) => {
                ErrorResponse::bad_request(format!("Invalid cron expression: {}", msg))
            }
            _ => ErrorResponse::internal(format!("Error updating schedule: {}", e)),
        })?;

    let jobs = scheduler.list_scheduled_jobs().await;
    let updated_job = jobs
        .into_iter()
        .find(|job| job.id == id)
        .ok_or_else(|| ErrorResponse::internal("Schedule not found after update"))?;

    Ok(Json(updated_job))
}

#[utoipa::path(
    post,
    path = "/schedule/{id}/kill",
    responses(
        (status = 200, description = "Running job killed successfully"),
    ),
    tag = "schedule"
)]
#[axum::debug_handler]
pub async fn kill_running_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<KillJobResponse>, ErrorResponse> {
    let scheduler = state.scheduler();

    scheduler.kill_running_job(&id).await.map_err(|e| match e {
        goose::scheduler::SchedulerError::JobNotFound(msg) => {
            ErrorResponse::not_found(format!("Job not found: {}", msg))
        }
        goose::scheduler::SchedulerError::AnyhowError(err) => {
            ErrorResponse::bad_request(format!("Cannot kill job: {}", err))
        }
        _ => ErrorResponse::internal(format!("Error killing job: {}", e)),
    })?;

    Ok(Json(KillJobResponse {
        message: format!("Successfully killed running job '{}'", id),
    }))
}

#[utoipa::path(
    get,
    path = "/schedule/{id}/inspect",
    params(
        ("id" = String, Path, description = "ID of the schedule to inspect")
    ),
    responses(
        (status = 200, description = "Running job information", body = InspectJobResponse),
        (status = 404, description = "Scheduled job not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "schedule"
)]
#[axum::debug_handler]
pub async fn inspect_running_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<InspectJobResponse>, ErrorResponse> {
    let scheduler = state.scheduler();

    let info = scheduler
        .get_running_job_info(&id)
        .await
        .map_err(|e| match e {
            goose::scheduler::SchedulerError::JobNotFound(msg) => {
                ErrorResponse::not_found(format!("Job not found: {}", msg))
            }
            _ => ErrorResponse::internal(format!("Error inspecting job: {}", e)),
        })?;

    if let Some((session_id, start_time)) = info {
        let duration = chrono::Utc::now().signed_duration_since(start_time);
        Ok(Json(InspectJobResponse {
            session_id: Some(session_id),
            process_start_time: Some(start_time.to_rfc3339()),
            running_duration_seconds: Some(duration.num_seconds()),
        }))
    } else {
        Ok(Json(InspectJobResponse {
            session_id: None,
            process_start_time: None,
            running_duration_seconds: None,
        }))
    }
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/schedule/create", post(create_schedule))
        .route("/schedule/list", get(list_schedules))
        .route("/schedule/delete/{id}", delete(delete_schedule)) // Corrected
        .route("/schedule/{id}", put(update_schedule))
        .route("/schedule/{id}/run_now", post(run_now_handler)) // Corrected
        .route("/schedule/{id}/pause", post(pause_schedule))
        .route("/schedule/{id}/unpause", post(unpause_schedule))
        .route("/schedule/{id}/kill", post(kill_running_job))
        .route("/schedule/{id}/inspect", get(inspect_running_job))
        .route("/schedule/{id}/sessions", get(sessions_handler)) // Corrected
        .with_state(state)
}
