use crate::acp::tool_call_notifier::ToolCallNotifier;
use crate::agents::Agent;
use crate::conversation::message::{ToolChainSummary, ToolRequest};
use crate::session::SessionManager;
use crate::tool_call_labels::{generate_tool_chain_summary, generate_tool_title};
use agent_client_protocol::schema::v1::{
    Meta, SessionId, ToolCallId, ToolCallUpdate, ToolCallUpdateFields,
};
use serde_json::{json, Map, Value};
use std::sync::Arc;
use tokio::spawn;

use super::chain::ReadyToolChain;

const TOOL_CHAIN_SUMMARY_META_KEY: &str = "toolChainSummary";

pub(crate) fn tool_chain_summary(chain_summary: &ToolChainSummary) -> (String, Value) {
    (
        TOOL_CHAIN_SUMMARY_META_KEY.to_string(),
        json!(chain_summary),
    )
}

fn build_chain_summary_update(
    tool_call_id: String,
    chain_summary: &ToolChainSummary,
) -> ToolCallUpdate {
    let goose_meta = Map::from_iter([tool_chain_summary(chain_summary)]);
    let mut meta = Meta::default();
    meta.insert("goose".to_string(), Value::Object(goose_meta));
    ToolCallUpdate::new(ToolCallId::new(tool_call_id), ToolCallUpdateFields::new()).meta(Some(meta))
}

pub(crate) fn spawn_tool_title_enrichment(
    agent: &Arc<Agent>,
    tool_call_notifier: ToolCallNotifier,
    session_manager: &Arc<SessionManager>,
    session_id: &str,
    tool_request: &ToolRequest,
) {
    let agent = agent.clone();
    let session_manager = session_manager.clone();
    let session_id = session_id.to_string();
    let tool_request = tool_request.clone();

    spawn(async move {
        if let Some(title) = generate_tool_title(
            agent.as_ref(),
            session_manager.as_ref(),
            &session_id,
            &tool_request,
        )
        .await
        {
            let _ = tool_call_notifier.send_update(ToolCallUpdate::new(
                ToolCallId::new(tool_request.id),
                ToolCallUpdateFields::new().title(title),
            ));
        }
    });
}

pub(crate) fn spawn_chain_summary_enrichment(
    agent: &Arc<Agent>,
    session_id: &SessionId,
    tool_call_notifier: ToolCallNotifier,
    session_manager: &Arc<SessionManager>,
    chain: ReadyToolChain,
) {
    let agent = agent.clone();
    let session_id = session_id.clone();
    let session_manager = session_manager.clone();

    spawn(async move {
        let ReadyToolChain { tool_requests } = chain;
        let first_tool_call_id = tool_requests[0].id.clone();

        let Some(summary) = generate_tool_chain_summary(
            agent.as_ref(),
            session_manager.as_ref(),
            &session_id.0,
            &tool_requests,
        )
        .await
        else {
            return;
        };

        let update = build_chain_summary_update(first_tool_call_id, &summary);
        let _ = tool_call_notifier.send_update(update);
    });
}

#[cfg(test)]
mod tests {
    mod build_chain_summary_update {
        use super::super::build_chain_summary_update;
        use crate::conversation::message::ToolChainSummary;
        use serde_json::json;

        #[test]
        fn contains_only_the_chain_summary_delta() {
            let update = build_chain_summary_update(
                "req_1".to_string(),
                &ToolChainSummary {
                    summary: "applied dark mode".to_string(),
                    count: 4,
                },
            );

            assert_eq!(
                serde_json::to_value(update).expect("update should serialize"),
                json!({
                    "toolCallId": "req_1",
                    "_meta": {
                        "goose": {
                            "toolChainSummary": {
                                "summary": "applied dark mode",
                                "count": 4,
                            },
                        },
                    },
                }),
            );
        }
    }
}
