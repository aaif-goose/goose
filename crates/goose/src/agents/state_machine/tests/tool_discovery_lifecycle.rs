use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use super::calculator_extension::{value, ADD, ADD_VALUES, APP_ONLY, MULTIPLY};
use super::dummy_api::DummyApi;
use super::pipeline::MessageKind::Agent;
use super::pipeline::TestPipeline;
use super::test_pipeline;
use crate::agents::tool_discovery::{not_loaded_message, NotSent, LOAD_TOOLS_TOOL_NAME};
use crate::session::ToolDiscoveryState;

const LOADED_ADD: &str = r#""loaded":["calculator__add"]"#;
const LOADED_ADD_VALUES: &str = r#""loaded":["calculator__add_values"]"#;
const SUMMARIZE_HISTORY: &str = "Please summarize the conversation history";
const DEFERRED_CALCULATOR_TOOLS: &str = "Deferred tools: calculator__add, calculator__add_values, \
     calculator__add_with_audience, calculator__divide, calculator__request_value, calculator__subtract";

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

/// Answers `prompt` with one `load_tools` call, then with `reply`.
fn load_then_reply(api: &DummyApi, prompt: &str, names: &[&str], reply: &str) {
    api.on(prompt).reply(reply);
    api.on(prompt)
        .call(LOAD_TOOLS_TOOL_NAME, load(names))
        .times(1);
}

/// Loads `calculator__add_values`, then `calculator__add`, which sorts first.
async fn load_add_values_then_add(pipeline: &TestPipeline, api: &DummyApi) -> Result<()> {
    api.on("load values")
        .call(LOAD_TOOLS_TOOL_NAME, load(&[ADD_VALUES]));
    api.on(LOADED_ADD_VALUES).reply("values loaded");
    pipeline.run(["load values"]).await?;
    load_and_add_four(pipeline, api).await?;
    assert_eq!(
        pipeline.tool_discovery().await.loaded_tools,
        [ADD_VALUES, ADD]
    );
    Ok(())
}

fn ends_with_loaded_tools(call: &super::dummy_api::ApiCall, loaded: &[&str]) -> bool {
    call.tool_names().ends_with(loaded)
}

#[tokio::test]
async fn sessions_that_do_not_defer_tools_send_every_tool() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    api.on("hello").reply("hi");

    pipeline.run(["hello"]).await?;

    let call = &api.calls()[0];
    assert!(call.advertises_tool(ADD));
    assert!(!call.advertises_tool(LOAD_TOOLS_TOOL_NAME));
    assert!(!call.system_contains("Some tools are deferred"));
    assert!(!call.system_contains("Deferred tools"));
    Ok(())
}

#[tokio::test]
async fn a_loaded_tool_is_sent_from_the_next_inference() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;

    load_and_add_four(&pipeline, &api).await?;

    let calls = api.calls();
    assert!(calls[0].advertises_tool(LOAD_TOOLS_TOOL_NAME));
    assert!(!calls[0].advertises_tool(ADD));
    assert!(calls[0].system_contains(DEFERRED_CALCULATOR_TOOLS));
    assert!(!calls[0].system_contains(APP_ONLY));
    assert!(calls[1].advertises_tool(ADD));
    assert!(!calls[1].advertises_tool(ADD_VALUES));
    assert_eq!(calls[1].system(), calls[0].system());
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
async fn loads_do_not_change_the_system_prompt() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    load_and_add_four(&pipeline, &api).await?;
    load_then_reply(&api, "load add again", &[ADD], "already loaded");
    load_then_reply(&api, "load a typo", &["calculator__nope"], "no such tool");
    load_then_reply(&api, "load the rest", &["calculator__*"], "all loaded");

    pipeline
        .run(["load add again", "load a typo", "load the rest"])
        .await?;

    let calls = api.calls();
    let first = calls[0].system();
    assert!(first.contains("Some tools are deferred and omitted from tools[]."));
    assert!(first.contains(DEFERRED_CALCULATOR_TOOLS));
    assert!(calls.iter().all(|call| call.system() == first));
    let last = calls.last().unwrap();
    assert!(last.advertises_tool("calculator__subtract"));
    assert!(last.advertises_tool(LOAD_TOOLS_TOOL_NAME));
    Ok(())
}

#[tokio::test]
async fn a_load_appends_to_the_tools_already_sent() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;

    load_add_values_then_add(&pipeline, &api).await?;

    let calls = api.calls();
    for pair in calls.windows(2) {
        assert!(pair[1].tools().starts_with(pair[0].tools()));
    }
    assert!(!calls[0].advertises_tool(ADD_VALUES));
    assert!(ends_with_loaded_tools(&calls[1], &[ADD_VALUES]));
    assert!(ends_with_loaded_tools(
        calls.last().unwrap(),
        &[ADD_VALUES, ADD]
    ));
    Ok(())
}

#[tokio::test]
async fn a_tool_called_in_the_batch_that_loads_it_is_not_run() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    api.on("add four").unadvertised_calls([
        ("load", LOAD_TOOLS_TOOL_NAME, load(&[ADD])),
        ("early", ADD, value(4)),
    ]);
    api.on(not_loaded_message(ADD, NotSent::LoadedInSameResponse))
        .call(ADD, value(4));
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
    load_add_values_then_add(&pipeline, &api).await?;

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
    assert!(ends_with_loaded_tools(
        &after_compaction,
        &[ADD_VALUES, ADD]
    ));
    assert_eq!(pipeline.calculator_total(), 6);
    assert_eq!(
        pipeline.tool_discovery().await.loaded_tools,
        [ADD_VALUES, ADD]
    );
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
    load_add_values_then_add(&pipeline, &api).await?;

    let resumed = pipeline.reconstruct().await?;
    api.on("add one").call(ADD, value(1));
    api.on("result: 5").reply("five");
    resumed
        .run(["add one"])
        .await?
        .assert_message(-1, Agent, "five");

    assert!(ends_with_loaded_tools(
        api.calls().last().unwrap(),
        &[ADD_VALUES, ADD]
    ));
    Ok(())
}

#[tokio::test]
async fn calling_a_tool_before_loading_it_explains_how_to_load_it() -> Result<()> {
    let (pipeline, api) = deferring_pipeline().await?;
    api.on("add four").unadvertised_call(ADD, value(4));
    api.on(not_loaded_message(ADD, NotSent::NotLoaded))
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
    api.on(not_loaded_message(ADD, NotSent::NotLoaded))
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
async fn a_loaded_tool_left_out_of_a_full_request_says_so() -> Result<()> {
    use crate::agents::state_machine::ops_llm::{ADVERTISED_TOOLS_NOTE, LLM_OPERATION_NAME};
    use crate::conversation::message::{InferenceMetadata, Message};
    use rmcp::model::CallToolRequestParams;

    let (pipeline, api) = test_pipeline().await?;
    pipeline
        .set_tool_discovery(ToolDiscoveryState {
            defer_tools: true,
            loaded_tools: vec![ADD.to_string()],
        })
        .await;
    api.on("kickoff").reply("ready");
    pipeline.run(["kickoff"]).await?;

    let mut request = Message::assistant()
        .with_tool_request(
            "left-out",
            Ok(CallToolRequestParams::new(ADD)
                .with_arguments(value(4).as_object().unwrap().clone())),
        )
        .with_inference(InferenceMetadata {
            provider: "openai".to_string(),
            requested_model: "test-model".to_string(),
            resolved_model: None,
            provider_session_id: None,
        });
    request.metadata.set_operation_note(
        LLM_OPERATION_NAME,
        ADVERTISED_TOOLS_NOTE,
        json!([LOAD_TOOLS_TOOL_NAME]),
    );
    pipeline.seed([request]).await?;
    api.on(not_loaded_message(ADD, NotSent::LeftOutForLimit))
        .reply("no room");

    pipeline
        .resume()
        .await?
        .assert_message(-1, Agent, "no room");
    assert_eq!(pipeline.calculator_total(), 0);
    Ok(())
}
