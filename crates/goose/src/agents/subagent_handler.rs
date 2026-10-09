use crate::{
    agents::{
        final_output_tool::FinalOutputTool,
        state_machine::{trailing_error, MAX_TURNS_MESSAGE},
        Agent, AgentConfig, GoosePlatform, SessionConfig,
    },
    config::permission::PermissionManager,
    conversation::{message::Message, Conversation},
    prompt_template::render_template,
    session::extension_data::{EnabledExtensionsState, ExtensionState},
    session::{Session, SessionManager, SessionType},
};
use anyhow::{anyhow, Result};
use futures::future::BoxFuture;
use futures::StreamExt;
use goose_agent::subagent::SubagentOutcome;
use rmcp::model::Role;
use serde::Serialize;
use std::future::Future;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, Instrument};

#[derive(Serialize)]
pub struct SubagentPromptContext {
    pub max_turns: usize,
    pub tool_count: usize,
    pub available_tools: String,
}

pub(crate) async fn from_foreground_subagent_session(
    session_manager: Arc<SessionManager>,
    session: &Session,
    use_login_shell_path: bool,
) -> Result<(Agent, SessionConfig)> {
    let session_id = &session.id;
    if session.session_type != SessionType::SubAgent {
        return Err(anyhow!("Session {session_id} is not a subagent"));
    }
    let recipe = session
        .recipe
        .as_ref()
        .ok_or_else(|| anyhow!("Subagent {session_id} has no saved recipe"))?;
    let max_turns = recipe
        .settings
        .as_ref()
        .and_then(|settings| settings.max_turns)
        .ok_or_else(|| anyhow!("Subagent {session_id} has no saved turn limit"))?;
    let provider_name = session
        .provider_name
        .as_deref()
        .ok_or_else(|| anyhow!("Subagent {session_id} has no saved provider"))?;
    let model_config = session
        .model_config
        .as_ref()
        .ok_or_else(|| anyhow!("Subagent {session_id} has no saved model"))?;
    let saved_extensions = session
        .extension_data
        .get_extension_state(
            EnabledExtensionsState::EXTENSION_NAME,
            EnabledExtensionsState::VERSION,
        )
        .ok_or_else(|| anyhow!("Subagent {session_id} has no saved extension selection"))?;
    let extensions = EnabledExtensionsState::from_value(saved_extensions)?.extensions;

    let mut config = AgentConfig::new(
        session_manager,
        PermissionManager::instance(),
        None,
        true,
        GoosePlatform::GooseCli,
    )
    .with_use_login_shell_path(use_login_shell_path);
    config.is_subagent = true;
    let agent = Agent::with_config(config);
    agent
        .switch_provider(session_id, provider_name, model_config.clone())
        .await?;
    for extension in extensions {
        let name = extension.name();
        if let Err(e) = agent.add_extension_inner(extension, session_id).await {
            debug!("Failed to add extension '{}' to subagent: {}", name, e);
        }
    }

    let subagent_prompt = build_subagent_prompt(&agent, max_turns, session_id).await?;
    agent
        .config
        .session_manager
        .update(session_id)
        .system_prompt_override(Some(subagent_prompt))
        .apply()
        .await?;
    let session_config = SessionConfig {
        id: session_id.to_string(),
        schedule_id: None,
        max_turns: Some(max_turns as u32),
    };
    Ok((agent, session_config))
}

#[derive(Clone)]
pub(crate) struct ForegroundSubagentRunner {
    session_manager: Arc<SessionManager>,
    use_login_shell_path: bool,
}

impl ForegroundSubagentRunner {
    pub(crate) fn new(session_manager: Arc<SessionManager>, use_login_shell_path: bool) -> Self {
        Self {
            session_manager,
            use_login_shell_path,
        }
    }

    pub(crate) async fn load(&self, subagent_id: &str) -> Result<Session> {
        self.session_manager.get_session(subagent_id, true).await
    }

    pub(crate) fn spawn(
        &self,
        subagent: Session,
        cancel: CancellationToken,
    ) -> BoxFuture<'static, SubagentOutcome> {
        spawn_run(self.clone().run(subagent, cancel))
    }

    async fn run_to_end(&self, subagent: &Session, cancel: CancellationToken) -> Result<()> {
        let (agent, session_config) = from_foreground_subagent_session(
            self.session_manager.clone(),
            subagent,
            self.use_login_shell_path,
        )
        .await?;
        let mut events = agent
            .stream_state_machine_session(session_config, cancel)
            .await?;
        while let Some(event) = events.next().await {
            event?;
        }
        Ok(())
    }

    async fn run(self, subagent: Session, cancel: CancellationToken) -> SubagentOutcome {
        let run_result = self.run_to_end(&subagent, cancel.clone()).await;
        let stopped = cancel.is_cancelled();
        let subagent = match self.session_manager.get_session(&subagent.id, true).await {
            Ok(subagent) => subagent,
            Err(error) => return SubagentOutcome::Failed(error.to_string()),
        };
        let messages = subagent.conversation.as_ref().map(Conversation::messages);
        if let Some(output) =
            messages.and_then(|messages| FinalOutputTool::successful_output(messages))
        {
            return SubagentOutcome::Completed(output);
        }
        if stopped {
            return SubagentOutcome::Cancelled;
        }
        if let Err(error) = run_result {
            return SubagentOutcome::Failed(error.to_string());
        }
        if let Some(error) = subagent.conversation.as_ref().and_then(trailing_error) {
            return SubagentOutcome::Failed(format!("{error:?}"));
        }
        SubagentOutcome::Failed(failure_reason(
            messages.map(Vec::as_slice).unwrap_or_default(),
        ))
    }
}

fn spawn_run(
    run: impl Future<Output = SubagentOutcome> + Send + 'static,
) -> BoxFuture<'static, SubagentOutcome> {
    let task = AbortOnDropHandle::new(tokio::spawn(run.in_current_span()));
    Box::pin(async move {
        task.await
            .unwrap_or_else(|error| SubagentOutcome::Failed(error.to_string()))
    })
}

fn failure_reason(messages: &[Message]) -> String {
    let Some(last) = messages.last() else {
        return "stopped without final output".to_string();
    };
    let last_text = last.as_concat_text();
    if last_text == MAX_TURNS_MESSAGE {
        let last_response = messages
            .iter()
            .rev()
            .filter(|message| message.role == Role::Assistant)
            .map(Message::as_concat_text)
            .find(|text| !text.is_empty() && text != MAX_TURNS_MESSAGE);
        return match last_response {
            Some(text) => format!("max turns reached; last response: {text}"),
            None => "max turns reached".to_string(),
        };
    }
    if last_text.is_empty() {
        "stopped without final output".to_string()
    } else {
        last_text
    }
}

pub const SUBAGENT_TOOL_REQUEST_TYPE: &str = "subagent_tool_request";

async fn build_subagent_prompt(
    agent: &Agent,
    max_turns: usize,
    session_id: &str,
) -> Result<String> {
    let mut tool_names: Vec<_> = agent
        .list_tools(session_id, None)
        .await
        .into_iter()
        .filter(super::reply_parts::is_tool_visible_to_model)
        .map(|t| t.name.to_string())
        .collect();
    tool_names.sort_unstable();
    render_template(
        "subagent_system.md",
        &SubagentPromptContext {
            max_turns,
            tool_count: tool_names.len(),
            available_tools: tool_names.join(", "),
        },
    )
    .map_err(|e| anyhow!("Failed to render subagent system prompt: {}", e))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{failure_reason, spawn_run};
    use crate::agents::state_machine::MAX_TURNS_MESSAGE;
    use crate::conversation::message::Message;
    use goose_agent::subagent::SubagentOutcome;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn dropping_a_started_run_aborts_its_task() {
        let (alive, aborted) = oneshot::channel::<()>();
        let run = spawn_run(async move {
            let _alive = alive;
            std::future::pending::<SubagentOutcome>().await
        });

        drop(run);

        let ended = tokio::time::timeout(Duration::from_secs(5), aborted).await;
        assert!(ended.expect("the task was not aborted").is_err());
    }

    #[test]
    fn failure_reason_describes_how_the_subagent_stopped() {
        let max_turns = Message::assistant().with_text(MAX_TURNS_MESSAGE);
        let cases = [
            (vec![], "stopped without final output".to_string()),
            (
                vec![Message::assistant().with_text("")],
                "stopped without final output".to_string(),
            ),
            (
                vec![Message::assistant().with_text("I gave up")],
                "I gave up".to_string(),
            ),
            (
                vec![Message::user().with_text("go"), max_turns.clone()],
                "max turns reached".to_string(),
            ),
            (
                vec![
                    Message::assistant().with_text("halfway there"),
                    Message::user().with_text("tool result"),
                    max_turns,
                ],
                "max turns reached; last response: halfway there".to_string(),
            ),
        ];
        for (messages, expected) in cases {
            assert_eq!(failure_reason(&messages), expected);
        }
    }
}
