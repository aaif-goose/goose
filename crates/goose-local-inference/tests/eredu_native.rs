#![cfg(all(feature = "eredu", feature = "hf-hub", target_os = "macos"))]

use futures::StreamExt;
use goose_local_inference::{
    config_resolver, evict_model,
    model::{ChatTemplate, ModelSettings, SamplingConfig, ToolCallingMode},
    LocalInferenceProvider,
};
use goose_provider_types::{
    base::Provider,
    conversation::{
        message::{Message, MessageContent},
        token_usage::ProviderUsage,
    },
    model::ModelConfig,
};
use rmcp::model::{CallToolResult, ContentBlock, Tool};
use serde_json::json;
use std::sync::Mutex;

static SETTINGS: Mutex<Option<ModelSettings>> = Mutex::new(None);

async fn collect(
    provider: &LocalInferenceProvider,
    config: &ModelConfig,
    messages: &[Message],
    tools: &[Tool],
) -> (Vec<Message>, ProviderUsage) {
    let mut stream = provider.stream(config, "You are a concise assistant. Follow the user's instructions and use tools when requested.", messages, tools).await.unwrap();
    let mut output = Vec::new();
    let mut usages = Vec::new();
    while let Some(item) = stream.next().await {
        let (message, usage) = item.unwrap();
        if let Some(message) = message {
            output.push(message);
        }
        if let Some(usage) = usage {
            usages.push(usage);
        }
    }
    assert_eq!(usages.len(), 1, "exactly one terminal usage");
    let usage = usages.pop().unwrap();
    let plan = &usage.additional_data.as_ref().unwrap()["eredu_execution_plan"];
    assert!(
        plan.to_string().contains("mlx"),
        "native run must use Eredu MLX: {plan}"
    );
    assert!(usage
        .stats
        .as_ref()
        .unwrap()
        .time_to_first_token_ms
        .is_some());
    println!(
        "messages={} usage={}",
        serde_json::to_string(&output).unwrap(),
        serde_json::to_string(&usage).unwrap()
    );
    (output, usage)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires GOOSE_EREDU_TEST_MODEL pointing at a downloaded native checkpoint"]
async fn native_chat_tools_templates_cancellation_and_speculation() {
    let path = std::env::var("GOOSE_EREDU_TEST_MODEL").unwrap();
    let phase = std::env::var("GOOSE_EREDU_TEST_PHASE").unwrap_or_else(|_| "all".into());
    let draft = std::env::var("GOOSE_EREDU_TEST_DRAFT").ok();
    config_resolver::set_model_settings_resolver(|_| Ok(SETTINGS.lock().unwrap().clone()));
    let settings = ModelSettings {
        chat_template: std::env::var("GOOSE_EREDU_TEST_TEMPLATE")
            .ok()
            .map(|path| ChatTemplate::CustomInline {
                template: std::fs::read_to_string(path).unwrap(),
            })
            .unwrap_or_default(),
        backend_id: Some("eredu".into()),
        draft_model: draft.clone(),
        ..Default::default()
    };
    *SETTINGS.lock().unwrap() = Some(settings.clone());
    let provider = LocalInferenceProvider::from_env().await.unwrap();
    let config =
        ModelConfig::new(&path)
            .with_thinking_effort(goose_provider_types::thinking::ThinkingEffort::High)
            .with_temperature(std::env::var("GOOSE_EREDU_TEST_TEMPERATURE").ok().map(
                |temperature| {
                    temperature
                        .parse()
                        .expect("GOOSE_EREDU_TEST_TEMPERATURE must be a number")
                },
            ))
            .with_max_tokens(
                std::env::var("GOOSE_EREDU_TEST_MAX_TOKENS")
                    .ok()
                    .map(|tokens| {
                        tokens
                            .parse()
                            .expect("GOOSE_EREDU_TEST_MAX_TOKENS must be an integer")
                    }),
            );
    let (output, usage) = collect(
        &provider,
        &config,
        &[Message::user().with_text("Reply with just the word hello.")],
        &[],
    )
    .await;
    assert!(output.iter().any(|message| message
        .content
        .iter()
        .any(|c| matches!(c, MessageContent::Text(t) if !t.text.trim().is_empty()))));
    if draft.is_some() {
        let stats = usage.stats.as_ref().unwrap().draft.as_ref().unwrap();
        assert!(stats.draft_tokens > 0);
        assert!(stats.rounds > 0);
        assert!(usage
            .additional_data
            .as_ref()
            .unwrap()
            .contains_key("eredu_speculation"));
    }

    let mut bounded = config.clone();
    bounded.max_tokens = Some(config.max_tokens.unwrap_or(512));
    let (_, reused) = collect(
        &provider,
        &bounded,
        &[Message::user().with_text("What is two plus two? Answer briefly.")],
        &[],
    )
    .await;
    assert_eq!(reused.stats.unwrap().model_load_ms, None);

    if phase == "all" || phase == "tools" {
        let mut native = settings.clone();
        native.tool_calling = ToolCallingMode::ForceNative;
        *SETTINGS.lock().unwrap() = Some(native);
        let tool = Tool::new(
            "lookup",
            "Look up the answer for a code. Set async to true to run in the background.",
            json!({"type":"object","properties":{"code":{"type":"integer","minimum":1},"async":{"type":"boolean","default":false}},"required":["code"]})
                .as_object()
                .unwrap()
                .clone(),
        );
        let mut history = vec![Message::user().with_text(
            "Use lookup to look up code 7 with async set to true. Do not answer until you have the tool result.",
        )];
        let (output, _) = collect(&provider, &bounded, &history, std::slice::from_ref(&tool)).await;
        let calls: Vec<_> = output
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContent::ToolRequest(call) => Some(call.clone()),
                _ => None,
            })
            .collect();
        assert!(!calls.is_empty(), "model should call lookup");
        for call in &calls {
            let tool_call = call.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "lookup");
            assert_eq!(tool_call.arguments.as_ref().unwrap()["code"], 7);
            assert_eq!(tool_call.arguments.as_ref().unwrap()["async"], true);
        }
        let mut assistant = Message::assistant();
        assistant.content = output
            .into_iter()
            .flat_map(|m| m.content)
            .filter(|c| !matches!(c, MessageContent::SystemNotification(_)))
            .collect();
        history.push(assistant);
        for call in calls {
            history.push(Message::user().with_tool_response(
                call.id,
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    "The answer is BLUE.",
                )])),
            ));
        }
        history.push(Message::user().with_text(
            "Now tell me the answer returned by lookup. Do not make another tool call.",
        ));
        let (answer, _) = collect(&provider, &bounded, &history, &[tool]).await;
        let text: String = answer
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(text.to_lowercase().contains("blue"));
    }
    // Reasoning is an explicit phase because some native checkpoints cannot emit it.
    if phase == "reasoning" {
        let mut thinking = settings.clone();
        thinking.enable_thinking = Some(true);
        *SETTINGS.lock().unwrap() = Some(thinking);
        let (reasoning, _) = collect(
            &provider,
            &bounded,
            &[Message::user().with_text(
                "Think briefly about why 17 is prime, then give a one-sentence answer.",
            )],
            &[],
        )
        .await;
        assert!(
            reasoning
                .iter()
                .flat_map(|m| &m.content)
                .any(|c| matches!(c, MessageContent::Thinking(t) if !t.thinking.trim().is_empty())),
            "the reasoning phase requires a checkpoint that emits reasoning"
        );
    }
    if phase == "all" || phase == "remaining" || phase == "emulation" {
        let mut emulated = settings.clone();
        emulated.tool_calling = ToolCallingMode::ForceEmulated;
        *SETTINGS.lock().unwrap() = Some(emulated);
        let tool = Tool::new(
        "developer__shell",
        "Run a shell command",
        json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"]})
            .as_object()
            .unwrap()
            .clone(),
    );
        let (emulated, _) = collect(
        &provider,
        &bounded,
        &[Message::user().with_text(
            "Emit exactly this command on its own line, followed by a newline, with no Markdown fence: $ pwd",
        )],
        &[tool],
    )
    .await;
        assert!(emulated
            .iter()
            .flat_map(|m| &m.content)
            .any(|c| matches!(c, MessageContent::ToolRequest(_))));
    }
    *SETTINGS.lock().unwrap() = Some(settings.clone());
    let stream = provider
        .stream(
            &bounded,
            "Count slowly",
            &[Message::user().with_text("Count from 1 to 1000")],
            &[],
        )
        .await
        .unwrap();
    drop(stream);
    let (_, reused) = collect(
        &provider,
        &bounded,
        &[Message::user().with_text("Say hello.")],
        &[],
    )
    .await;
    assert_eq!(reused.stats.unwrap().model_load_ms, None);

    if phase == "all" || phase == "remaining" || phase == "custom" {
        let mut custom = settings;
        let template =
            std::fs::read_to_string(std::path::Path::new(&path).join("chat_template.jinja"))
                .unwrap();
        custom.chat_template = ChatTemplate::CustomInline {
            template: format!("{{# Goose explicit override #}}\n{template}"),
        };
        custom.sampling = SamplingConfig::MirostatV2 {
            temperature: Some(0.8),
            tau: 5.0,
            eta: 0.1,
            seed: Some(42),
        };
        *SETTINGS.lock().unwrap() = Some(custom);
        let (_, reloaded) = collect(
            &provider,
            &bounded,
            &[Message::user().with_text("Reply hello.")],
            &[],
        )
        .await;
        assert!(reloaded.stats.unwrap().model_load_ms.is_some());
    }
    assert!(evict_model(&path).await.unwrap());
}
