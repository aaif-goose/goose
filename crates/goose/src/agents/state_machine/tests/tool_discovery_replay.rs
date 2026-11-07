//! Tool budget paths through the real pipeline: extensions changing between
//! loads, and several load_tools calls in one response.

use std::sync::Arc;

use anyhow::Result;
use rmcp::model::CallToolRequestParams;
use serde_json::json;

use super::calculator_extension::{CalculatorExtension, ADD, DIVIDE};
use super::dummy_api::DummyApi;
use super::pipeline::TestPipeline;
use super::test_pipeline;
use crate::agents::extension::ExtensionConfig;
use crate::agents::mcp_client::McpClientTrait;
use crate::agents::state_machine::ops_llm::{ADVERTISED_TOOLS_NOTE, LLM_OPERATION_NAME};
use crate::agents::tool_discovery::{LOAD_TOOLS_TOOL_NAME, MAX_ADVERTISED_TOOLS};
use crate::conversation::message::Message;
use crate::session::ToolDiscoveryState;

async fn deferring_pipeline() -> Result<(TestPipeline, Arc<DummyApi>)> {
    let (pipeline, api) = test_pipeline().await?;
    pipeline
        .set_tool_discovery(ToolDiscoveryState::deferring())
        .await;
    Ok((pipeline, api))
}

/// Enables another copy of the calculator under `name`, so its tools are
/// `<name>__add` and so on.
async fn attach(pipeline: &TestPipeline, name: &str) {
    let client = Arc::new(CalculatorExtension::new(
        pipeline.session_manager.action_required(),
    ));
    let info = client.get_info().cloned();
    let config = ExtensionConfig::Builtin {
        name: name.to_string(),
        description: name.to_string(),
        display_name: None,
        timeout: None,
        bundled: None,
        available_tools: vec![],
    };
    pipeline
        .services()
        .await
        .extension_manager
        .add_test_client(&pipeline.session_id, config, client, info)
        .await;
}

async fn load_turn(
    pipeline: &TestPipeline,
    api: &DummyApi,
    prompt: &str,
    names: &[&str],
) -> Result<()> {
    api.on(prompt).reply(format!("{prompt} done"));
    api.on(prompt)
        .call(LOAD_TOOLS_TOOL_NAME, json!({ "tool_names": names }))
        .times(1);
    pipeline.run([prompt]).await?;
    Ok(())
}

fn load_call(name: &str) -> CallToolRequestParams {
    CallToolRequestParams::new(LOAD_TOOLS_TOOL_NAME)
        .with_arguments(json!({ "tool_names": [name] }).as_object().unwrap().clone())
}

/// Known gap: a tool loaded before its extension was turned off comes back
/// loaded when the extension returns, and no budget check runs then. The
/// budget accepted the last load when the next request would carry 13 tools;
/// the request after the extension returns carries 15. At
/// MAX_ADVERTISED_TOOLS = 13 that request is past the limit.
#[tokio::test]
async fn replay_an_extension_returning_with_loads_skips_the_budget() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    attach(&pipeline, "abacus").await;

    load_turn(&pipeline, &api, "step-alpha", &[ADD]).await?;
    pipeline.remove_extension("calculator").await?;
    load_turn(&pipeline, &api, "step-bravo", &["abacus__*"]).await?;
    attach(&pipeline, "calculator").await;
    api.on("step-charlie").reply("step-charlie done");
    pipeline.run(["step-charlie"]).await?;

    let counts: Vec<usize> = api
        .calls()
        .iter()
        .map(|call| call.tool_names().len())
        .collect();
    assert_eq!(counts, [8, 9, 7, 13, 15]);
    let loaded = pipeline.tool_discovery().await.loaded_tools;
    assert_eq!(loaded.first().map(String::as_str), Some(ADD));
    assert_eq!(loaded.len(), 7);
    Ok(())
}

/// Two load_tools calls in one response from a request that carried
/// MAX_ADVERTISED_TOOLS - 1 tools. Only the first fits.
#[tokio::test]
async fn replay_two_loads_in_one_response_share_the_budget() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    api.on("kickoff").reply("ready");
    pipeline.run(["kickoff"]).await?;

    let advertised: Vec<String> = (1..MAX_ADVERTISED_TOOLS)
        .map(|n| format!("tool_{n:03}"))
        .collect();
    let mut request = Message::assistant()
        .with_tool_request("first", Ok(load_call(ADD)))
        .with_tool_request("second", Ok(load_call(DIVIDE)));
    request.metadata.set_operation_note(
        LLM_OPERATION_NAME,
        ADVERTISED_TOOLS_NOTE,
        json!(advertised),
    );
    pipeline.seed([request]).await?;
    api.on("tool_limit").reply("done");

    pipeline.resume().await?;

    assert_eq!(pipeline.tool_discovery().await.loaded_tools, [ADD]);
    assert!(api.calls()[1].input_contains(r#""reason":"tool_limit""#));
    Ok(())
}
