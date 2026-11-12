use anyhow::Result;
use async_trait::async_trait;

use crate::agents::state_machine::{GooseEffect, Operation};
use crate::conversation::message::MessageContent;
use crate::conversation::Conversation;
use crate::hints::load_hints::SubdirectoryHintTracker;
use crate::session::Session;

pub struct SubdirectoryHintsOperation;

#[async_trait]
impl Operation<Session, GooseEffect> for SubdirectoryHintsOperation {
    fn name(&self) -> &'static str {
        "subdirectory_hints"
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let mut hints = SubdirectoryHintTracker::new();
        for message in conversation
            .messages()
            .iter()
            .filter(|message| message.is_agent_visible())
        {
            for content in &message.content {
                if let MessageContent::ToolRequest(request) = content {
                    if let Ok(tool_call) = &request.tool_call {
                        hints.record_tool_arguments(&tool_call.arguments, &session.working_dir);
                    }
                }
            }
        }
        Ok(hints.load_new_hints(&session.working_dir))
    }
}
