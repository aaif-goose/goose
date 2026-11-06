use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use super::calculator_extension::{value, ADD, ADD_VALUES, APP_ONLY, MULTIPLY};
use super::dummy_api::DummyApi;
use super::pipeline::MessageKind::Agent;
use super::pipeline::TestPipeline;
use super::test_pipeline;
use crate::agents::tool_discovery::{not_loaded_message, LOAD_TOOLS_TOOL_NAME};
use crate::session::ToolDiscoveryState;

const LOADED_ADD: &str = r#""loaded":["calculator__add"]"#;
const SUMMARIZE_HISTORY: &str = "Please summarize the conversation history";

async fn deferring_pipeline() -> Result<(TestPipeline, Arc<DummyApi>)> {
    let (pipeline, api) = test_pipeline().await?;
    pipeline
        .set_tool_discovery(ToolDiscoveryState::deferring())
        .await;
    Ok((pipeline, api))
}

fn load(names: &[&str]) -> serde_json::Value {
    json!({ "tool_names": names })
}

async fn load_and_add_four(pipeline: &TestPipeline, api: &DummyApi) -> Result<()> {
    api.on("add four").call(LOAD_TOOLS_TOOL_NAME, load(&[ADD]));
    api.on(LOADED_ADD).call(ADD, value(4));
    api.on("result: 4").reply("added four");
    pipeline
        .run(["add four"])
        .await?
        .assert_message(-1, Agent, "added four");
    Ok(())
}

#[tokio::test]
async fn sessions_that_do_not_defer_tools_send_every_tool() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    api.on("hello").reply("hi");

    pipeline.run(["hello"]).await?;

    let call = &api.calls()[0];
    assert!(call.advertises_tool(ADD));
    assert!(!call.advertises_tool(LOAD_TOOLS_TOOL_NAME));
    assert!(!call.system_contains("Not loaded"));
    Ok(())
}

#[tokio::test]
async fn a_loaded_tool_is_sent_from_the_next_inference() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;

    load_and_add_four(&pipeline, &api).await?;

    let calls = api.calls();
    assert!(calls[0].advertises_tool(LOAD_TOOLS_TOOL_NAME));
    assert!(!calls[0].advertises_tool(ADD));
    assert!(calls[0].system_contains("Not loaded: calculator__add, calculator__add_values"));
    assert!(!calls[0].system_contains(APP_ONLY));
    assert!(calls[1].advertises_tool(ADD));
    assert!(!calls[1].advertises_tool(ADD_VALUES));
    assert!(!calls[1].system_contains("Not loaded: calculator__add,"));
    assert_eq!(pipeline.calculator_total(), 4);
    assert_eq!(pipeline.tool_discovery().await.loaded_tools, [ADD]);
    Ok(())
}

#[tokio::test]
async fn always_load_tools_are_sent_up_front() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    api.on("hello").reply("hi");

    pipeline.run(["hello"]).await?;

    let call = &api.calls()[0];
    assert!(call.advertises_tool(MULTIPLY));
    assert!(!call.system_contains(MULTIPLY));
    Ok(())
}

#[tokio::test]
async fn a_tool_called_in_the_batch_that_loads_it_is_not_run() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    api.on("add four").unadvertised_calls([
        ("load", LOAD_TOOLS_TOOL_NAME, load(&[ADD])),
        ("early", ADD, value(4)),
    ]);
    api.on(not_loaded_message(ADD, true)).call(ADD, value(4));
    api.on("result: 4").reply("added four");

    pipeline
        .run(["add four"])
        .await?
        .assert_message(-1, Agent, "added four");

    assert!(api.calls()[1].input_contains(LOADED_ADD));
    assert_eq!(pipeline.calculator_total(), 4);
    Ok(())
}

#[tokio::test]
async fn unknown_names_are_rejected_without_loading_anything() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    api.on("load a typo")
        .call(LOAD_TOOLS_TOOL_NAME, load(&["calculator__nope"]));
    api.on(r#""reason":"unknown_tool""#).reply("no such tool");

    pipeline
        .run(["load a typo"])
        .await?
        .assert_message(-1, Agent, "no such tool");

    assert!(pipeline.tool_discovery().await.loaded_tools.is_empty());
    Ok(())
}

#[tokio::test]
async fn loaded_tools_survive_auto_compaction() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    load_and_add_four(&pipeline, &api).await?;

    pipeline
        .set_total_tokens((pipeline.context_limit() as f64 * 0.81) as i32)
        .await;
    api.on("add two more").call(ADD, value(2));
    api.on("result: 6").reply("six");
    api.on(SUMMARIZE_HISTORY).reply("summary");

    let compacted = pipeline.run(["add two more"]).await?;

    compacted.assert_message(-1, Agent, "six");
    assert_eq!(compacted.history_replacements(), 1);
    let after_compaction = api
        .calls()
        .into_iter()
        .find(|call| call.input_contains("add two more") && !call.input_contains(SUMMARIZE_HISTORY))
        .expect("an inference after compaction");
    assert!(!after_compaction.input_contains(LOADED_ADD));
    assert!(after_compaction.advertises_tool(ADD));
    assert_eq!(pipeline.calculator_total(), 6);
    assert_eq!(pipeline.tool_discovery().await.loaded_tools, [ADD]);
    Ok(())
}

#[tokio::test]
async fn clear_unloads_tools_but_keeps_deferring() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    load_and_add_four(&pipeline, &api).await?;

    pipeline.run(["/clear"]).await?;

    assert_eq!(
        pipeline.tool_discovery().await,
        ToolDiscoveryState::deferring()
    );
    api.on("hello again").reply("hi");
    pipeline.run(["hello again"]).await?;
    let call = api.calls().pop().unwrap();
    assert!(!call.advertises_tool(ADD));
    assert!(call.advertises_tool(LOAD_TOOLS_TOOL_NAME));
    Ok(())
}

#[tokio::test]
async fn loaded_tools_survive_reconstruction() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    load_and_add_four(&pipeline, &api).await?;

    let resumed = pipeline.reconstruct().await?;
    api.on("add one").call(ADD, value(1));
    api.on("result: 5").reply("five");
    resumed
        .run(["add one"])
        .await?
        .assert_message(-1, Agent, "five");

    assert!(api.calls().last().unwrap().advertises_tool(ADD));
    Ok(())
}

#[tokio::test]
async fn calling_a_tool_before_loading_it_explains_how_to_load_it() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    api.on("add four").unadvertised_call(ADD, value(4));
    api.on(not_loaded_message(ADD, false))
        .call(LOAD_TOOLS_TOOL_NAME, load(&[ADD]));
    api.on(LOADED_ADD).call(ADD, value(4));
    api.on("result: 4").reply("added four");

    pipeline
        .run(["add four"])
        .await?
        .assert_message(-1, Agent, "added four");

    assert_eq!(pipeline.calculator_total(), 4);
    Ok(())
}

#[tokio::test]
async fn a_mangled_call_to_a_deferred_tool_explains_how_to_load_it() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    api.on("add four")
        .unadvertised_call("calculator.add", value(4));
    api.on(not_loaded_message(ADD, false))
        .call(LOAD_TOOLS_TOOL_NAME, load(&[ADD]));
    api.on(LOADED_ADD).call(ADD, value(4));
    api.on("result: 4").reply("added four");

    pipeline
        .run(["add four"])
        .await?
        .assert_message(-1, Agent, "added four");

    assert_eq!(pipeline.calculator_total(), 4);
    Ok(())
}
