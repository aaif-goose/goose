mod command_dispatch;
mod dispatcher;
mod session_ops;
mod thread;

use std::path::PathBuf;
use std::sync::Arc;

use acp_client::MessageWriter;
use agent_client_protocol::{Agent, ClientSideConnection, ExtRequest};
use serde::Deserialize;
use serde_json::value::RawValue;
use tokio::sync::{mpsc, oneshot, OnceCell};

pub use session_ops::AcpSessionInfo;

enum ManagerCommand {
    ListProviders {
        response: oneshot::Sender<Result<Vec<GooseAcpProvider>, String>>,
    },
    ListSessions {
        response: oneshot::Sender<Result<Vec<AcpSessionInfo>, String>>,
    },
    LoadSession {
        local_session_id: String,
        goose_session_id: String,
        working_dir: PathBuf,
        response: oneshot::Sender<Result<(), String>>,
    },
    PrepareSession {
        composite_key: String,
        local_session_id: String,
        provider_id: String,
        working_dir: PathBuf,
        existing_agent_session_id: Option<String>,
        response: oneshot::Sender<Result<(), String>>,
    },
    SendPrompt {
        composite_key: String,
        local_session_id: String,
        provider_id: String,
        working_dir: PathBuf,
        existing_agent_session_id: Option<String>,
        writer: Arc<dyn MessageWriter>,
        prompt: String,
        images: Vec<(String, String)>,
        response: oneshot::Sender<Result<(), String>>,
    },
    CancelSession {
        composite_key: String,
        response: oneshot::Sender<Result<bool, String>>,
    },
    ExportSession {
        session_id: String,
        response: oneshot::Sender<Result<String, String>>,
    },
    ImportSession {
        json: String,
        response: oneshot::Sender<Result<AcpSessionInfo, String>>,
    },
    ForkSession {
        session_id: String,
        response: oneshot::Sender<Result<AcpSessionInfo, String>>,
    },
    SetModel {
        local_session_id: String,
        model_id: String,
        response: oneshot::Sender<Result<(), String>>,
    },
}

pub struct GooseAcpManager {
    command_tx: mpsc::UnboundedSender<ManagerCommand>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GooseAcpProvider {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Deserialize)]
struct GooseProvidersResponse {
    providers: Vec<GooseAcpProvider>,
}

static GOOSE_ACP_MANAGER: OnceCell<Arc<GooseAcpManager>> = OnceCell::const_new();

impl GooseAcpManager {
    pub async fn start(app_handle: tauri::AppHandle) -> Result<Arc<Self>, String> {
        GOOSE_ACP_MANAGER
            .get_or_try_init(|| async move {
                let (command_tx, command_rx) = mpsc::unbounded_channel();
                let manager = Arc::new(Self { command_tx });
                let (ready_tx, ready_rx) = oneshot::channel();

                std::thread::Builder::new()
                    .name("goose-acp-manager".to_string())
                    .spawn(move || run_manager_thread(app_handle, command_rx, ready_tx))
                    .map_err(|error| {
                        format!("Failed to spawn Goose ACP manager thread: {error}")
                    })?;

                ready_rx
                    .await
                    .map_err(|_| "Goose ACP manager failed before initialization".to_string())??;

                Ok(manager)
            })
            .await
            .map(Arc::clone)
    }

    pub async fn list_sessions(&self) -> Result<Vec<AcpSessionInfo>, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::ListSessions {
                response: response_tx,
            })
            .map_err(|_| "Goose ACP manager is unavailable".to_string())?;
        response_rx
            .await
            .map_err(|_| "Goose ACP manager dropped list sessions request".to_string())?
    }

    pub async fn load_session(
        &self,
        local_session_id: String,
        goose_session_id: String,
        working_dir: PathBuf,
    ) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::LoadSession {
                local_session_id,
                goose_session_id,
                working_dir,
                response: response_tx,
            })
            .map_err(|_| "Goose ACP manager is unavailable".to_string())?;
        response_rx
            .await
            .map_err(|_| "Goose ACP manager dropped load session request".to_string())?
    }

    pub async fn prepare_session(
        &self,
        composite_key: String,
        local_session_id: String,
        provider_id: String,
        working_dir: PathBuf,
        existing_agent_session_id: Option<String>,
    ) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::PrepareSession {
                composite_key,
                local_session_id,
                provider_id,
                working_dir,
                existing_agent_session_id,
                response: response_tx,
            })
            .map_err(|_| "Goose ACP manager is unavailable".to_string())?;
        response_rx
            .await
            .map_err(|_| "Goose ACP manager dropped prepare request".to_string())?
    }

    pub async fn list_providers(&self) -> Result<Vec<GooseAcpProvider>, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::ListProviders {
                response: response_tx,
            })
            .map_err(|_| "Goose ACP manager is unavailable".to_string())?;
        response_rx
            .await
            .map_err(|_| "Goose ACP manager dropped provider discovery request".to_string())?
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_prompt(
        &self,
        composite_key: String,
        local_session_id: String,
        provider_id: String,
        working_dir: PathBuf,
        existing_agent_session_id: Option<String>,
        writer: Arc<dyn MessageWriter>,
        prompt: String,
        images: Vec<(String, String)>,
    ) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::SendPrompt {
                composite_key,
                local_session_id,
                provider_id,
                working_dir,
                existing_agent_session_id,
                writer,
                prompt,
                images,
                response: response_tx,
            })
            .map_err(|_| "Goose ACP manager is unavailable".to_string())?;
        response_rx
            .await
            .map_err(|_| "Goose ACP manager dropped prompt request".to_string())?
    }

    pub async fn cancel_session(&self, composite_key: String) -> Result<bool, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::CancelSession {
                composite_key,
                response: response_tx,
            })
            .map_err(|_| "Goose ACP manager is unavailable".to_string())?;
        response_rx
            .await
            .map_err(|_| "Goose ACP manager dropped cancel request".to_string())?
    }

    pub async fn set_model(
        &self,
        local_session_id: String,
        model_id: String,
    ) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::SetModel {
                local_session_id,
                model_id,
                response: response_tx,
            })
            .map_err(|_| "Goose ACP manager is unavailable".to_string())?;
        response_rx
            .await
            .map_err(|_| "Goose ACP manager dropped set model request".to_string())?
    }

    /// Export a session as JSON via the goose binary.
    pub async fn export_session(&self, session_id: String) -> Result<String, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::ExportSession {
                session_id,
                response: response_tx,
            })
            .map_err(|_| "Goose ACP manager is unavailable".to_string())?;
        response_rx
            .await
            .map_err(|_| "Goose ACP manager dropped export session request".to_string())?
    }

    /// Import a session from JSON via the goose binary.
    pub async fn import_session(&self, json: String) -> Result<AcpSessionInfo, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::ImportSession {
                json,
                response: response_tx,
            })
            .map_err(|_| "Goose ACP manager is unavailable".to_string())?;
        response_rx
            .await
            .map_err(|_| "Goose ACP manager dropped import session request".to_string())?
    }

    /// Fork (duplicate) a session via the goose binary.
    pub async fn fork_session(&self, session_id: String) -> Result<AcpSessionInfo, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(ManagerCommand::ForkSession {
                session_id,
                response: response_tx,
            })
            .map_err(|_| "Goose ACP manager is unavailable".to_string())?;
        response_rx
            .await
            .map_err(|_| "Goose ACP manager dropped fork session request".to_string())?
    }
}

/// Call a goose ext_method and return the raw JSON response string.
async fn call_ext_method(
    connection: &Arc<ClientSideConnection>,
    method: &str,
    params_json: serde_json::Value,
) -> Result<String, String> {
    let params = RawValue::from_string(params_json.to_string())
        .map_err(|e| format!("Failed to build {method} request: {e}"))?;
    let resp = connection
        .ext_method(ExtRequest::new(method, params.into()))
        .await
        .map_err(|e| format!("{method} failed via Goose ACP: {e:?}"))?;
    Ok(resp.0.get().to_string())
}

fn run_manager_thread(
    app_handle: tauri::AppHandle,
    command_rx: mpsc::UnboundedReceiver<ManagerCommand>,
    ready_tx: oneshot::Sender<Result<(), String>>,
) {
    thread::run_manager_thread(app_handle, command_rx, ready_tx);
}
