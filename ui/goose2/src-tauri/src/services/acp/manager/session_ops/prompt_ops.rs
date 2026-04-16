use std::path::PathBuf;
use std::sync::Arc;

use acp_client::MessageWriter;
use agent_client_protocol::{
    Agent, ClientSideConnection, ContentBlock as AcpContentBlock, ImageContent, PromptRequest,
    TextContent,
};
use tokio::sync::Mutex;

use super::super::dispatcher::SessionEventDispatcher;
use super::{prepare_session_inner, ManagerState, PrepareSessionInput};

async fn take_cancel_requested(state: &Arc<Mutex<ManagerState>>, composite_key: &str) -> bool {
    let mut guard = state.lock().await;
    guard.take_cancel_requested(composite_key)
}

async fn clear_cancel_requested(state: &Arc<Mutex<ManagerState>>, composite_key: &str) {
    let mut guard = state.lock().await;
    guard.pending_cancels.remove(composite_key);
}

pub(super) fn build_content_blocks(
    prompt: String,
    images: Vec<(String, String)>,
) -> Vec<AcpContentBlock> {
    let mut content_blocks = Vec::with_capacity(images.len() + 1);
    for (data, mime_type) in images {
        content_blocks.push(AcpContentBlock::Image(ImageContent::new(
            data.as_str(),
            mime_type.as_str(),
        )));
    }
    content_blocks.push(AcpContentBlock::Text(TextContent::new(prompt)));
    content_blocks
}

#[allow(clippy::too_many_arguments)]
pub(in super::super) async fn send_prompt_inner(
    connection: &Arc<ClientSideConnection>,
    dispatcher: &Arc<SessionEventDispatcher>,
    state: &Arc<Mutex<ManagerState>>,
    composite_key: String,
    local_session_id: String,
    provider_id: String,
    working_dir: PathBuf,
    existing_agent_session_id: Option<String>,
    writer: Arc<dyn MessageWriter>,
    prompt: String,
    images: Vec<(String, String)>,
) -> Result<(), String> {
    let provider_id_for_writer = provider_id.clone();
    let goose_session_id = match prepare_session_inner(
        connection,
        dispatcher,
        state,
        PrepareSessionInput {
            composite_key: composite_key.clone(),
            local_session_id: local_session_id.clone(),
            provider_id,
            working_dir,
            existing_agent_session_id,
        },
    )
    .await
    {
        Ok(goose_session_id) => goose_session_id,
        Err(error) => {
            clear_cancel_requested(state, &composite_key).await;
            return Err(error);
        }
    };

    if take_cancel_requested(state, &composite_key).await {
        return Ok(());
    }

    dispatcher
        .attach_writer(
            &goose_session_id,
            &local_session_id,
            Some(&provider_id_for_writer),
            writer.clone(),
        )
        .await;

    if dispatcher.is_canceled(&goose_session_id).await
        || take_cancel_requested(state, &composite_key).await
    {
        dispatcher.clear_writer(&goose_session_id).await;
        return Ok(());
    }

    let content_blocks = build_content_blocks(prompt, images);

    let result = connection
        .prompt(PromptRequest::new(goose_session_id.clone(), content_blocks))
        .await
        .map(|_| ())
        .map_err(|error| format!("Prompt failed via Goose ACP: {error:?}"));

    let canceled = dispatcher.is_canceled(&goose_session_id).await;
    dispatcher.clear_writer(&goose_session_id).await;
    clear_cancel_requested(state, &composite_key).await;

    if result.is_ok() && !canceled {
        writer.finalize().await;
    }

    result
}
