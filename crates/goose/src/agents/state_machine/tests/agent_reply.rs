//! Covers `StateMachineServices::reply`, the entry point the CLI and desktop reach.

use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    Annotations as AcpAnnotations, ContentBlock as AcpContentBlock, EmbeddedResource,
    EmbeddedResourceResource, ResourceLink, Role as AcpRole, TextContent as AcpTextContent,
    TextResourceContents,
};
use anyhow::Result;
use futures::StreamExt;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::calculator_extension::{delayed_value, value, CalculatorExtension, ADD};
use super::dummy_api::{ApiCall, DummyApi, ProviderFeatures};
use super::pipeline::calculator_extension;
use crate::acp::server::AcpConnection;
use crate::agents::state_machine::ops_toolcalling::EXPIRED_APPROVAL_RESPONSE;
use crate::agents::{
    AgentEvent, GoosePlatform, SessionConfig, StateMachineServices, StateMachineServicesConfig,
};
use crate::config::permission::PermissionManager;
use crate::config::GooseMode;
use crate::conversation::message::{ActionRequiredData, Message, MessageContent};
use crate::permission::Permission;
use crate::providers::base::Provider;
use crate::session::{SessionManager, SessionType};
use goose_agent::extension::ExtensionConfig;
use goose_agent::mcp_client::McpClientTrait;
use goose_providers::model::ModelConfig;

async fn agent_with_dummy_api() -> Result<(
    Arc<StateMachineServices>,
    Arc<DummyApi>,
    String,
    tempfile::TempDir,
)> {
    agent_with_hooks(crate::hooks::HookManager::default()).await
}

async fn agent_with_hooks(
    hooks: crate::hooks::HookManager,
) -> Result<(
    Arc<StateMachineServices>,
    Arc<DummyApi>,
    String,
    tempfile::TempDir,
)> {
    agent_with(hooks, 100).await
}

async fn agent_with(
    hooks: crate::hooks::HookManager,
    max_sessions: usize,
) -> Result<(
    Arc<StateMachineServices>,
    Arc<DummyApi>,
    String,
    tempfile::TempDir,
)> {
    let api = Arc::new(DummyApi::start(ProviderFeatures::default()).await);
    let api_client = goose_providers::api_client::ApiClient::new_with_tls(
        api.uri(),
        goose_providers::api_client::AuthMethod::NoAuth,
        None,
    )?
    .with_request_builder(crate::session_context::session_id_request_builder());
    let provider: Arc<dyn Provider> = Arc::new(
        goose_providers::openai::OpenAiProviderBuilder::new(api_client)
            .base_path("chat/completions")
            .name("openai")
            .build(),
    );

    let temp_dir = tempfile::tempdir()?;
    let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
    let session = session_manager
        .create_session(
            temp_dir.path().to_path_buf(),
            "state-machine-reply".to_string(),
            SessionType::Hidden,
            GooseMode::Auto,
        )
        .await?;
    let mut config = StateMachineServicesConfig::new(
        session_manager,
        Arc::new(PermissionManager::new(temp_dir.path().join("permissions"))),
        None,
        true,
        GoosePlatform::GooseCli,
    );
    config.hooks = Some(hooks);
    config.max_sessions = max_sessions;
    let services = StateMachineServices::with_config(config);
    services
        .update_provider(
            provider,
            ModelConfig::new(goose_providers::openai::OPEN_AI_DEFAULT_MODEL)
                .with_canonical_limits("openai"),
            &session.id,
        )
        .await?;

    Ok((services, api, session.id, temp_dir))
}

async fn agent_with_calculator(
    hooks: crate::hooks::HookManager,
) -> Result<(
    Arc<StateMachineServices>,
    Arc<DummyApi>,
    String,
    Arc<CalculatorExtension>,
    tempfile::TempDir,
)> {
    let (services, api, session_id, temp_dir) = agent_with_hooks(hooks).await?;
    services
        .update_goose_mode(GooseMode::Approve, &session_id)
        .await?;
    let calculator = Arc::new(CalculatorExtension::new(
        services.session_manager.action_required(),
    ));
    services
        .extension_manager
        .add_test_client(
            &session_id,
            calculator_extension(),
            calculator.clone(),
            calculator.get_info().cloned(),
        )
        .await;
    Ok((services, api, session_id, calculator, temp_dir))
}

async fn enable_developer(services: &Arc<StateMachineServices>, session_id: &str) -> Result<()> {
    services
        .extension_manager
        .enable(
            session_id,
            ExtensionConfig::Platform {
                name: crate::agents::platform_extensions::developer::EXTENSION_NAME.to_string(),
                description: "Developer tools".to_string(),
                display_name: Some("Developer".to_string()),
                bundled: None,
                available_tools: Vec::new(),
            },
        )
        .await?;
    Ok(())
}

fn install_skill(working_dir: &std::path::Path, content: &str) -> Result<()> {
    let skill_dir = working_dir.join(".agents/skills/review");
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: review\ndescription: Review code\n---\n{content}"),
    )?;
    Ok(())
}

fn confirmation_ids(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|content| match content {
            MessageContent::ActionRequired(action) => match &action.data {
                ActionRequiredData::ToolConfirmation { id, .. } => Some(id.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

async fn stream_messages(
    mut stream: futures::stream::BoxStream<'_, Result<AgentEvent>>,
) -> Result<Vec<Message>> {
    let mut messages = Vec::new();
    while let Some(event) = stream.next().await {
        if let AgentEvent::Message(message) = event? {
            messages.push(message);
        }
    }
    Ok(messages)
}

#[tokio::test]
async fn state_machine_confirmation_through_agent_resumes_tool_call() -> Result<()> {
    let (hooks, _hook_dir, hook_log_path) = hook_log(&["PreToolUse"], 0)?;
    let (services, api, session_id, calculator, temp_dir) = agent_with_calculator(hooks).await?;

    api.on("add one").call(ADD, delayed_value(1, 80));
    api.on("result: 1").reply("the result is one");

    let session_config = SessionConfig {
        id: session_id,
        schedule_id: None,
        max_turns: Some(2),
    };
    let mut stream = services
        .reply(
            Message::user().with_text("add one"),
            session_config.clone(),
            Some(CancellationToken::new()),
        )
        .await?;
    let mut messages = Vec::new();
    let confirmation_id = loop {
        let event = stream
            .next()
            .await
            .expect("state machine should request confirmation")?;
        if let AgentEvent::Message(message) = event {
            let confirmation_id = confirmation_ids(std::slice::from_ref(&message)).pop();
            messages.push(message);
            if let Some(confirmation_id) = confirmation_id {
                break confirmation_id;
            }
        }
    };
    assert_eq!(calculator.total(), 0);

    let new_working_dir = tempfile::tempdir()?;
    services
        .session_manager
        .update(&session_config.id)
        .working_dir(new_working_dir.path().to_path_buf())
        .apply()
        .await?;

    {
        let session = services
            .session_manager
            .get_session(&session_config.id, true)
            .await?;
        assert!(confirmation_ids(
            session
                .conversation
                .as_ref()
                .expect("session conversation")
                .messages()
        )
        .contains(&confirmation_id));
    }

    let replacement = Arc::new(CalculatorExtension::new(
        services.session_manager.action_required(),
    ));
    services
        .extension_manager
        .add_test_client(
            &session_config.id,
            calculator_extension(),
            replacement.clone(),
            replacement.get_info().cloned(),
        )
        .await;

    services
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::AllowOnce)
        .await?;
    {
        let session = services
            .session_manager
            .get_session(&session_config.id, true)
            .await?;
        assert!(session
            .conversation
            .as_ref()
            .expect("session conversation")
            .messages()
            .iter()
            .any(|message| {
                message.content.iter().any(|content| {
                    matches!(
                        content,
                        MessageContent::ActionRequired(action)
                            if matches!(
                                &action.data,
                                ActionRequiredData::ToolConfirmationResponse { id, permission }
                                    if id == &confirmation_id && permission == &Permission::AllowOnce
                            )
                    )
                })
            }));
    }
    services
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::AllowOnce)
        .await?;
    assert!(services
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::DenyOnce)
        .await
        .is_err());

    loop {
        tokio::select! {
            event = stream.next() => {
                let event = event.expect("resumed turn should still be active")?;
                if let AgentEvent::Message(message) = event {
                    messages.push(message);
                }
            }
            _ = async {
                while calculator.contexts().is_empty() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            } => break,
        }
    }
    assert_eq!(calculator.total(), 0);
    drop(stream);
    let stream = services
        .resume_turn(session_config.clone(), CancellationToken::new())
        .await?
        .expect("persisted confirmation response should resume the state-machine turn");
    messages.extend(stream_messages(stream).await?);

    let hook_log = std::fs::read_to_string(&hook_log_path)?;
    let hook_payloads = hook_log
        .lines()
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<serde_json::Result<Vec<_>>>()?;
    assert_eq!(hook_payloads.len(), 2);
    assert!(hook_payloads.iter().all(|payload| {
        payload["working_dir"].as_str() == Some(temp_dir.path().to_string_lossy().as_ref())
    }));
    assert!(messages.iter().any(|message| message
        .get_tool_response_ids()
        .contains(&confirmation_id.as_str())));
    assert_eq!(
        (
            calculator.total(),
            calculator.contexts().len(),
            replacement.total(),
            replacement.contexts().len(),
        ),
        (1, 2, 0, 0),
    );
    assert!(calculator
        .contexts()
        .iter()
        .all(|context| { context.working_dir.as_deref() == Some(temp_dir.path()) }));
    assert_eq!(api.call_count(), 2);
    assert!(
        api.calls()
            .iter()
            .all(|call| call.session_id() == Some(session_config.id.as_str())),
        "initial and resumed provider requests must retain the session context"
    );

    assert!(services
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::AllowOnce)
        .await
        .is_err());
    assert_eq!(calculator.total(), 1);

    assert!(services
        .submit_tool_confirmation(&session_config.id, "stale-request", Permission::AllowOnce)
        .await
        .is_err());

    let session = services
        .session_manager
        .get_session(&session_config.id, true)
        .await?;
    let messages = session
        .conversation
        .as_ref()
        .expect("session conversation")
        .messages();
    let confirmation_responses = messages
        .iter()
        .filter(|message| {
            message.content.iter().any(|content| {
                matches!(
                    content,
                    MessageContent::ActionRequired(action)
                        if matches!(
                            &action.data,
                            ActionRequiredData::ToolConfirmationResponse { id, .. }
                                if id == &confirmation_id
                        )
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(confirmation_responses.len(), 1);
    assert!(!confirmation_responses[0].is_user_visible());
    assert!(!confirmation_responses[0].is_agent_visible());
    assert_eq!(
        messages
            .iter()
            .filter(|message| {
                message.role == rmcp::model::Role::User
                    && message.is_user_visible()
                    && !message.is_tool_response()
            })
            .count(),
        1
    );

    Ok(())
}

#[tokio::test]
async fn stop_at_a_confirmation_interrupts_the_waiting_call() -> Result<()> {
    let (services, api, session_id, calculator, _temp_dir) =
        agent_with_calculator(crate::hooks::HookManager::default()).await?;
    api.on("add one").calls([("call_add", ADD, value(1))]);

    let cancel = CancellationToken::new();
    let session_config = SessionConfig {
        id: session_id.clone(),
        schedule_id: None,
        max_turns: Some(2),
    };
    let mut stream = services
        .reply(
            Message::user().with_text("add one"),
            session_config,
            Some(cancel.clone()),
        )
        .await?;
    loop {
        let event = stream
            .next()
            .await
            .expect("state machine should request confirmation")?;
        if let AgentEvent::Message(message) = event {
            if !confirmation_ids(std::slice::from_ref(&message)).is_empty() {
                break;
            }
        }
    }
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(50), stream.next()).await {}
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(5), stream_messages(stream)).await??;

    let session = services
        .session_manager
        .get_session(&session_id, true)
        .await?;
    let conversation = session.conversation.expect("session conversation");
    let add_response = conversation
        .messages()
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(MessageContent::as_tool_response)
        .find(|response| response.id == "call_add")
        .expect("the waiting call is answered");
    assert_eq!(
        add_response.tool_result.as_ref().unwrap().content[0]
            .as_text()
            .unwrap()
            .text,
        "Tool call was interrupted before completing"
    );
    assert_eq!(calculator.total(), 0);
    Ok(())
}

#[tokio::test]
async fn state_machine_skill_approval_uses_its_leased_working_dir() -> Result<()> {
    let (services, api, session_id, old_working_dir) = agent_with_dummy_api().await?;
    install_skill(old_working_dir.path(), "OLD_SKILL_CONTENT")?;
    services
        .update_goose_mode(GooseMode::Approve, &session_id)
        .await?;

    api.on("load review")
        .call("load_skill", json!({ "name": "review" }));
    api.on("OLD_SKILL_CONTENT").reply("loaded original skill");

    let session_config = SessionConfig {
        id: session_id,
        schedule_id: None,
        max_turns: Some(2),
    };
    let mut stream = services
        .reply(
            Message::user().with_text("load review"),
            session_config.clone(),
            Some(CancellationToken::new()),
        )
        .await?;
    let confirmation_id = loop {
        let event = stream
            .next()
            .await
            .expect("state machine should request confirmation")?;
        if let AgentEvent::Message(message) = event {
            if let Some(confirmation_id) = confirmation_ids(std::slice::from_ref(&message)).pop() {
                break confirmation_id;
            }
        }
    };

    let new_working_dir = tempfile::tempdir()?;
    install_skill(new_working_dir.path(), "NEW_SKILL_CONTENT")?;
    services
        .session_manager
        .update(&session_config.id)
        .working_dir(new_working_dir.path().to_path_buf())
        .apply()
        .await?;
    services
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::AllowOnce)
        .await?;
    let messages = stream_messages(stream).await?;

    assert!(messages.iter().any(|message| {
        message.content.iter().any(|content| {
            content.as_tool_response_text().is_some_and(|text| {
                text.contains("OLD_SKILL_CONTENT") && !text.contains("NEW_SKILL_CONTENT")
            })
        })
    }));
    let calls = api.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[1].input_contains("OLD_SKILL_CONTENT"));
    assert!(!calls[1].input_contains("NEW_SKILL_CONTENT"));
    Ok(())
}

#[tokio::test]
async fn state_machine_rejects_resumed_skill_approval_without_its_lease() -> Result<()> {
    let (services, api, session_id, working_dir) = agent_with_dummy_api().await?;
    install_skill(working_dir.path(), "SKILL_MUST_NOT_LOAD")?;
    services
        .update_goose_mode(GooseMode::Approve, &session_id)
        .await?;

    api.on("load review")
        .call("load_skill", json!({ "name": "review" }));
    api.on(EXPIRED_APPROVAL_RESPONSE).reply("request it again");

    let session_config = SessionConfig {
        id: session_id,
        schedule_id: None,
        max_turns: Some(2),
    };
    let mut stream = services
        .reply(
            Message::user().with_text("load review"),
            session_config.clone(),
            Some(CancellationToken::new()),
        )
        .await?;
    let confirmation_id = loop {
        let event = stream
            .next()
            .await
            .expect("state machine should request confirmation")?;
        if let AgentEvent::Message(message) = event {
            if let Some(confirmation_id) = confirmation_ids(std::slice::from_ref(&message)).pop() {
                break confirmation_id;
            }
        }
    };

    services
        .runtime
        .confirmations
        .session(&session_config.id)
        .clear_extension_lease();
    services
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::AllowOnce)
        .await?;
    let messages = stream_messages(stream).await?;

    assert!(messages.iter().any(|message| {
        message.content.iter().any(|content| {
            content
                .as_tool_response_text()
                .is_some_and(|text| text.contains(EXPIRED_APPROVAL_RESPONSE))
        })
    }));
    let calls = api.calls();
    assert_eq!(calls.len(), 2);
    assert!(!calls[1].input_contains("SKILL_MUST_NOT_LOAD"));
    Ok(())
}

#[tokio::test]
async fn state_machine_rejects_resumed_approval_without_its_lease() -> Result<()> {
    let (services, api, session_id, calculator, _temp_dir) =
        agent_with_calculator(crate::hooks::HookManager::default()).await?;

    api.on("add one").call(ADD, value(1));
    api.on(EXPIRED_APPROVAL_RESPONSE).reply("request it again");

    let session_config = SessionConfig {
        id: session_id,
        schedule_id: None,
        max_turns: Some(2),
    };
    let mut stream = services
        .reply(
            Message::user().with_text("add one"),
            session_config.clone(),
            Some(CancellationToken::new()),
        )
        .await?;
    let confirmation_id = loop {
        let event = stream
            .next()
            .await
            .expect("state machine should request confirmation")?;
        if let AgentEvent::Message(message) = event {
            let confirmation_id = confirmation_ids(std::slice::from_ref(&message)).pop();
            if let Some(confirmation_id) = confirmation_id {
                break confirmation_id;
            }
        }
    };

    services
        .runtime
        .confirmations
        .session(&session_config.id)
        .clear_extension_lease();
    services
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::AllowOnce)
        .await?;
    stream_messages(stream).await?;

    assert_eq!(calculator.total(), 0);
    let session = services
        .session_manager
        .get_session(&session_config.id, true)
        .await?;
    assert!(session
        .conversation
        .as_ref()
        .expect("session conversation")
        .messages()
        .iter()
        .flat_map(|message| &message.content)
        .any(|content| {
            content
                .as_tool_response_text()
                .is_some_and(|text| text.contains(EXPIRED_APPROVAL_RESPONSE))
        }));
    assert_eq!(api.call_count(), 2);

    Ok(())
}

#[tokio::test]
async fn state_machine_rejects_resumed_bang_shell_without_its_lease() -> Result<()> {
    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on(EXPIRED_APPROVAL_RESPONSE).reply("request it again");
    enable_developer(&services, &session_id).await?;
    services
        .update_goose_mode(GooseMode::Approve, &session_id)
        .await?;
    let session_config = SessionConfig {
        id: session_id,
        schedule_id: None,
        max_turns: Some(2),
    };
    let mut stream = services
        .reply(
            Message::user().with_text("!echo should-not-run"),
            session_config.clone(),
            Some(CancellationToken::new()),
        )
        .await?;
    let confirmation_id = loop {
        let event = stream
            .next()
            .await
            .expect("state machine should request confirmation")?;
        if let AgentEvent::Message(message) = event {
            if let Some(confirmation_id) = confirmation_ids(std::slice::from_ref(&message)).pop() {
                break confirmation_id;
            }
        }
    };

    services
        .runtime
        .confirmations
        .session(&session_config.id)
        .clear_extension_lease();
    services
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::AllowOnce)
        .await?;
    stream_messages(stream).await?;

    let session = services
        .session_manager
        .get_session(&session_config.id, true)
        .await?;
    assert!(session
        .conversation
        .as_ref()
        .expect("session conversation")
        .messages()
        .iter()
        .flat_map(|message| &message.content)
        .any(|content| {
            content
                .as_tool_response_text()
                .is_some_and(|text| text.contains(EXPIRED_APPROVAL_RESPONSE))
        }));
    assert_eq!(api.call_count(), 0);
    Ok(())
}

#[tokio::test]
async fn reply_streams_the_turn_and_ends() -> Result<()> {
    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("are you there?").reply("still here");

    let session_config = SessionConfig {
        id: session_id.clone(),
        schedule_id: None,
        max_turns: Some(1),
    };
    let stream = services
        .reply(
            Message::user().with_text("are you there?"),
            session_config,
            Some(CancellationToken::new()),
        )
        .await?;

    let replies = tokio::time::timeout(Duration::from_secs(30), async move {
        tokio::pin!(stream);
        let mut replies = Vec::new();
        while let Some(event) = stream.next().await {
            if let AgentEvent::Message(message) = event? {
                replies.push(message.as_concat_text());
            }
        }
        anyhow::Ok(replies)
    })
    .await??;

    assert_eq!(replies.last().map(String::as_str), Some("still here"));
    assert_eq!(api.call_count(), 1);

    Ok(())
}

#[tokio::test]
async fn bang_shell_uses_state_machine_when_explicitly_enabled() -> Result<()> {
    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    enable_developer(&services, &session_id).await?;
    let session_config = SessionConfig {
        id: session_id,
        schedule_id: None,
        max_turns: Some(2),
    };
    let stream = services
        .reply(
            Message::user().with_text("!echo hello"),
            session_config,
            Some(CancellationToken::new()),
        )
        .await?;
    tokio::pin!(stream);
    let mut requested_shell = false;
    let mut shell_output = false;
    while let Some(event) = stream.next().await {
        if let AgentEvent::Message(message) = event? {
            requested_shell |= message.content.iter().any(|content| {
                matches!(
                    content,
                    crate::conversation::message::MessageContent::ToolRequest(request)
                        if request.tool_call.as_ref().is_ok_and(|call| call.name == "shell")
                )
            });
            shell_output |= message.content.iter().any(|content| {
                content
                    .as_tool_response_text()
                    .is_some_and(|text| text.contains("hello"))
            });
        }
    }

    assert!(requested_shell);
    assert!(shell_output);
    assert_eq!(api.call_count(), 0);

    Ok(())
}

async fn reply_messages(
    services: &Arc<StateMachineServices>,
    session_id: String,
    message: Message,
) -> Result<Vec<Message>> {
    let stream = services
        .reply(
            message,
            SessionConfig {
                id: session_id,
                schedule_id: None,
                max_turns: Some(2),
            },
            Some(CancellationToken::new()),
        )
        .await?;
    tokio::pin!(stream);
    let mut messages = Vec::new();
    while let Some(event) = stream.next().await {
        if let AgentEvent::Message(message) = event? {
            messages.push(message);
        }
    }
    Ok(messages)
}

fn assistant_only_acp_annotations() -> AcpAnnotations {
    AcpAnnotations::new().audience(vec![AcpRole::Assistant])
}

fn assistant_only_acp_text(text: &str) -> AcpContentBlock {
    AcpContentBlock::Text(AcpTextContent::new(text).annotations(assistant_only_acp_annotations()))
}

fn empty_audience_acp_annotations() -> AcpAnnotations {
    AcpAnnotations::new().audience(Vec::new())
}

fn empty_audience_acp_text(text: &str) -> AcpContentBlock {
    AcpContentBlock::Text(AcpTextContent::new(text).annotations(empty_audience_acp_annotations()))
}

fn assistant_only_embedded_resource(text: &str) -> AcpContentBlock {
    AcpContentBlock::Resource(
        EmbeddedResource::new(EmbeddedResourceResource::TextResourceContents(
            TextResourceContents::new(text, "file:///hidden-resource.txt"),
        ))
        .annotations(assistant_only_acp_annotations()),
    )
}

fn empty_audience_embedded_resource(text: &str) -> AcpContentBlock {
    AcpContentBlock::Resource(
        EmbeddedResource::new(EmbeddedResourceResource::TextResourceContents(
            TextResourceContents::new(text, "file:///empty-audience-resource.txt"),
        ))
        .annotations(empty_audience_acp_annotations()),
    )
}

fn assistant_only_resource_link(text: &str) -> Result<(AcpContentBlock, tempfile::NamedTempFile)> {
    let file = tempfile::NamedTempFile::new()?;
    std::fs::write(file.path(), text)?;
    let uri = url::Url::from_file_path(file.path())
        .map_err(|()| anyhow::anyhow!("temporary resource path is not a valid file URL"))?;
    let link = ResourceLink::new("hidden-resource.txt", uri.to_string())
        .annotations(assistant_only_acp_annotations());
    Ok((AcpContentBlock::ResourceLink(link), file))
}

fn shell_commands(messages: &[Message]) -> Vec<&str> {
    messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|content| match content {
            MessageContent::ToolRequest(request) => request
                .tool_call
                .as_ref()
                .ok()
                .filter(|call| call.name == "shell")
                .and_then(|call| call.arguments.as_ref())
                .and_then(|arguments| arguments.get("command"))
                .and_then(serde_json::Value::as_str),
            _ => None,
        })
        .collect()
}

async fn assert_bang_shell_uses_only_user_visible_content() -> Result<()> {
    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("benign visible input")
        .reply("handled as ordinary input");
    let hidden_text_prefix = AcpConnection::convert_acp_prompt_to_message(&[
        assistant_only_acp_text("!echo hidden"),
        AcpContentBlock::Text(AcpTextContent::new("benign visible input")),
    ]);
    let messages = reply_messages(&services, session_id, hidden_text_prefix).await?;
    assert!(shell_commands(&messages).is_empty());
    assert_eq!(api.call_count(), 1);

    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("benign visible input")
        .reply("handled as ordinary input");
    let empty_audience_text = AcpConnection::convert_acp_prompt_to_message(&[
        empty_audience_acp_text("!echo hidden"),
        AcpContentBlock::Text(AcpTextContent::new("benign visible input")),
    ]);
    let messages = reply_messages(&services, session_id, empty_audience_text).await?;
    assert!(shell_commands(&messages).is_empty());
    assert_eq!(api.call_count(), 1);

    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    let hidden_text_suffix = AcpConnection::convert_acp_prompt_to_message(&[
        AcpContentBlock::Text(AcpTextContent::new("!echo visible")),
        assistant_only_acp_text("&& echo hidden"),
    ]);
    let messages = reply_messages(&services, session_id, hidden_text_suffix).await?;
    assert_eq!(shell_commands(&messages), ["echo visible"]);
    assert_eq!(api.call_count(), 0);

    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("benign visible input")
        .reply("handled as ordinary input");
    let hidden_resource_prefix = AcpConnection::convert_acp_prompt_to_message(&[
        assistant_only_embedded_resource("!echo hidden"),
        AcpContentBlock::Text(AcpTextContent::new("benign visible input")),
    ]);
    let messages = reply_messages(&services, session_id, hidden_resource_prefix).await?;
    assert!(shell_commands(&messages).is_empty());
    assert_eq!(api.call_count(), 1);

    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("benign visible input")
        .reply("handled as ordinary input");
    let empty_audience_resource = AcpConnection::convert_acp_prompt_to_message(&[
        empty_audience_embedded_resource("!echo hidden"),
        AcpContentBlock::Text(AcpTextContent::new("benign visible input")),
    ]);
    let messages = reply_messages(&services, session_id, empty_audience_resource).await?;
    assert!(shell_commands(&messages).is_empty());
    assert_eq!(api.call_count(), 1);

    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    let (hidden_link, _resource_file) = assistant_only_resource_link("&& echo hidden")?;
    let hidden_link_suffix = AcpConnection::convert_acp_prompt_to_message(&[
        AcpContentBlock::Text(AcpTextContent::new("!echo visible")),
        hidden_link,
    ]);
    let messages = reply_messages(&services, session_id, hidden_link_suffix).await?;
    assert_eq!(shell_commands(&messages), ["echo visible"]);
    assert_eq!(api.call_count(), 0);

    Ok(())
}

#[tokio::test]
async fn bang_shell_visibility_is_enforced_when_state_machine_is_enabled() -> Result<()> {
    assert_bang_shell_uses_only_user_visible_content().await
}

#[tokio::test]
async fn a_subagent_runs_the_parents_hooks_and_is_released_when_it_finishes() -> Result<()> {
    let (hooks, _hook_dir, hook_log_path) = hook_log(&["PreToolUse", "SessionEnd"], 0)?;
    let (services, api, session_id, _temp_dir) = agent_with_hooks(hooks).await?;
    let host = api.uri();
    let _guard = env_lock::lock_env([
        ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
        ("OPENAI_HOST", Some(host.as_str())),
        ("OPENAI_BASE_PATH", Some("chat/completions")),
        ("OPENAI_CUSTOM_HEADERS", Some("")),
    ]);
    enable_developer(&services, &session_id).await?;
    services
        .extension_manager
        .enable(
            &session_id,
            ExtensionConfig::Platform {
                name: "summon".to_string(),
                description: "Delegate work".to_string(),
                display_name: None,
                bundled: None,
                available_tools: Vec::new(),
            },
        )
        .await?;

    api.on("hand this off").call(
        "delegate",
        json!({ "instructions": "print the subagent marker", "max_turns": 3 }),
    );
    api.on("print the subagent marker")
        .call("shell", json!({ "command": "echo marker-from-subagent" }));
    api.on("marker-from-subagent").call(
        crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME,
        json!({ "summary": "printed it" }),
    );
    api.on("printed it").reply("the subagent printed it");

    reply_messages(
        &services,
        session_id.clone(),
        Message::user().with_text("hand this off"),
    )
    .await?;

    let hook_log = std::fs::read_to_string(&hook_log_path)?;
    let payloads = hook_log
        .lines()
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<serde_json::Result<Vec<_>>>()?;
    let sessions_where = |event: &str, tool: Option<&str>| -> Vec<String> {
        payloads
            .iter()
            .filter(|payload| payload["event"].as_str() == Some(event))
            .filter(|payload| payload["tool_name"].as_str() == tool)
            .map(|payload| {
                payload["session_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    };
    let subagent_shell_calls = sessions_where("PreToolUse", Some("shell"));
    let [subagent_id] = subagent_shell_calls.as_slice() else {
        panic!("expected one shell call through the hook, got {subagent_shell_calls:?}");
    };
    assert_ne!(subagent_id, &session_id);
    assert_eq!(
        sessions_where("SessionEnd", None),
        std::slice::from_ref(subagent_id)
    );
    assert!(!services.tracks_confirmations_for(subagent_id));
    assert!(services
        .extension_manager
        .scope_lease(subagent_id)
        .await
        .configs()
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn a_subagent_asks_the_parents_client_for_approval_in_the_parents_mode() -> Result<()> {
    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    let host = api.uri();
    let _guard = env_lock::lock_env([
        ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
        ("OPENAI_HOST", Some(host.as_str())),
        ("OPENAI_BASE_PATH", Some("chat/completions")),
        ("OPENAI_CUSTOM_HEADERS", Some("")),
    ]);
    services
        .session_manager
        .update(&session_id)
        .goose_mode(GooseMode::Approve)
        .apply()
        .await?;
    enable_developer(&services, &session_id).await?;
    services
        .extension_manager
        .enable(
            &session_id,
            ExtensionConfig::Platform {
                name: "summon".to_string(),
                description: "Delegate work".to_string(),
                display_name: None,
                bundled: None,
                available_tools: Vec::new(),
            },
        )
        .await?;

    api.on("hand this off").call(
        "delegate",
        json!({ "instructions": "print the subagent marker", "max_turns": 3 }),
    );
    api.on("print the subagent marker")
        .call("shell", json!({ "command": "echo marker-from-subagent" }));
    api.on("marker-from-subagent").call(
        crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME,
        json!({ "summary": "printed it" }),
    );
    api.on("printed it").reply("the subagent printed it");

    let mut stream = services
        .reply(
            Message::user().with_text("hand this off"),
            SessionConfig {
                id: session_id.clone(),
                schedule_id: None,
                max_turns: Some(3),
            },
            None,
        )
        .await?;
    let mut approved_tools = Vec::new();
    let mut texts = Vec::new();
    while let Some(event) = stream.next().await {
        let AgentEvent::Message(message) = event? else {
            continue;
        };
        texts.push(message.as_concat_text());
        for content in &message.content {
            let MessageContent::ActionRequired(action) = content else {
                continue;
            };
            let ActionRequiredData::ToolConfirmation { id, tool_name, .. } = &action.data else {
                continue;
            };
            approved_tools.push(tool_name.clone());
            services
                .submit_tool_confirmation(&session_id, id, Permission::AllowOnce)
                .await?;
        }
    }

    assert_eq!(
        approved_tools,
        vec![
            "delegate",
            "shell",
            crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME,
        ]
    );
    assert!(texts.concat().ends_with("the subagent printed it"));
    Ok(())
}

fn serve_elicit_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
    use rmcp::ServiceExt;
    tokio::spawn(async move {
        let running = goose_test_support::mcp::McpFixtureServer::new()
            .serve((read, write))
            .await
            .unwrap();
        let _ = running.waiting().await;
    });
}

#[tokio::test]
async fn a_subagents_elicitation_is_answered_by_the_parents_client() -> Result<()> {
    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    let host = api.uri();
    let _guard = env_lock::lock_env([
        ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
        ("OPENAI_HOST", Some(host.as_str())),
        ("OPENAI_BASE_PATH", Some("chat/completions")),
        ("OPENAI_CUSTOM_HEADERS", Some("")),
    ]);
    crate::builtin_extension::register_builtin_extension("asks_a_name", serve_elicit_fixture);
    for config in [
        ExtensionConfig::Builtin {
            name: "asks_a_name".to_string(),
            display_name: None,
            description: "Asks for a name".to_string(),
            timeout: None,
            bundled: None,
            available_tools: Vec::new(),
        },
        ExtensionConfig::Platform {
            name: "summon".to_string(),
            description: "Delegate work".to_string(),
            display_name: None,
            bundled: None,
            available_tools: Vec::new(),
        },
    ] {
        services
            .extension_manager
            .enable(&session_id, config)
            .await?;
    }

    api.on("hand this off").call(
        "delegate",
        json!({ "instructions": "find out who is asking", "max_turns": 3 }),
    );
    api.on("find out who is asking")
        .call("asks_a_name__elicit", json!({ "echo_meta": true }));
    api.on("Ada Lovelace").call(
        crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME,
        json!({ "summary": "it was Ada" }),
    );
    api.on("it was Ada").reply("Ada asked");

    let mut stream = services
        .reply(
            Message::user().with_text("hand this off"),
            SessionConfig {
                id: session_id.clone(),
                schedule_id: None,
                max_turns: Some(3),
            },
            None,
        )
        .await?;
    let mut elicitations = 0;
    let mut texts = Vec::new();
    while let Some(event) = stream.next().await {
        let AgentEvent::Message(message) = event? else {
            continue;
        };
        texts.push(message.as_concat_text());
        for content in &message.content {
            let MessageContent::ActionRequired(action) = content else {
                continue;
            };
            let ActionRequiredData::Elicitation { id, .. } = &action.data else {
                continue;
            };
            elicitations += 1;
            crate::elicitation::complete_elicitation_with_generated_message(
                &services.session_manager,
                &session_id,
                id,
                goose_agent::action_required_manager::ElicitationOutcome::Accept(
                    json!({ "name": "Ada Lovelace" }),
                ),
            )
            .await?;
        }
    }

    assert_eq!(elicitations, 1);
    assert!(texts.concat().ends_with("Ada asked"));
    Ok(())
}

#[tokio::test]
async fn concurrent_subagents_get_their_own_answers_for_the_same_tool_call_id() -> Result<()> {
    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    let host = api.uri();
    let _guard = env_lock::lock_env([
        ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
        ("OPENAI_HOST", Some(host.as_str())),
        ("OPENAI_BASE_PATH", Some("chat/completions")),
        ("OPENAI_CUSTOM_HEADERS", Some("")),
    ]);
    services
        .session_manager
        .update(&session_id)
        .goose_mode(GooseMode::Approve)
        .apply()
        .await?;
    enable_developer(&services, &session_id).await?;
    services
        .extension_manager
        .enable(
            &session_id,
            ExtensionConfig::Platform {
                name: "summon".to_string(),
                description: "Delegate work".to_string(),
                display_name: None,
                bundled: None,
                available_tools: Vec::new(),
            },
        )
        .await?;

    api.on("hand both off").calls([
        (
            "call_first",
            "delegate",
            json!({ "instructions": "the approved task", "max_turns": 3 }),
        ),
        (
            "call_second",
            "delegate",
            json!({ "instructions": "the denied task", "max_turns": 3 }),
        ),
    ]);
    api.on("the approved task").calls([(
        "call_shell",
        "shell",
        json!({ "command": "printf 'ran-%s' approved" }),
    )]);
    api.on("the denied task").calls([(
        "call_shell",
        "shell",
        json!({ "command": "printf 'ran-%s' denied" }),
    )]);
    api.on("ran-approved").call(
        crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME,
        json!({ "summary": "approved one ran" }),
    );
    api.on(crate::agents::tool_execution::DECLINED_RESPONSE)
        .call(
            crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME,
            json!({ "summary": "denied one stopped" }),
        );
    api.on("denied one stopped").reply("both are back");

    let mut stream = services
        .reply(
            Message::user().with_text("hand both off"),
            SessionConfig {
                id: session_id.clone(),
                schedule_id: None,
                max_turns: Some(3),
            },
            None,
        )
        .await?;
    let mut shell_request_ids = Vec::new();
    while let Some(event) = stream.next().await {
        let AgentEvent::Message(message) = event? else {
            continue;
        };
        for content in &message.content {
            let MessageContent::ActionRequired(action) = content else {
                continue;
            };
            let ActionRequiredData::ToolConfirmation {
                id,
                tool_name,
                arguments,
                ..
            } = &action.data
            else {
                continue;
            };
            let denied = arguments
                .get("command")
                .is_some_and(|command| command.to_string().contains("denied"));
            let permission = if denied {
                Permission::DenyOnce
            } else {
                Permission::AllowOnce
            };
            if tool_name == "shell" {
                shell_request_ids.push(id.clone());
            }
            services
                .submit_tool_confirmation(&session_id, id, permission)
                .await?;
        }
    }

    assert_eq!(shell_request_ids.len(), 2);
    assert_ne!(shell_request_ids[0], shell_request_ids[1]);
    let calls = api.calls();
    assert!(calls.iter().any(|call| call.input_contains("ran-approved")));
    assert!(!calls.iter().any(|call| call.input_contains("ran-denied")));
    Ok(())
}

fn hook_log(
    events: &[&str],
    delay_seconds: u64,
) -> Result<(
    crate::hooks::HookManager,
    tempfile::TempDir,
    std::path::PathBuf,
)> {
    let hook_dir = tempfile::tempdir()?;
    let plugin_dir = hook_dir.path().join("test-plugin");
    std::fs::create_dir_all(plugin_dir.join("hooks"))?;
    let log_command = json!([{ "hooks": [{
        "type": "command",
        "command": format!(
            "payload=$(cat); sleep {delay_seconds}; printf '%s\\n' \"$payload\" >> \"$PLUGIN_ROOT/hook.log\""
        )
    }] }]);
    let hooks: serde_json::Map<String, serde_json::Value> = events
        .iter()
        .map(|event| (event.to_string(), log_command.clone()))
        .collect();
    std::fs::write(
        plugin_dir.join("hooks/hooks.json"),
        json!({ "hooks": hooks }).to_string(),
    )?;
    let hooks = crate::hooks::HookManager::from_plugins_for_test(vec![
        crate::plugins::discovery::DiscoveredPlugin {
            name: "test-plugin".into(),
            root: plugin_dir.clone(),
            scope: crate::plugins::discovery::PluginScope::Project,
        },
    ]);
    Ok((hooks, hook_dir, plugin_dir.join("hook.log")))
}

fn hook_sessions(log: &std::path::Path, event: &str) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|payload| payload["event"].as_str() == Some(event))
        .filter_map(|payload| payload["session_id"].as_str().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn cancelling_the_parent_ends_a_waiting_subagent() -> Result<()> {
    let (hooks, _hook_dir, hook_log) = hook_log(&["SessionEnd"], 0)?;
    let (services, api, session_id, _temp_dir) = agent_with_hooks(hooks).await?;
    let host = api.uri();
    let _guard = env_lock::lock_env([
        ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
        ("OPENAI_HOST", Some(host.as_str())),
        ("OPENAI_BASE_PATH", Some("chat/completions")),
        ("OPENAI_CUSTOM_HEADERS", Some("")),
    ]);
    services
        .session_manager
        .update(&session_id)
        .goose_mode(GooseMode::Approve)
        .apply()
        .await?;
    enable_developer(&services, &session_id).await?;
    enable_summon(&services, &session_id).await?;
    api.on("hand this off").call(
        "delegate",
        json!({ "instructions": "wait for approval", "max_turns": 3 }),
    );
    api.on("wait for approval")
        .call("shell", json!({ "command": "echo never" }));

    reply_approving(&services, &session_id, "hand this off", Some("shell")).await?;

    let subagent_id = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let [subagent_id] = hook_sessions(&hook_log, "SessionEnd").as_slice() {
                if !services.tracks_confirmations_for(subagent_id)
                    && services
                        .extension_manager
                        .scope_lease(subagent_id)
                        .await
                        .configs()
                        .is_empty()
                {
                    return subagent_id.clone();
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    assert_ne!(subagent_id, session_id);
    Ok(())
}

async fn enable_summon(services: &Arc<StateMachineServices>, session_id: &str) -> Result<()> {
    services
        .extension_manager
        .enable(
            session_id,
            ExtensionConfig::Platform {
                name: "summon".to_string(),
                description: "Delegate work".to_string(),
                display_name: None,
                bundled: None,
                available_tools: Vec::new(),
            },
        )
        .await?;
    Ok(())
}

async fn delegated_subagent_id(
    services: &Arc<StateMachineServices>,
    session_id: &str,
) -> Result<String> {
    let session = services
        .session_manager
        .get_session(session_id, true)
        .await?;
    session
        .conversation
        .as_ref()
        .unwrap()
        .messages()
        .iter()
        .flat_map(|message| &message.content)
        .find_map(|content| {
            let MessageContent::ToolResponse(response) = content else {
                return None;
            };
            let meta = response.tool_result.as_ref().ok()?.meta.as_ref()?;
            meta.0
                .get("subagent_session_id")?
                .as_str()
                .map(str::to_owned)
        })
        .ok_or_else(|| anyhow::anyhow!("no delegated subagent"))
}

async fn parent_texts(
    services: &Arc<StateMachineServices>,
    session_id: &str,
) -> Result<Vec<String>> {
    let session = services
        .session_manager
        .get_session(session_id, true)
        .await?;
    Ok(session
        .conversation
        .as_ref()
        .unwrap()
        .messages()
        .iter()
        .map(Message::as_concat_text)
        .collect())
}

async fn reply_approving(
    services: &Arc<StateMachineServices>,
    session_id: &str,
    text: &str,
    stop_at_tool: Option<&str>,
) -> Result<()> {
    let cancel = CancellationToken::new();
    let mut stream = services
        .reply(
            Message::user().with_text(text),
            SessionConfig {
                id: session_id.to_string(),
                schedule_id: None,
                max_turns: Some(3),
            },
            Some(cancel.clone()),
        )
        .await?;
    while let Some(event) = stream.next().await {
        let AgentEvent::Message(message) = event? else {
            continue;
        };
        for content in &message.content {
            let MessageContent::ActionRequired(action) = content else {
                continue;
            };
            let ActionRequiredData::ToolConfirmation { id, tool_name, .. } = &action.data else {
                continue;
            };
            if Some(tool_name.as_str()) == stop_at_tool {
                cancel.cancel();
            } else {
                services
                    .submit_tool_confirmation(session_id, id, Permission::AllowOnce)
                    .await?;
            }
        }
    }
    Ok(())
}

async fn resume_until_completed(
    services: &Arc<StateMachineServices>,
    api: &DummyApi,
    session_id: &str,
    subagent_id: &str,
) -> Result<ApiCall> {
    api.on("resume it").call(
        "delegate",
        json!({ "resume": subagent_id, "instructions": "finish the count" }),
    );
    api.on("finish the count").call(
        crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME,
        json!({ "summary": "counted after resume" }),
    );
    api.on("counted after resume").reply("all done");
    reply_approving(services, session_id, "resume it", None).await?;

    assert!(parent_texts(services, session_id)
        .await?
        .iter()
        .any(
            |text| text.starts_with(&format!("Subagent {subagent_id} completed"))
                && text.contains("counted after resume")
        ));
    api.calls()
        .into_iter()
        .find(|call| {
            call.session_id() == Some(subagent_id) && call.input_contains("finish the count")
        })
        .ok_or_else(|| anyhow::anyhow!("the resumed subagent did not call the model"))
}

#[tokio::test]
async fn a_subagent_that_ran_out_of_turns_finishes_after_resume() -> Result<()> {
    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    let host = api.uri();
    let _guard = env_lock::lock_env([
        ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
        ("OPENAI_HOST", Some(host.as_str())),
        ("OPENAI_BASE_PATH", Some("chat/completions")),
        ("OPENAI_CUSTOM_HEADERS", Some("")),
    ]);
    enable_developer(&services, &session_id).await?;
    enable_summon(&services, &session_id).await?;
    api.on("hand this off").call(
        "delegate",
        json!({ "instructions": "count the files", "max_turns": 1 }),
    );
    api.on("count the files")
        .call("shell", json!({ "command": "echo partial-progress" }));
    api.on("max turns reached")
        .reply("the subagent ran out of turns");

    reply_approving(&services, &session_id, "hand this off", None).await?;
    let subagent_id = delegated_subagent_id(&services, &session_id).await?;
    assert!(
        parent_texts(&services, &session_id)
            .await?
            .iter()
            .any(|text| text
                .starts_with(&format!("Subagent {subagent_id} failed: max turns reached")))
    );

    let resumed_request =
        resume_until_completed(&services, &api, &session_id, &subagent_id).await?;
    assert!(resumed_request.input_contains("partial-progress"));
    Ok(())
}

#[tokio::test]
async fn a_stopped_subagent_is_released_before_a_slow_session_end_hook_and_resumes() -> Result<()> {
    let (hooks, _hook_dir, hook_log) = hook_log(&["SessionEnd"], 2)?;
    let (services, api, session_id, _temp_dir) = agent_with_hooks(hooks).await?;
    let host = api.uri();
    let _guard = env_lock::lock_env([
        ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
        ("OPENAI_HOST", Some(host.as_str())),
        ("OPENAI_BASE_PATH", Some("chat/completions")),
        ("OPENAI_CUSTOM_HEADERS", Some("")),
    ]);
    services
        .session_manager
        .update(&session_id)
        .goose_mode(GooseMode::Approve)
        .apply()
        .await?;
    enable_developer(&services, &session_id).await?;
    enable_summon(&services, &session_id).await?;
    api.on("hand this off").call(
        "delegate",
        json!({ "instructions": "wait for approval", "max_turns": 3 }),
    );
    api.on("wait for approval")
        .call("shell", json!({ "command": "echo never" }));

    reply_approving(&services, &session_id, "hand this off", Some("shell")).await?;
    let subagent_id = delegated_subagent_id(&services, &session_id).await?;
    tokio::time::timeout(Duration::from_secs(1), async {
        while services.tracks_confirmations_for(&subagent_id)
            || !services
                .extension_manager
                .scope_lease(&subagent_id)
                .await
                .configs()
                .is_empty()
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    assert!(hook_sessions(&hook_log, "SessionEnd").is_empty());

    let resumed_request =
        resume_until_completed(&services, &api, &session_id, &subagent_id).await?;
    assert!(resumed_request.answers_every_tool_call_next());
    Ok(())
}

#[tokio::test]
async fn a_parent_tool_call_id_that_looks_forwarded_is_still_the_parents() -> Result<()> {
    let (services, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    services
        .session_manager
        .update(&session_id)
        .goose_mode(GooseMode::Approve)
        .apply()
        .await?;
    enable_developer(&services, &session_id).await?;
    api.on("run it").calls([(
        "subagent:not-a-session:call",
        "shell",
        json!({ "command": "printf 'ran-%s' parent" }),
    )]);
    api.on("ran-parent").reply("the parent ran it");

    let mut stream = services
        .reply(
            Message::user().with_text("run it"),
            SessionConfig {
                id: session_id.clone(),
                schedule_id: None,
                max_turns: Some(2),
            },
            None,
        )
        .await?;
    let mut texts = Vec::new();
    while let Some(event) = stream.next().await {
        let AgentEvent::Message(message) = event? else {
            continue;
        };
        texts.push(message.as_concat_text());
        for id in confirmation_ids(std::slice::from_ref(&message)) {
            services
                .submit_tool_confirmation(&session_id, &id, Permission::AllowOnce)
                .await?;
        }
    }
    assert!(texts.concat().ends_with("the parent ran it"));
    Ok(())
}

fn attachment() -> crate::agents::Attachment {
    crate::agents::Attachment {
        client: crate::agents::extension_manager::client_context(
            &GoosePlatform::GooseDesktop,
            None,
        ),
        session_names: None,
    }
}

#[tokio::test]
async fn a_finished_turn_gives_its_place_to_the_session_opened_meanwhile() -> Result<()> {
    let (services, api, session_id, _temp_dir) =
        agent_with(crate::hooks::HookManager::default(), 1).await?;
    api.on("hello").reply("hi there!");
    services.attach_session(&session_id, 1, attachment()).await;

    let events = services
        .reply(
            Message::user().with_text("hello"),
            SessionConfig {
                id: session_id.clone(),
                schedule_id: None,
                max_turns: None,
            },
            None,
        )
        .await?;
    services.attach_session("newer", 1, attachment()).await;
    assert!(services.runtime.is_attached(&session_id, 1));

    for event in events.collect::<Vec<_>>().await {
        event?;
    }
    assert!(services.runtime.is_attached("newer", 1));
    assert!(!services.runtime.is_attached(&session_id, 1));
    Ok(())
}
