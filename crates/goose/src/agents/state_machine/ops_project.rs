//! Adds the selected project's instructions to inference prompts.

use anyhow::Result;
use async_trait::async_trait;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::Operation;
use crate::conversation::Conversation;
use crate::session::Session;

pub struct ProjectOperation;

#[async_trait]
impl Operation<Session, GooseEffect> for ProjectOperation {
    fn name(&self) -> &'static str {
        "project"
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let Some(project_id) = session.project_id.as_deref() else {
            return Ok(Vec::new());
        };
        let Ok(entry) = crate::sources::read_project(project_id) else {
            return Ok(Vec::new());
        };
        let mut parts = vec![format!("# Project: {}", entry.name)];
        if !entry.description.is_empty() {
            parts.push(entry.description);
        }
        if !entry.content.is_empty() {
            parts.push(entry.content);
        }
        Ok(vec![("project".to_string(), parts.join("\n\n"))])
    }
}
