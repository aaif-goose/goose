use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol::{
    Agent, ClientSideConnection, Implementation, InitializeRequest, ProtocolVersion,
};
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::services::acp::goose_serve::{GooseServeProcess, WS_BRIDGE_BUFFER_BYTES};

use super::command_dispatch;
use super::dispatcher::{SessionEventDispatcher, SessionRoute};
use super::ManagerCommand;

pub(super) fn run_manager_thread(
    app_handle: tauri::AppHandle,
    command_rx: mpsc::UnboundedReceiver<ManagerCommand>,
    ready_tx: oneshot::Sender<Result<(), String>>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready_tx.send(Err(format!(
                "Failed to build Goose ACP manager runtime: {error}"
            )));
            return;
        }
    };

    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, async move {
        let setup_result = async {
            GooseServeProcess::start().await?;
            let process = GooseServeProcess::get()?;
            let ws_stream = connect_async(process.ws_url())
                .await
                .map(|(stream, _)| stream)
                .map_err(|error| format!("Failed to connect to goose serve: {error}"))?;

            let (mut ws_writer, mut ws_reader) = ws_stream.split();
            let (connection_outgoing, bridge_outgoing_reader) =
                tokio::io::duplex(WS_BRIDGE_BUFFER_BYTES);
            let (bridge_incoming_writer, connection_incoming) =
                tokio::io::duplex(WS_BRIDGE_BUFFER_BYTES);

            tokio::task::spawn_local(async move {
                let mut reader = BufReader::new(bridge_outgoing_reader);
                let mut line = String::new();

                loop {
                    line.clear();
                    let bytes_read = match reader.read_line(&mut line).await {
                        Ok(bytes_read) => bytes_read,
                        Err(error) => {
                            log::error!("Failed to read ACP output for Goose WebSocket: {error}");
                            break;
                        }
                    };
                    if bytes_read == 0 {
                        break;
                    }

                    let payload = line.trim_end_matches(['\r', '\n']);
                    if payload.is_empty() {
                        continue;
                    }

                    if let Err(error) = ws_writer.send(Message::Text(payload.to_string())).await {
                        log::error!("Failed to write Goose WebSocket frame: {error}");
                        break;
                    }
                }

                let _ = ws_writer.close().await;
            });

            tokio::task::spawn_local(async move {
                let mut writer = bridge_incoming_writer;

                while let Some(message) = ws_reader.next().await {
                    match message {
                        Ok(Message::Text(text)) => {
                            if let Err(error) = writer.write_all(text.as_bytes()).await {
                                log::error!(
                                    "Failed to forward Goose WebSocket payload to ACP client: {error}"
                                );
                                break;
                            }
                            if let Err(error) = writer.write_all(b"\n").await {
                                log::error!(
                                    "Failed to delimit Goose WebSocket payload for ACP client: {error}"
                                );
                                break;
                            }
                            if let Err(error) = writer.flush().await {
                                log::error!("Failed to flush Goose WebSocket payload: {error}");
                                break;
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Ok(Message::Binary(_))
                        | Ok(Message::Ping(_))
                        | Ok(Message::Pong(_))
                        | Ok(Message::Frame(_)) => {}
                        Err(error) => {
                            log::error!("Goose WebSocket read failed: {error}");
                            break;
                        }
                    }
                }

                let _ = writer.shutdown().await;
            });

            let routes = Arc::new(Mutex::new(HashMap::<String, SessionRoute>::new()));
            let dispatcher = Arc::new(SessionEventDispatcher::new(
                app_handle.clone(),
                Arc::clone(&routes),
            ));
            let (connection, io_future) = ClientSideConnection::new(
                dispatcher.clone(),
                connection_outgoing.compat_write(),
                connection_incoming.compat(),
                |future| {
                    tokio::task::spawn_local(future);
                },
            );
            let connection = Arc::new(connection);

            tokio::task::spawn_local(async move {
                if let Err(error) = io_future.await {
                    log::error!("Goose ACP IO error: {error:?}");
                }
            });

            connection
                .initialize(
                    InitializeRequest::new(ProtocolVersion::LATEST)
                        .client_info(Implementation::new("goose2", env!("CARGO_PKG_VERSION"))),
                )
                .await
                .map_err(|error| format!("Goose ACP initialize failed: {error:?}"))?;

            Ok::<_, String>((connection, dispatcher))
        }
        .await;

        let (connection, dispatcher) = match setup_result {
            Ok(parts) => {
                let _ = ready_tx.send(Ok(()));
                parts
            }
            Err(error) => {
                let _ = ready_tx.send(Err(error));
                return;
            }
        };

        command_dispatch::dispatch_commands(command_rx, connection, dispatcher).await;
    });
}
