//! Covers `Agent::reply_with_state_machine`, the entry point the CLI and desktop
//! reach when the state machine is enabled.

use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    Annotations as AcpAnnotations, ContentBlock as AcpContentBlock, EmbeddedResource,
    EmbeddedResourceResource, ResourceLink, Role as AcpRole, TextContent as AcpTextContent,
    TextResourceContents,
};
use anyhow::Result;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use super::calculator_extension::{delayed_value, value, CalculatorExtension, ADD};
use super::dummy_api::{DummyApi, ProviderFeatures};
use crate::acp::server::GooseAcpAgent;
use crate::agents::extension::ExtensionConfig;
use crate::agents::mcp_client::McpClientTrait;
use crate::agents::{Agent, AgentConfig, AgentEvent, GoosePlatform, SessionConfig};
use crate::config::permission::PermissionManager;
use crate::config::GooseMode;
use crate::conversation::message::{ActionRequiredData, Message, MessageContent};
use crate::permission::Permission;
use crate::providers::base::Provider;
use crate::session::{SessionManager, SessionType};
use goose_providers::model::ModelConfig;

async fn agent_with_dummy_api() -> Result<(Agent, Arc<DummyApi>, String, tempfile::TempDir)> {
    let api = Arc::new(DummyApi::start(ProviderFeatures::default()).await);
    let api_client = goose_providers::api_client::ApiClient::new_with_tls(
        api.uri(),
        goose_providers::api_client::AuthMethod::NoAuth,
        None,
    )?;
    let provider: Arc<dyn Provider> = Arc::new(
        goose_providers::openai::OpenAiProviderBuilder::new(api_client)
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
    let agent = Agent::with_config(AgentConfig::new(
        session_manager,
        Arc::new(PermissionManager::new(temp_dir.path().join("permissions"))),
        None,
        GooseMode::Auto,
        true,
        GoosePlatform::GooseCli,
    ));
    agent
        .update_provider(
            provider,
            ModelConfig::new(goose_providers::openai::OPEN_AI_DEFAULT_MODEL)
                .with_canonical_limits("openai"),
            &session.id,
        )
        .await?;

    Ok((agent, api, session.id, temp_dir))
}

async fn agent_with_calculator() -> Result<(
    Agent,
    Arc<DummyApi>,
    String,
    Arc<CalculatorExtension>,
    tempfile::TempDir,
)> {
    let (agent, api, session_id, temp_dir) = agent_with_dummy_api().await?;
    agent
        .update_goose_mode(GooseMode::Approve, &session_id)
        .await?;
    let calculator = Arc::new(CalculatorExtension::new(
        agent.config.session_manager.action_required(),
    ));
    agent
        .extension_manager
        .add_client(
            "calculator".to_string(),
            ExtensionConfig::Platform {
                name: "calculator".to_string(),
                description: "Stateful test calculator".to_string(),
                display_name: None,
                bundled: None,
                available_tools: vec![],
            },
            calculator.clone(),
            calculator.get_info().cloned(),
        )
        .await;
    Ok((agent, api, session_id, calculator, temp_dir))
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

async fn next_confirmation(
    stream: &mut futures::stream::BoxStream<'_, Result<AgentEvent>>,
) -> Result<String> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = stream.next().await {
            if let AgentEvent::Message(message) = event? {
                if let Some(id) = confirmation_ids(&[message]).pop() {
                    return Ok(id);
                }
            }
        }
        anyhow::bail!("turn ended without requesting confirmation")
    })
    .await?
}

async fn check_cancelled_approval(state_machine: bool, late_answer: bool) -> Result<()> {
    let _guard = env_lock::lock_env([(
        "GOOSE_STATE_MACHINE",
        Some(if state_machine { "1" } else { "0" }),
    )]);
    let (mut agent, api, session_id, calculator, temp_dir) = agent_with_calculator().await?;
    let plugin_root = temp_dir.path().join("approval-hook");
    std::fs::create_dir_all(plugin_root.join("hooks"))?;
    std::fs::write(
        plugin_root.join("hooks/hooks.json"),
        r#"{"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"echo ran >> \"${PLUGIN_ROOT}/ran\""}]}]}}"#,
    )?;
    agent.set_hook_manager_for_test(crate::hooks::HookManager::from_plugins_for_test(vec![
        crate::plugins::discovery::DiscoveredPlugin {
            name: "approval-hook".into(),
            root: plugin_root.clone(),
            scope: crate::plugins::discovery::PluginScope::Project,
        },
    ]));
    api.on("add one").call(ADD, value(1));
    let cancel = CancellationToken::new();
    let config = SessionConfig {
        id: session_id.clone(),
        schedule_id: None,
        max_turns: Some(2),
        retry_config: None,
    };
    let mut stream = agent
        .reply(
            Message::user().with_text("add one"),
            config.clone(),
            state_machine,
            Some(cancel.clone()),
        )
        .await?;
    let id = next_confirmation(&mut stream).await?;
    cancel.cancel();
    if late_answer {
        // Race a queued answer against the next poll of the cancelled turn.
        let _ = agent
            .submit_tool_confirmation(&session_id, &id, Permission::AlwaysAllow)
            .await;
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(event) = stream.next().await {
            event.expect("cancelling an approval wait must end the stream without an error");
        }
    })
    .await
    .expect("Stop must finish an approval wait without a client answer");
    drop(stream);
    assert_eq!(calculator.total(), 0);
    assert!(
        !plugin_root.join("ran").exists(),
        "late approval ran a hook"
    );
    assert_eq!(
        agent.config.permission_manager.get_user_permission(ADD),
        None
    );
    let reloaded = PermissionManager::new(temp_dir.path().join("permissions"));
    assert_eq!(reloaded.get_user_permission(ADD), None);
    assert!(agent
        .submit_tool_confirmation(&session_id, &id, Permission::AlwaysAllow)
        .await
        .is_err());
    let saved = agent
        .config
        .session_manager
        .get_session(&session_id, true)
        .await?;
    let conversation = saved.conversation.unwrap();
    assert!(
        conversation
            .messages()
            .iter()
            .all(|message| !message.content.is_empty()),
        "cancelled approval persisted an empty message"
    );
    if !state_machine {
        let requests: Vec<_> = conversation
            .messages()
            .iter()
            .flat_map(Message::get_tool_request_ids)
            .collect();
        let responses: Vec<_> = conversation
            .messages()
            .iter()
            .flat_map(Message::get_tool_response_ids)
            .collect();
        assert_eq!(requests, responses);
        assert_eq!(requests, vec![id.as_str()]);
    }
    assert!(!conversation.messages().iter().any(|message| {
        message.content.iter().any(|content| matches!(content,
            MessageContent::ActionRequired(action) if matches!(
                &action.data,
                ActionRequiredData::ToolConfirmationResponse { id: saved_id, permission: Permission::AlwaysAllow }
                    if saved_id == &id
            )
        ))
    }));
    api.on("continue without tools").reply("continued safely");
    let messages = tokio::time::timeout(
        Duration::from_secs(5),
        stream_messages(
            agent
                .reply(
                    Message::user().with_text("continue without tools"),
                    config,
                    state_machine,
                    Some(CancellationToken::new()),
                )
                .await?,
        ),
    )
    .await??;
    assert!(messages
        .iter()
        .any(|message| message.as_concat_text().contains("continued safely")));
    assert_eq!(calculator.total(), 0);
    assert!(!plugin_root.join("ran").exists());
    if !state_machine {
        assert!(api
            .calls()
            .last()
            .unwrap()
            .input_contains("Tool call was interrupted before completing"));
    }
    Ok(())
}

#[tokio::test]
async fn cancelled_approval_finishes_without_an_answer() -> Result<()> {
    for state_machine in [false, true] {
        check_cancelled_approval(state_machine, false).await?;
    }
    Ok(())
}

#[tokio::test]
async fn cancelled_approval_ignores_queued_always_allow() -> Result<()> {
    for state_machine in [false, true] {
        check_cancelled_approval(state_machine, true).await?;
    }
    Ok(())
}

#[tokio::test]
async fn cancelled_tool_batch_preserves_completed_results() -> Result<()> {
    for state_machine in [false, true] {
        let _guard = env_lock::lock_env([(
            "GOOSE_STATE_MACHINE",
            Some(if state_machine { "1" } else { "0" }),
        )]);
        let (agent, api, session_id, calculator, _temp_dir) = agent_with_calculator().await?;
        agent
            .update_goose_mode(GooseMode::Auto, &session_id)
            .await?;
        api.on("cancel unfinished calls").calls([
            ("completed", ADD, value(1)),
            ("unfinished", ADD, delayed_value(10, 500)),
        ]);
        let cancel = CancellationToken::new();
        let stream = agent
            .reply(
                Message::user().with_text("cancel unfinished calls"),
                SessionConfig {
                    id: session_id.clone(),
                    schedule_id: None,
                    max_turns: Some(2),
                    retry_config: None,
                },
                state_machine,
                Some(cancel.clone()),
            )
            .await?;
        let cancel_after_result = async {
            calculator.wait_for_result().await;
            cancel.cancel();
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(stream_messages(stream), cancel_after_result)
        })
        .await?;
        result?;
        assert_eq!(calculator.total(), 1);
        let conversation = agent
            .config
            .session_manager
            .get_session(&session_id, true)
            .await?
            .conversation
            .unwrap();
        assert!(conversation
            .messages()
            .iter()
            .all(|message| !message.content.is_empty()));
        let responses: Vec<_> = conversation
            .messages()
            .iter()
            .flat_map(|message| &message.content)
            .filter_map(MessageContent::as_tool_response)
            .collect();
        assert_eq!(responses.len(), 2);
        let completed = responses
            .iter()
            .find(|response| response.id == "completed")
            .unwrap();
        let result = completed.tool_result.as_ref().unwrap();
        assert_ne!(result.is_error, Some(true));
        assert_eq!(result.content[0].as_text().unwrap().text, "result: 1");
        let unfinished = responses
            .iter()
            .find(|response| response.id == "unfinished")
            .unwrap();
        assert!(unfinished
            .tool_result
            .as_ref()
            .map_or(true, |result| result.is_error == Some(true)));
    }
    Ok(())
}

#[tokio::test]
async fn live_approval_answers_continue_both_agent_loops() -> Result<()> {
    use crate::config::permission::PermissionLevel;

    for state_machine in [false, true] {
        let _guard = env_lock::lock_env([(
            "GOOSE_STATE_MACHINE",
            Some(if state_machine { "1" } else { "0" }),
        )]);
        for (permission, total, saved_permission) in [
            (Permission::AllowOnce, 1, None),
            (
                Permission::AlwaysAllow,
                1,
                Some(PermissionLevel::AlwaysAllow),
            ),
            (Permission::DenyOnce, 0, None),
            (Permission::AlwaysDeny, 0, Some(PermissionLevel::NeverAllow)),
        ] {
            let (agent, api, session_id, calculator, _temp_dir) = agent_with_calculator().await?;
            api.on("add one").call(ADD, value(1));
            api.on("result: 1").reply("the result is one");
            api.on("user has declined").reply("the tool was declined");
            let mut stream = agent
                .reply(
                    Message::user().with_text("add one"),
                    SessionConfig {
                        id: session_id.clone(),
                        schedule_id: None,
                        max_turns: Some(2),
                        retry_config: None,
                    },
                    state_machine,
                    Some(CancellationToken::new()),
                )
                .await?;
            let id = next_confirmation(&mut stream).await?;
            agent
                .submit_tool_confirmation(&session_id, &id, permission)
                .await?;
            let messages =
                tokio::time::timeout(Duration::from_secs(5), stream_messages(stream)).await??;
            assert!(messages
                .iter()
                .any(|message| message.get_tool_response_ids().contains(&id.as_str())));
            assert_eq!(calculator.total(), total);
            assert_eq!(
                agent.config.permission_manager.get_user_permission(ADD),
                saved_permission
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn state_machine_confirmation_through_agent_resumes_tool_call() -> Result<()> {
    let _guard = env_lock::lock_env([("GOOSE_STATE_MACHINE", Some("1"))]);
    let (agent, api, session_id, calculator, _temp_dir) = agent_with_calculator().await?;
    let agent = Arc::new(agent);

    api.on("add one").call(ADD, value(1));
    api.on("result: 1").reply("the result is one");

    let session_config = SessionConfig {
        id: session_id,
        schedule_id: None,
        max_turns: Some(2),
        retry_config: None,
    };
    let mut stream = agent
        .reply(
            Message::user().with_text("add one"),
            session_config.clone(),
            true,
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
    {
        let session = agent
            .config
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

    agent
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::AllowOnce)
        .await?;
    {
        let session = agent
            .config
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
    agent
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::AllowOnce)
        .await?;
    assert!(agent
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::DenyOnce)
        .await
        .is_err());
    drop(stream);
    let stream = agent
        .resume_state_machine_turn(session_config.clone(), CancellationToken::new())
        .await?
        .expect("persisted confirmation response should resume the state-machine turn");
    messages.extend(stream_messages(stream).await?);
    assert!(messages.iter().any(|message| message
        .get_tool_response_ids()
        .contains(&confirmation_id.as_str())));
    assert_eq!(calculator.total(), 1);
    assert_eq!(api.call_count(), 2);

    assert!(agent
        .submit_tool_confirmation(&session_config.id, &confirmation_id, Permission::AllowOnce)
        .await
        .is_err());
    assert_eq!(calculator.total(), 1);

    assert!(agent
        .submit_tool_confirmation(&session_config.id, "stale-request", Permission::AllowOnce)
        .await
        .is_err());

    let session = agent
        .config
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
async fn reply_streams_the_turn_and_ends() -> Result<()> {
    let (agent, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("are you there?").reply("still here");

    let session_config = SessionConfig {
        id: session_id.clone(),
        schedule_id: None,
        max_turns: Some(2),
        retry_config: None,
    };
    let stream = agent
        .reply_with_state_machine(
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

    assert!(
        replies.iter().any(|reply| reply == "still here"),
        "expected the scripted reply, got {replies:?}"
    );
    assert_eq!(api.call_count(), 1);

    Ok(())
}

#[tokio::test]
async fn bang_shell_uses_state_machine_when_explicitly_enabled() -> Result<()> {
    let (agent, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    let session_config = SessionConfig {
        id: session_id,
        schedule_id: None,
        max_turns: Some(2),
        retry_config: None,
    };
    let stream = agent
        .reply(
            Message::user().with_text("!echo hello"),
            session_config,
            true,
            Some(CancellationToken::new()),
        )
        .await?;
    tokio::pin!(stream);
    let mut requested_shell = false;
    while let Some(event) = stream.next().await {
        if let AgentEvent::Message(message) = event? {
            requested_shell |= message.content.iter().any(|content| {
                matches!(
                    content,
                    crate::conversation::message::MessageContent::ToolRequest(request)
                        if request.tool_call.as_ref().is_ok_and(|call| call.name == "shell")
                )
            });
        }
    }

    assert!(requested_shell);
    assert_eq!(api.call_count(), 0);

    Ok(())
}

async fn reply_messages(
    agent: &Agent,
    session_id: String,
    message: Message,
) -> Result<Vec<Message>> {
    let stream = agent
        .reply(
            message,
            SessionConfig {
                id: session_id,
                schedule_id: None,
                max_turns: Some(2),
                retry_config: None,
            },
            crate::agents::state_machine::enabled(),
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
    let (agent, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("benign visible input")
        .reply("handled as ordinary input");
    let hidden_text_prefix = GooseAcpAgent::convert_acp_prompt_to_message(&[
        assistant_only_acp_text("!echo hidden"),
        AcpContentBlock::Text(AcpTextContent::new("benign visible input")),
    ]);
    let messages = reply_messages(&agent, session_id, hidden_text_prefix).await?;
    assert!(shell_commands(&messages).is_empty());
    assert_eq!(api.call_count(), 1);

    let (agent, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("benign visible input")
        .reply("handled as ordinary input");
    let empty_audience_text = GooseAcpAgent::convert_acp_prompt_to_message(&[
        empty_audience_acp_text("!echo hidden"),
        AcpContentBlock::Text(AcpTextContent::new("benign visible input")),
    ]);
    let messages = reply_messages(&agent, session_id, empty_audience_text).await?;
    assert!(shell_commands(&messages).is_empty());
    assert_eq!(api.call_count(), 1);

    let (agent, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    let hidden_text_suffix = GooseAcpAgent::convert_acp_prompt_to_message(&[
        AcpContentBlock::Text(AcpTextContent::new("!echo visible")),
        assistant_only_acp_text("&& echo hidden"),
    ]);
    let messages = reply_messages(&agent, session_id, hidden_text_suffix).await?;
    assert_eq!(shell_commands(&messages), ["echo visible"]);
    assert_eq!(api.call_count(), 0);

    let (agent, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("benign visible input")
        .reply("handled as ordinary input");
    let hidden_resource_prefix = GooseAcpAgent::convert_acp_prompt_to_message(&[
        assistant_only_embedded_resource("!echo hidden"),
        AcpContentBlock::Text(AcpTextContent::new("benign visible input")),
    ]);
    let messages = reply_messages(&agent, session_id, hidden_resource_prefix).await?;
    assert!(shell_commands(&messages).is_empty());
    assert_eq!(api.call_count(), 1);

    let (agent, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("benign visible input")
        .reply("handled as ordinary input");
    let empty_audience_resource = GooseAcpAgent::convert_acp_prompt_to_message(&[
        empty_audience_embedded_resource("!echo hidden"),
        AcpContentBlock::Text(AcpTextContent::new("benign visible input")),
    ]);
    let messages = reply_messages(&agent, session_id, empty_audience_resource).await?;
    assert!(shell_commands(&messages).is_empty());
    assert_eq!(api.call_count(), 1);

    let (agent, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    let (hidden_link, _resource_file) = assistant_only_resource_link("&& echo hidden")?;
    let hidden_link_suffix = GooseAcpAgent::convert_acp_prompt_to_message(&[
        AcpContentBlock::Text(AcpTextContent::new("!echo visible")),
        hidden_link,
    ]);
    let messages = reply_messages(&agent, session_id, hidden_link_suffix).await?;
    assert_eq!(shell_commands(&messages), ["echo visible"]);
    assert_eq!(api.call_count(), 0);

    Ok(())
}

#[tokio::test]
async fn bang_shell_not_executed_in_legacy_loop() -> Result<()> {
    let _guard = env_lock::lock_env([("GOOSE_STATE_MACHINE", None::<&str>)]);
    let (agent, api, session_id, _temp_dir) = agent_with_dummy_api().await?;
    api.on("!echo hello").reply("treated as text");
    let messages =
        reply_messages(&agent, session_id, Message::user().with_text("!echo hello")).await?;
    assert!(shell_commands(&messages).is_empty());
    assert_eq!(api.call_count(), 1);
    Ok(())
}

#[tokio::test]
async fn bang_shell_visibility_is_enforced_when_state_machine_is_enabled() -> Result<()> {
    let _guard = env_lock::lock_env([("GOOSE_STATE_MACHINE", Some("1"))]);
    assert_bang_shell_uses_only_user_visible_content().await
}
