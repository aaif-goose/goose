mod prompt_ops;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::{
    Agent, CancelNotification, ClientSideConnection, ExtRequest, ListSessionsRequest,
    LoadSessionRequest, NewSessionRequest, SessionConfigKind, SessionConfigOption,
    SessionConfigSelectOptions, SetSessionConfigOptionRequest,
};
use serde_json::value::RawValue;
use tokio::sync::Mutex;

use super::dispatcher::SessionEventDispatcher;
use crate::services::acp::split_composite_key;
pub(super) use prompt_ops::send_prompt_inner;

/// Lightweight session metadata returned by `list_sessions`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionInfo {
    pub session_id: String,
    pub title: Option<String>,
    pub updated_at: Option<String>,
    pub message_count: usize,
}

#[derive(Clone)]
pub(super) struct PreparedSession {
    goose_session_id: String,
    provider_id: String,
    working_dir: PathBuf,
}

pub(super) struct ManagerState {
    pub(super) sessions: HashMap<String, PreparedSession>,
    pub(super) op_locks: HashMap<String, Arc<Mutex<()>>>,
    pub(super) pending_cancels: HashSet<String>,
    pub(super) preparing_sessions: HashSet<String>,
}

impl ManagerState {
    pub(super) fn session_lock(&mut self, composite_key: &str) -> Arc<Mutex<()>> {
        self.op_locks
            .entry(composite_key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub(super) fn mark_cancel_requested(&mut self, composite_key: &str) {
        self.pending_cancels.insert(composite_key.to_string());
    }

    pub(super) fn take_cancel_requested(&mut self, composite_key: &str) -> bool {
        self.pending_cancels.remove(composite_key)
    }
}

fn extract_current_select_value(
    options: &[SessionConfigOption],
    option_id: &str,
) -> Option<String> {
    let option = options
        .iter()
        .find(|candidate| candidate.id.0.as_ref() == option_id)?;
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };

    let current_value = select.current_value.to_string();
    match &select.options {
        SessionConfigSelectOptions::Ungrouped(values) => values
            .iter()
            .any(|value| value.value == select.current_value)
            .then_some(current_value),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .any(|value| value.value == select.current_value)
            .then_some(current_value),
        _ => Some(current_value),
    }
}

fn needs_provider_update(current_provider_id: Option<&str>, requested_provider_id: &str) -> bool {
    current_provider_id != Some(requested_provider_id)
}

fn prepared_session_for_key(
    sessions: &HashMap<String, PreparedSession>,
    composite_key: &str,
    local_session_id: &str,
) -> Option<PreparedSession> {
    sessions
        .get(composite_key)
        .cloned()
        .or_else(|| sessions.get(local_session_id).cloned())
}

fn register_prepared_session_keys(
    sessions: &mut HashMap<String, PreparedSession>,
    composite_key: &str,
    local_session_id: &str,
    prepared: PreparedSession,
) {
    sessions.insert(composite_key.to_string(), prepared.clone());
    sessions.insert(local_session_id.to_string(), prepared);
}

async fn update_working_dir_inner(
    connection: &Arc<ClientSideConnection>,
    goose_session_id: &str,
    working_dir: &PathBuf,
) -> Result<(), String> {
    let params = RawValue::from_string(
        serde_json::json!({
            "sessionId": goose_session_id,
            "workingDir": working_dir,
        })
        .to_string(),
    )
    .map_err(|error| format!("Failed to build working dir update request: {error}"))?;

    connection
        .ext_method(ExtRequest::new("goose/working_dir/update", params.into()))
        .await
        .map_err(|error| format!("Failed to update Goose ACP working directory: {error:?}"))?;

    Ok(())
}

pub(super) struct PrepareSessionInput {
    pub(super) composite_key: String,
    pub(super) local_session_id: String,
    pub(super) provider_id: String,
    pub(super) working_dir: PathBuf,
    pub(super) existing_agent_session_id: Option<String>,
}

async fn try_load_existing_session(
    connection: &Arc<ClientSideConnection>,
    dispatcher: &Arc<SessionEventDispatcher>,
    local_session_id: &str,
    candidate_session_id: &str,
    provider_id: &str,
    working_dir: &PathBuf,
) -> Result<Option<String>, String> {
    let response = match connection
        .load_session(LoadSessionRequest::new(
            candidate_session_id.to_string(),
            working_dir.clone(),
        ))
        .await
    {
        Ok(response) => response,
        Err(_) => return Ok(None),
    };

    dispatcher
        .bind_session(candidate_session_id, local_session_id, Some(provider_id))
        .await;

    if let Some(models) = &response.models {
        dispatcher.emit_model_state(local_session_id, Some(provider_id), models);
    }
    if let Some(options) = &response.config_options {
        dispatcher.emit_model_state_from_options(local_session_id, Some(provider_id), options);
    }
    update_working_dir_inner(connection, candidate_session_id, working_dir).await?;

    let loaded_provider_id = response
        .config_options
        .as_deref()
        .and_then(|options| extract_current_select_value(options, "provider"));
    if needs_provider_update(loaded_provider_id.as_deref(), provider_id) {
        let response = connection
            .set_session_config_option(SetSessionConfigOptionRequest::new(
                candidate_session_id.to_string(),
                "provider",
                provider_id,
            ))
            .await
            .map_err(|error| format!("Failed to update provider via Goose ACP: {error:?}"))?;
        dispatcher.emit_model_state_from_options(
            local_session_id,
            Some(provider_id),
            &response.config_options,
        );
    }

    Ok(Some(candidate_session_id.to_string()))
}

pub(super) async fn prepare_session_inner(
    connection: &Arc<ClientSideConnection>,
    dispatcher: &Arc<SessionEventDispatcher>,
    state: &Arc<Mutex<ManagerState>>,
    input: PrepareSessionInput,
) -> Result<String, String> {
    let PrepareSessionInput {
        composite_key,
        local_session_id,
        provider_id,
        working_dir,
        existing_agent_session_id,
    } = input;

    let session_lock = {
        let mut guard = state.lock().await;
        guard.session_lock(&composite_key)
    };
    let _lock_guard = session_lock.lock().await;

    {
        let mut guard = state.lock().await;
        guard.preparing_sessions.insert(composite_key.clone());
    }

    let prepare_result: Result<(String, Option<PreparedSession>), String> = async {
        let existing_prepared = {
            let guard = state.lock().await;
            prepared_session_for_key(&guard.sessions, &composite_key, &local_session_id)
        };

        if let Some(prepared) = existing_prepared {
            dispatcher
                .bind_session(
                    &prepared.goose_session_id,
                    &local_session_id,
                    Some(&provider_id),
                )
                .await;

            {
                let mut guard = state.lock().await;
                register_prepared_session_keys(
                    &mut guard.sessions,
                    &composite_key,
                    &local_session_id,
                    prepared.clone(),
                );
            }

            if prepared.working_dir != working_dir {
                update_working_dir_inner(connection, &prepared.goose_session_id, &working_dir)
                    .await?;

                let mut guard = state.lock().await;
                register_prepared_session_keys(
                    &mut guard.sessions,
                    &composite_key,
                    &local_session_id,
                    PreparedSession {
                        goose_session_id: prepared.goose_session_id.clone(),
                        provider_id: prepared.provider_id.clone(),
                        working_dir: working_dir.clone(),
                    },
                );
            }

            if needs_provider_update(Some(&prepared.provider_id), &provider_id) {
                let response = connection
                    .set_session_config_option(SetSessionConfigOptionRequest::new(
                        prepared.goose_session_id.clone(),
                        "provider",
                        provider_id.as_str(),
                    ))
                    .await
                    .map_err(|error| {
                        format!("Failed to update provider via Goose ACP: {error:?}")
                    })?;
                dispatcher.emit_model_state_from_options(
                    &local_session_id,
                    Some(&provider_id),
                    &response.config_options,
                );

                let mut guard = state.lock().await;
                let updated_working_dir = guard
                    .sessions
                    .get(&composite_key)
                    .map(|session| session.working_dir.clone())
                    .unwrap_or_else(|| prepared.working_dir.clone());
                let updated = PreparedSession {
                    goose_session_id: prepared.goose_session_id.clone(),
                    provider_id: provider_id.clone(),
                    working_dir: updated_working_dir,
                };
                register_prepared_session_keys(
                    &mut guard.sessions,
                    &composite_key,
                    &local_session_id,
                    updated,
                );
            }

            return Ok((prepared.goose_session_id, None));
        }

        let goose_session_id = if let Some(existing_id) = existing_agent_session_id {
            try_load_existing_session(
                connection,
                dispatcher,
                &local_session_id,
                &existing_id,
                &provider_id,
                &working_dir,
            )
            .await?
            .ok_or_else(|| format!("Failed to load Goose session '{existing_id}'"))?
        } else if let Some(existing_id) = try_load_existing_session(
            connection,
            dispatcher,
            &local_session_id,
            &local_session_id,
            &provider_id,
            &working_dir,
        )
        .await?
        {
            existing_id
        } else {
            let mut request = NewSessionRequest::new(working_dir.clone());
            if provider_id != "goose" {
                let mut meta = serde_json::Map::new();
                meta.insert(
                    "provider".into(),
                    serde_json::Value::String(provider_id.clone()),
                );
                request = request.meta(meta);
            }

            let response = connection
                .new_session(request)
                .await
                .map_err(|error| format!("Failed to create Goose session: {error:?}"))?;

            let new_id = response.session_id.to_string();

            dispatcher
                .bind_session(&new_id, &local_session_id, Some(&provider_id))
                .await;

            if let Some(models) = &response.models {
                dispatcher.emit_model_state(&local_session_id, Some(&provider_id), models);
            }
            if let Some(options) = &response.config_options {
                dispatcher.emit_model_state_from_options(
                    &local_session_id,
                    Some(&provider_id),
                    options,
                );
            }

            new_id
        };

        Ok((
            goose_session_id.clone(),
            Some(PreparedSession {
                goose_session_id,
                provider_id,
                working_dir,
            }),
        ))
    }
    .await;

    {
        let mut guard = state.lock().await;
        guard.preparing_sessions.remove(&composite_key);
    }

    let (goose_session_id, prepared_session) = prepare_result?;

    if let Some(prepared_session) = prepared_session {
        let mut guard = state.lock().await;
        guard.sessions.insert(composite_key, prepared_session);
    }

    Ok(goose_session_id)
}

/// List all sessions known to the goose binary.
pub(super) async fn list_sessions_inner(
    connection: &Arc<ClientSideConnection>,
) -> Result<Vec<AcpSessionInfo>, String> {
    let response = connection
        .list_sessions(ListSessionsRequest::default())
        .await
        .map_err(|error| format!("Failed to list sessions via Goose ACP: {error:?}"))?;

    Ok(response
        .sessions
        .into_iter()
        .map(|info| {
            let message_count = info
                .meta
                .as_ref()
                .and_then(|m| m.get("messageCount"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            AcpSessionInfo {
                session_id: info.session_id.to_string(),
                title: info.title,
                updated_at: info.updated_at,
                message_count,
            }
        })
        .collect())
}

/// Load an existing session from the goose binary.
///
/// This binds the goose session ID to the local session ID in the dispatcher
/// so that replayed `SessionNotification` events are routed to the correct
/// frontend session. It also registers the session in the manager state so
/// that subsequent `send_prompt` calls can reuse the goose session.
pub(super) async fn load_session_inner(
    connection: &Arc<ClientSideConnection>,
    dispatcher: &Arc<SessionEventDispatcher>,
    state: &Arc<Mutex<ManagerState>>,
    local_session_id: &str,
    goose_session_id: &str,
    working_dir: PathBuf,
) -> Result<(), String> {
    dispatcher
        .bind_session(goose_session_id, local_session_id, None)
        .await;

    let response = connection
        .load_session(LoadSessionRequest::new(
            goose_session_id.to_string(),
            working_dir.clone(),
        ))
        .await
        .map_err(|error| format!("Failed to load Goose session: {error:?}"))?;

    // The ACP RPC layer resolves responses synchronously but dispatches
    // notifications asynchronously via spawned tasks. After load_session
    // returns, replay notifications may still be queued. Yield repeatedly
    // to let the single-threaded runtime drain them before counting.
    wait_for_replay_drain(|| async { dispatcher.get_replay_event_count(goose_session_id).await })
        .await;

    // Finalize any in-progress replay assistant message
    dispatcher.finalize_replay(goose_session_id).await;

    dispatcher.emit_replay_complete(local_session_id);

    if let Some(models) = &response.models {
        dispatcher.emit_model_state(local_session_id, None, models);
    }
    if let Some(options) = &response.config_options {
        dispatcher.emit_model_state_from_options(local_session_id, None, options);
    }

    // Register the session so future prompts reuse this goose session
    let mut guard = state.lock().await;
    guard.sessions.insert(
        local_session_id.to_string(),
        PreparedSession {
            goose_session_id: goose_session_id.to_string(),
            provider_id: "goose".to_string(), // will be updated on next prepare
            working_dir,
        },
    );

    Ok(())
}

pub(super) async fn cancel_session_inner(
    connection: &Arc<ClientSideConnection>,
    dispatcher: &Arc<SessionEventDispatcher>,
    state: &Arc<Mutex<ManagerState>>,
    composite_key: &str,
) -> Result<bool, String> {
    let (goose_session_id, is_preparing) = {
        let mut guard = state.lock().await;
        let goose_session_id = guard
            .sessions
            .get(composite_key)
            .map(|session| session.goose_session_id.clone());
        let is_preparing = guard.preparing_sessions.contains(composite_key);
        if goose_session_id.is_some() || is_preparing {
            guard.mark_cancel_requested(composite_key);
        }
        (goose_session_id, is_preparing)
    };

    let Some(goose_session_id) = goose_session_id else {
        return Ok(is_preparing);
    };

    let had_writer = dispatcher.mark_canceled(&goose_session_id).await;
    if !had_writer {
        return Ok(true);
    }

    connection
        .cancel(CancelNotification::new(goose_session_id))
        .await
        .map_err(|error| format!("Failed to cancel Goose ACP session: {error:?}"))?;

    Ok(true)
}

pub(super) async fn set_model_inner(
    connection: &Arc<ClientSideConnection>,
    dispatcher: &Arc<SessionEventDispatcher>,
    state: &Arc<Mutex<ManagerState>>,
    local_session_id: &str,
    model_id: &str,
) -> Result<(), String> {
    let prepared_sessions = {
        let guard = state.lock().await;
        guard
            .sessions
            .iter()
            .filter_map(|(composite_key, session)| {
                let (session_id, _) = split_composite_key(composite_key);
                (session_id == local_session_id).then_some((
                    composite_key.clone(),
                    session.goose_session_id.clone(),
                    session.provider_id.clone(),
                ))
            })
            .collect::<Vec<_>>()
    };

    if prepared_sessions.is_empty() {
        return Err(format!(
            "Failed to update model for session '{local_session_id}': no prepared ACP session"
        ));
    }

    let mut updated_goose_sessions = HashSet::new();
    for (composite_key, goose_session_id, provider_id) in prepared_sessions {
        if !updated_goose_sessions.insert(goose_session_id.clone()) {
            continue;
        }

        let session_lock = {
            let mut guard = state.lock().await;
            guard.session_lock(&composite_key)
        };
        let _lock_guard = session_lock.lock().await;

        let response = connection
            .set_session_config_option(SetSessionConfigOptionRequest::new(
                goose_session_id,
                "model",
                model_id,
            ))
            .await
            .map_err(|error| format!("Failed to update model via Goose ACP: {error:?}"))?;
        dispatcher.emit_model_state_from_options(
            local_session_id,
            Some(&provider_id),
            &response.config_options,
        );
    }

    Ok(())
}

/// Yield repeatedly until an async counter stabilises for 3 consecutive rounds.
///
/// After `load_session` returns, the ACP RPC layer may still have spawned
/// notification tasks that haven't run yet. This function yields to the
/// runtime between polls so those tasks get a chance to execute, and only
/// returns once the count has been stable for 3 consecutive yields — giving
/// us confidence that all replay events have been dispatched.
///
/// A safety cap of 100 iterations prevents infinite spinning if a bug causes
/// the counter to increment indefinitely.
const MAX_DRAIN_ITERATIONS: u32 = 100;

async fn wait_for_replay_drain<F, Fut>(mut get_count: F) -> u32
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = u32>,
{
    let mut prev_total = 0u32;
    let mut stable_rounds = 0u8;
    let mut iterations = 0u32;
    loop {
        tokio::task::yield_now().await;
        let total = get_count().await;
        iterations += 1;
        if total == prev_total {
            stable_rounds += 1;
            if stable_rounds >= 3 {
                return total;
            }
        } else {
            stable_rounds = 0;
            prev_total = total;
        }
        if iterations >= MAX_DRAIN_ITERATIONS {
            log::warn!(
                "wait_for_replay_drain hit iteration cap ({MAX_DRAIN_ITERATIONS}); \
                 returning partial count {total}"
            );
            return total;
        }
    }
}
