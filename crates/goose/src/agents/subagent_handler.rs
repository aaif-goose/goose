use crate::{
    agents::{
        final_output_tool::FinalOutputTool,
        state_machine::{forwarded_from_subagent, trailing_error, MAX_TURNS_MESSAGE},
        Agent, AgentEvent, SessionConfig,
    },
    conversation::{
        message::{ActionRequiredData, Message, MessageContent},
        Conversation,
    },
    prompt_template::render_template,
    session::extension_data::{EnabledExtensionsState, ExtensionState},
    session::{Session, SessionManager, SessionType},
};
use anyhow::{anyhow, Result};
use futures::future::BoxFuture;
use futures::StreamExt;
use rmcp::model::Role;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Serialize)]
pub struct SubagentPromptContext {
    pub max_turns: usize,
    pub tool_count: usize,
    pub available_tools: String,
}

pub(crate) async fn prepare_foreground_subagent(
    agent: &Agent,
    session: &Session,
) -> Result<SessionConfig> {
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
    session
        .extension_data
        .get_extension_state(
            EnabledExtensionsState::EXTENSION_NAME,
            EnabledExtensionsState::VERSION,
        )
        .ok_or_else(|| anyhow!("Subagent {session_id} has no saved extension selection"))?;

    agent
        .switch_provider(session_id, provider_name, model_config.clone())
        .await?;

    let subagent_prompt = build_subagent_prompt(agent, max_turns, session_id).await?;
    agent
        .config
        .session_manager
        .update(session_id)
        .system_prompt_override(Some(subagent_prompt))
        .apply()
        .await?;
    Ok(SessionConfig {
        id: session_id.to_string(),
        schedule_id: None,
        max_turns: Some(max_turns as u32),
    })
}

pub(crate) enum SubagentOutcome {
    Completed(String),
    Failed(String),
    Cancelled,
}

pub(crate) enum SubagentStart {
    HasOutcome(SubagentOutcome),
    Started {
        task: Option<String>,
        run: BoxFuture<'static, SubagentOutcome>,
    },
}

#[derive(Clone)]
pub(crate) struct ForegroundSubagentRunner {
    agent: Arc<Agent>,
}

impl ForegroundSubagentRunner {
    pub(crate) fn new(agent: Arc<Agent>) -> Self {
        Self { agent }
    }

    fn session_manager(&self) -> &SessionManager {
        &self.agent.config.session_manager
    }

    pub(crate) async fn start(
        &self,
        parent_id: &str,
        subagent_id: &str,
        cancel: CancellationToken,
        forward: mpsc::UnboundedSender<Message>,
    ) -> SubagentStart {
        let subagent = match self.session_manager().get_session(subagent_id, true).await {
            Ok(subagent) => subagent,
            Err(error) => {
                return SubagentStart::HasOutcome(SubagentOutcome::Failed(error.to_string()))
            }
        };
        if subagent.session_type != SessionType::SubAgent
            || subagent.parent_session_id.as_deref() != Some(parent_id)
        {
            return SubagentStart::HasOutcome(SubagentOutcome::Failed(
                "it does not belong to this session".to_string(),
            ));
        }
        if let Some(output) = subagent
            .conversation
            .as_ref()
            .and_then(|conversation| FinalOutputTool::successful_output(conversation.messages()))
        {
            return SubagentStart::HasOutcome(SubagentOutcome::Completed(output));
        }
        SubagentStart::Started {
            task: subagent
                .recipe
                .as_ref()
                .and_then(|recipe| recipe.prompt.clone()),
            run: Box::pin(self.clone().run(subagent, cancel, forward)),
        }
    }

    async fn run_to_end(
        &self,
        subagent: &Session,
        cancel: CancellationToken,
        forward: mpsc::UnboundedSender<Message>,
    ) -> Result<()> {
        let session_config = prepare_foreground_subagent(&self.agent, subagent).await?;
        let mut events = self
            .agent
            .stream_subagent_turn(session_config, cancel)
            .await?;
        while let Some(event) = events.next().await {
            if let AgentEvent::Message(message) = event? {
                if asks_for_a_decision(&message) {
                    let _ = forward.send(forwarded_from_subagent(message, &subagent.id));
                }
            }
        }
        Ok(())
    }

    async fn run(
        self,
        subagent: Session,
        cancel: CancellationToken,
        forward: mpsc::UnboundedSender<Message>,
    ) -> SubagentOutcome {
        let run_result = self.run_to_end(&subagent, cancel.clone(), forward).await;
        self.agent.release_session(&subagent.id).await;
        let stopped = cancel.is_cancelled();
        let subagent = match self.session_manager().get_session(&subagent.id, true).await {
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

fn asks_for_a_decision(message: &Message) -> bool {
    message.content.iter().any(|content| {
        matches!(
            content,
            MessageContent::ActionRequired(action)
                if matches!(action.data, ActionRequiredData::ToolConfirmation { .. })
        )
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
        .await?
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
    use super::failure_reason;
    use crate::agents::state_machine::MAX_TURNS_MESSAGE;
    use crate::conversation::message::Message;

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
