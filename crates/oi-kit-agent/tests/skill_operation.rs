#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_acp_types::custom_requests::{SourceEntry, SourceType};
use oi_kit_agent::operation::{ConversationEffect, Emitter, Operation, OperationResult};
use oi_kit_agent::skills::{SkillOperation, SkillSource, LOAD_SKILL_TOOL_NAME};
use oi_kit_agent::slash_command::SlashCommandOperation;
use oi_kit_agent::tool_call_state::EXECUTABLE_KEY;
use oi_kit_provider_types::conversation::message::{Message, MessageContent};
use oi_kit_provider_types::conversation::Conversation;
use rmcp::model::CallToolRequestParams;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct Skills;

#[async_trait]
impl SkillSource<()> for Skills {
    async fn skills(&self, _session: &()) -> Result<Vec<SourceEntry>> {
        Ok(vec![SourceEntry {
            source_type: SourceType::Skill,
            name: "review".to_string(),
            description: "Review a component".to_string(),
            content: "Review $component against $standard.".to_string(),
            path: "/skills/review".to_string(),
            global: false,
            writable: false,
            supporting_files: vec!["/skills/review/references/guide.md".to_string()],
            properties: HashMap::from([(
                "arguments".to_string(),
                json!(["component", "standard"]),
            )]),
        }])
    }

    async fn read_supporting_file(
        &self,
        skill: &SourceEntry,
        relative_path: &Path,
    ) -> Result<String> {
        match relative_path.to_str() {
            Some("references/guide.md") => Ok(format!("{} guide body", skill.name)),
            _ => Err(anyhow!("unexpected read of {}", relative_path.display())),
        }
    }
}

fn operation() -> SkillOperation<'static, ()> {
    SkillOperation::new(Arc::new(Skills))
}

fn emitter() -> Emitter {
    let (tx, _rx) = mpsc::unbounded_channel();
    Emitter::new(tx, CancellationToken::new())
}

fn load_skill_call(id: &str, arguments: serde_json::Value, executable: bool) -> Message {
    let serde_json::Value::Object(arguments) = arguments else {
        panic!("arguments must be an object")
    };
    Message::assistant().with_tool_request_with_metadata(
        id,
        Ok(CallToolRequestParams::new(LOAD_SKILL_TOOL_NAME).with_arguments(arguments)),
        None,
        Some(json!({ EXECUTABLE_KEY: executable })),
    )
}

fn appended(effects: &[ConversationEffect]) -> Vec<&Message> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            ConversationEffect::AppendMessage(message) => Some(message),
            _ => None,
        })
        .collect()
}

fn tool_results(message: &Message) -> Vec<(String, bool, String)> {
    message
        .content
        .iter()
        .filter_map(|content| match content {
            MessageContent::ToolResponse(response) => {
                let result = response.tool_result.as_ref().unwrap();
                Some((
                    response.id.clone(),
                    result.is_error == Some(true),
                    result.content[0].as_text().unwrap().text.clone(),
                ))
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn answers_approved_load_skill_calls_and_leaves_the_rest() -> Result<()> {
    let conversation = Conversation::new_unvalidated(vec![
        Message::user().with_text("help me review"),
        load_skill_call(
            "skill",
            json!({ "name": "review", "args": "Button \"design system\"" }),
            true,
        ),
        load_skill_call(
            "file",
            json!({ "name": "review/references/guide.md" }),
            true,
        ),
        load_skill_call("missing", json!({ "name": "reviews" }), true),
        load_skill_call("waiting", json!({ "name": "review" }), false),
    ]);

    let OperationResult::Applied(step) =
        Operation::<(), ConversationEffect>::run(&operation(), &(), &conversation, &emitter())
            .await?
    else {
        panic!("approved load_skill calls should be answered");
    };
    let results = tool_results(appended(&step.effects)[0]);

    let ids: Vec<_> = results.iter().map(|(id, _, _)| id.as_str()).collect();
    assert_eq!(ids, ["skill", "file", "missing"]);
    let (_, skill_error, skill_text) = &results[0];
    assert!(!skill_error);
    assert!(skill_text.contains("Review Button against design system."));
    assert!(skill_text.contains("load_skill(name: \"review/references/guide.md\")"));
    assert_eq!(
        results[1],
        ("file".into(), false, "review guide body".into())
    );
    assert_eq!(
        results[2],
        (
            "missing".into(),
            true,
            "Skill 'reviews' not found. Did you mean: review?".into()
        )
    );
    Ok(())
}

#[tokio::test]
async fn lists_skills_in_the_prompt_and_runs_them_as_commands() -> Result<()> {
    let skills = Arc::new(operation());
    let prompt = Operation::<(), ConversationEffect>::prompt_parts(
        skills.as_ref(),
        &(),
        &Conversation::empty(),
    )
    .await?;
    assert_eq!(prompt[0].0, "skills");
    assert!(prompt[0].1.contains("- review: Review a component"));

    let commands = SlashCommandOperation::<(), ConversationEffect>::new(vec![skills]);
    let kickoff = Message::user()
        .with_text("/Review Card a11y")
        .with_generated_id_if_missing();
    let kickoff_id = kickoff.id.clone().unwrap();
    let conversation = Conversation::new_unvalidated(vec![kickoff]);

    let OperationResult::Applied(step) = commands.run(&(), &conversation, &emitter()).await? else {
        panic!("/review should run the skill");
    };
    assert!(step.effects.iter().any(|effect| matches!(
        effect,
        ConversationEffect::SetMessageVisibility { message_id, agent_visible: false, .. }
            if *message_id == kickoff_id
    )));
    let prompt = appended(&step.effects)[0];
    assert!(prompt.is_agent_visible() && !prompt.is_user_visible());
    assert!(prompt
        .as_concat_text()
        .contains("Review Card against a11y."));
    Ok(())
}
