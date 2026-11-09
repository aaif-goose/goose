use anyhow::Result;
use serde_json::json;

use super::calculator_extension::{value, ADD, APP_ONLY};
use super::pipeline::{test_pipeline, MessageKind::Agent};
use crate::agents::platform_extensions::MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE;
use crate::agents::tool_execution::DECLINED_RESPONSE;
use crate::config::{permission::PermissionLevel, GooseMode};
use crate::conversation::Conversation;

const ENABLE: &str = "enable_tools";
const ENABLED: &str = r#""enabled":["calculator__add"]"#;

#[tokio::test]
async fn eager_publishing_is_the_default() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    api.on("hello").reply("ready");
    pipeline.run(["hello"]).await?;
    assert!(api.calls()[0].advertises_tool(ADD));
    assert!(!api.calls()[0].advertises_tool(ENABLE));
    Ok(())
}

#[tokio::test]
async fn enabling_follows_conversation_through_compaction_resume_and_clear() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline.with_deferred_tools();
    api.on("enable addition")
        .call(ENABLE, json!({"tool_names": [ADD]}));
    api.on(ENABLED).reply("enabled addition");
    pipeline.run(["enable addition"]).await?;
    assert_eq!(pipeline.calculator_total(), 0);
    let calls = api.calls();
    assert!(!calls[0].advertises_tool(ADD));
    assert!(calls[0].advertises_tool(MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE));
    assert!(calls[0].system_contains(ADD));
    assert!(!calls[0].system_contains(APP_ONLY));
    assert!(calls[1].advertises_tool(ADD));

    let pipeline = pipeline.reconstruct().await?;
    api.on("Please summarize the conversation history")
        .reply("summary");
    pipeline
        .set_total_tokens((pipeline.context_limit() as f64 * 0.81) as i32)
        .await;
    api.on("add four").call(ADD, value(4));
    api.on("result: 4").reply("four");
    let compacted = pipeline.run(["add four"]).await?;
    compacted.assert_message(-1, Agent, "four");
    assert_eq!(compacted.history_replacements(), 1);
    assert!(api.calls().last().unwrap().advertises_tool(ADD));
    assert!(!api.calls().last().unwrap().input_contains(ENABLED));

    pipeline.run(["/clear"]).await?;
    api.on("fresh").reply("ready").times(1);
    pipeline.run(["fresh"]).await?;
    assert!(!api.calls().last().unwrap().advertises_tool(ADD));
    assert!(api.calls().last().unwrap().advertises_tool(ENABLE));

    pipeline.run(["enable addition"]).await?;
    assert!(api.calls().last().unwrap().advertises_tool(ADD));
    pipeline
        .session_manager
        .replace_conversation(&pipeline.session_id, &Conversation::default())
        .await?;
    api.on("fresh").reply("ready");
    pipeline.run(["fresh"]).await?;
    assert!(!api.calls().last().unwrap().advertises_tool(ADD));
    Ok(())
}

#[tokio::test]
async fn unknown_and_app_only_names_do_not_enable_any_tools() -> Result<()> {
    for name in ["calculator__unknown", APP_ONLY] {
        let (pipeline, api) = test_pipeline().await?;
        let pipeline = pipeline.with_deferred_tools();
        api.on("enable")
            .call(ENABLE, json!({"tool_names": [ADD, name]}));
        api.on("Unknown MCP tool").reply("unavailable");
        pipeline
            .run(["enable"])
            .await?
            .assert_message(-1, Agent, "unavailable");
        assert!(!api.calls().last().unwrap().advertises_tool(ADD));
    }
    Ok(())
}

#[tokio::test]
async fn deferred_tools_cannot_execute_before_enabling() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline.with_deferred_tools();
    api.on("add four").unadvertised_call(ADD, value(4));
    api.on("not available").reply("enable it first");
    pipeline
        .run(["add four"])
        .await?
        .assert_message(-1, Agent, "enable it first");
    assert_eq!(pipeline.calculator_total(), 0);
    assert!(!api.calls().last().unwrap().advertises_tool(ADD));
    Ok(())
}

#[tokio::test]
async fn enabling_and_execution_keep_their_own_permissions() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline
        .with_deferred_tools()
        .with_goose_mode(GooseMode::Approve)
        .await;
    pipeline.set_permission(ENABLE, PermissionLevel::NeverAllow);
    api.on("enable addition")
        .call(ENABLE, json!({"tool_names": [ADD]}));
    api.on(DECLINED_RESPONSE).reply("denied");
    pipeline
        .run(["enable addition"])
        .await?
        .assert_message(-1, Agent, "denied");
    assert!(!api.calls().last().unwrap().advertises_tool(ADD));

    pipeline.run(["/clear"]).await?;
    pipeline.set_permission(ENABLE, PermissionLevel::AlwaysAllow);
    pipeline.set_permission(ADD, PermissionLevel::NeverAllow);
    api.on(ENABLED).call(ADD, value(4)).times(1);
    pipeline
        .run(["enable addition"])
        .await?
        .assert_message(-1, Agent, "denied");
    assert!(api.calls().last().unwrap().advertises_tool(ADD));
    assert_eq!(pipeline.calculator_total(), 0);
    Ok(())
}

#[cfg(feature = "code-mode")]
#[tokio::test]
async fn code_mode_keeps_its_own_catalog() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline.with_deferred_tools();
    pipeline.add_extension("code_execution").await?;
    api.on("hello").reply("ready");
    pipeline.run(["hello"]).await?;
    assert!(!api.calls()[0].advertises_tool(ENABLE));
    assert!(!api.calls()[0].system_contains("MCP tools available to enable"));
    Ok(())
}
