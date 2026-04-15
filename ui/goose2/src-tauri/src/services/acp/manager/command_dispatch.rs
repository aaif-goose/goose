use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use agent_client_protocol::{Agent, ClientSideConnection, ExtRequest, ForkSessionRequest};
use serde_json::value::RawValue;
use tokio::sync::{mpsc, Mutex};

use super::dispatcher::SessionEventDispatcher;
use super::session_ops::{
    cancel_session_inner, list_sessions_inner, load_session_inner, prepare_session_inner,
    send_prompt_inner, set_model_inner, AcpSessionInfo, ManagerState, PrepareSessionInput,
};
use super::{call_ext_method, GooseProvidersResponse, ManagerCommand};

pub(super) async fn dispatch_commands(
    mut command_rx: mpsc::UnboundedReceiver<ManagerCommand>,
    connection: Arc<ClientSideConnection>,
    dispatcher: Arc<SessionEventDispatcher>,
) {
    let state = Arc::new(Mutex::new(ManagerState {
        sessions: HashMap::new(),
        op_locks: HashMap::new(),
        pending_cancels: HashSet::new(),
        preparing_sessions: HashSet::new(),
    }));

    while let Some(command) = command_rx.recv().await {
        match command {
            ManagerCommand::ListProviders { response } => {
                let connection = Arc::clone(&connection);
                tokio::task::spawn_local(async move {
                    let result = async {
                        let params = RawValue::from_string("{}".to_string()).map_err(|error| {
                            format!("Failed to build ACP request body: {error}")
                        })?;
                        let response_value = connection
                            .ext_method(ExtRequest::new("goose/providers/list", params.into()))
                            .await
                            .map_err(|error| {
                                format!("Failed to list providers via Goose ACP: {error:?}")
                            })?;
                        let parsed: GooseProvidersResponse =
                            serde_json::from_str(response_value.0.get()).map_err(|error| {
                                format!("Failed to decode Goose provider list: {error}")
                            })?;
                        Ok::<_, String>(parsed.providers)
                    }
                    .await;
                    let _ = response.send(result);
                });
            }
            ManagerCommand::ListSessions { response } => {
                let connection = Arc::clone(&connection);
                tokio::task::spawn_local(async move {
                    let result = list_sessions_inner(&connection).await;
                    let _ = response.send(result);
                });
            }
            ManagerCommand::LoadSession {
                local_session_id,
                goose_session_id,
                working_dir,
                response,
            } => {
                let connection = Arc::clone(&connection);
                let dispatcher = dispatcher.clone();
                let state = Arc::clone(&state);
                tokio::task::spawn_local(async move {
                    let result = load_session_inner(
                        &connection,
                        &dispatcher,
                        &state,
                        &local_session_id,
                        &goose_session_id,
                        working_dir,
                    )
                    .await;
                    let _ = response.send(result);
                });
            }
            ManagerCommand::PrepareSession {
                composite_key,
                local_session_id,
                provider_id,
                working_dir,
                existing_agent_session_id,
                response,
            } => {
                let connection = Arc::clone(&connection);
                let dispatcher = dispatcher.clone();
                let state = Arc::clone(&state);
                tokio::task::spawn_local(async move {
                    let result = prepare_session_inner(
                        &connection,
                        &dispatcher,
                        &state,
                        PrepareSessionInput {
                            composite_key,
                            local_session_id,
                            provider_id,
                            working_dir,
                            existing_agent_session_id,
                        },
                    )
                    .await
                    .map(|_| ());
                    let _ = response.send(result);
                });
            }
            ManagerCommand::SendPrompt {
                composite_key,
                local_session_id,
                provider_id,
                working_dir,
                existing_agent_session_id,
                writer,
                prompt,
                images,
                response,
            } => {
                let connection = Arc::clone(&connection);
                let dispatcher = dispatcher.clone();
                let state = Arc::clone(&state);
                tokio::task::spawn_local(async move {
                    let result = send_prompt_inner(
                        &connection,
                        &dispatcher,
                        &state,
                        composite_key,
                        local_session_id,
                        provider_id,
                        working_dir,
                        existing_agent_session_id,
                        writer,
                        prompt,
                        images,
                    )
                    .await;
                    let _ = response.send(result);
                });
            }
            ManagerCommand::CancelSession {
                composite_key,
                response,
            } => {
                let connection = Arc::clone(&connection);
                let dispatcher = dispatcher.clone();
                let state = Arc::clone(&state);
                tokio::task::spawn_local(async move {
                    let result =
                        cancel_session_inner(&connection, &dispatcher, &state, &composite_key)
                            .await;
                    let _ = response.send(result);
                });
            }
            ManagerCommand::ExportSession {
                session_id,
                response,
            } => {
                let connection = Arc::clone(&connection);
                tokio::task::spawn_local(async move {
                    let result = async {
                        let raw = call_ext_method(
                            &connection,
                            "goose/session/export",
                            serde_json::json!({ "sessionId": session_id }),
                        )
                        .await?;
                        // Backend returns { "data": "<json string>" }
                        let resp: serde_json::Value = serde_json::from_str(&raw)
                            .map_err(|e| format!("Failed to decode export response: {e}"))?;
                        resp.get("data")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .ok_or_else(|| "Export response missing 'data' field".to_string())
                    }
                    .await;
                    let _ = response.send(result);
                });
            }
            ManagerCommand::ImportSession { json, response } => {
                let connection = Arc::clone(&connection);
                tokio::task::spawn_local(async move {
                    let result = async {
                        let raw = call_ext_method(
                            &connection,
                            "goose/session/import",
                            serde_json::json!({ "data": json }),
                        )
                        .await?;
                        serde_json::from_str(&raw)
                            .map_err(|e| format!("Failed to decode import response: {e}"))
                    }
                    .await;
                    let _ = response.send(result);
                });
            }
            ManagerCommand::ForkSession {
                session_id,
                response,
            } => {
                let connection = Arc::clone(&connection);
                tokio::task::spawn_local(async move {
                    let result = async {
                        let req = ForkSessionRequest::new(
                            session_id,
                            std::env::current_dir().unwrap_or_default(),
                        );
                        let resp = connection
                            .fork_session(req)
                            .await
                            .map_err(|e| format!("session/fork failed: {e:?}"))?;
                        let message_count = resp
                            .meta
                            .as_ref()
                            .and_then(|m| m.get("messageCount"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                        let title = resp
                            .meta
                            .as_ref()
                            .and_then(|m| m.get("title"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        Ok(AcpSessionInfo {
                            session_id: resp.session_id.to_string(),
                            title,
                            updated_at: None,
                            message_count,
                        })
                    }
                    .await;
                    let _ = response.send(result);
                });
            }
            ManagerCommand::SetModel {
                local_session_id,
                model_id,
                response,
            } => {
                let connection = Arc::clone(&connection);
                let dispatcher = dispatcher.clone();
                let state = Arc::clone(&state);
                tokio::task::spawn_local(async move {
                    let result = set_model_inner(
                        &connection,
                        &dispatcher,
                        &state,
                        &local_session_id,
                        &model_id,
                    )
                    .await;
                    let _ = response.send(result);
                });
            }
        }
    }
}
