use std::collections::HashMap;

use anyhow::{anyhow, Result};

use crate::context_mgmt::compact_messages;
use crate::conversation::message::{Message, SystemNotificationType};
use crate::slash_commands::{recipe_slash_command, skill_slash_command};

use super::Agent;

pub const COMPACT_TRIGGERS: &[&str] =
    &["/compact", "Please compact this conversation", "/summarize"];

pub struct CommandDef {
    pub name: &'static str,
    pub description: &'static str,
}

static COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "prompts",
        description: "List available prompts, optionally filtered by extension",
    },
    CommandDef {
        name: "prompt",
        description: "Execute a prompt or show its info with --info",
    },
    CommandDef {
        name: "compact",
        description: "Compact the conversation history",
    },
    CommandDef {
        name: "clear",
        description: "Clear the conversation history",
    },
    CommandDef {
        name: "skills",
        description: "List installed skills and other available sources",
    },
    CommandDef {
        name: "doctor",
        description: "Check that your Goose setup is working",
    },
    CommandDef {
        name: "goal",
        description: "Set a goal the agent must satisfy before finishing, or clear with /goal off",
    },
    CommandDef {
        name: "grind",
        description:
            "Set a goal the agent pursues relentlessly until max_turns, or clear with /grind off",
    },
];

pub struct ParsedSlashCommand<'a> {
    pub command: &'a str,
    pub params_str: &'a str,
}

pub fn parse_slash_command(message_text: &str) -> Option<ParsedSlashCommand<'_>> {
    let mut trimmed = message_text.trim();

    if COMPACT_TRIGGERS.contains(&trimmed) {
        trimmed = COMPACT_TRIGGERS[0];
    }

    if !trimmed.starts_with('/') {
        return None;
    }

    let command_str = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let (command, params_str) = command_str
        .split_once(' ')
        .map(|(cmd, p)| (cmd, p.trim()))
        .unwrap_or((command_str, ""));

    Some(ParsedSlashCommand {
        command,
        params_str,
    })
}

pub fn list_commands() -> &'static [CommandDef] {
    COMMANDS
}

impl Agent {
    pub async fn execute_command(
        &self,
        message_text: &str,
        session_id: &str,
    ) -> Result<Option<Message>> {
        let Some(parsed) = parse_slash_command(message_text) else {
            return Ok(None);
        };

        let command = parsed.command;
        let params_str = parsed.params_str;

        let params: Vec<&str> = if params_str.is_empty() {
            vec![]
        } else {
            params_str.split_whitespace().collect()
        };

        match command {
            "prompts" => self.handle_prompts_command(&params, session_id).await,
            "prompt" => self.handle_prompt_command(&params, session_id).await,
            "compact" => self.handle_compact_command(session_id).await,
            "clear" => self.handle_clear_command(session_id).await,
            "skills" => self.handle_skills_command(session_id).await,
            "doctor" => Ok(Some(crate::doctor::run(self, session_id).await?)),
            "goal" => self.handle_goal_command(params_str).await,
            "grind" => self.handle_grind_command(params_str).await,
            _ => {
                if let Some(message) = self
                    .handle_recipe_command(command, params_str, session_id)
                    .await?
                {
                    #[cfg(feature = "telemetry")]
                    crate::posthog::emit_custom_slash_command_used();
                    return Ok(Some(message));
                }

                self.handle_skill_command(command, params_str, session_id)
                    .await
            }
        }
    }

    async fn handle_compact_command(&self, session_id: &str) -> Result<Option<Message>> {
        let manager = self.config.session_manager.clone();
        let session = manager.get_session(session_id, true).await?;
        let conversation = session
            .conversation
            .ok_or_else(|| anyhow!("Session has no conversation"))?;

        let (compacted_conversation, usage) = compact_messages(
            self.provider().await?.as_ref(),
            session_id,
            &conversation,
            true, // is_manual_compact
        )
        .await?;

        manager
            .replace_conversation(session_id, &compacted_conversation)
            .await?;

        self.update_session_metrics(session_id, session.schedule_id, &usage, true)
            .await?;

        Ok(Some(Message::assistant().with_system_notification(
            SystemNotificationType::InlineMessage,
            "Compaction complete",
        )))
    }

    async fn handle_clear_command(&self, session_id: &str) -> Result<Option<Message>> {
        use crate::conversation::Conversation;

        let manager = self.config.session_manager.clone();
        manager
            .replace_conversation(session_id, &Conversation::default())
            .await?;

        manager
            .update(session_id)
            .total_tokens(Some(0))
            .input_tokens(Some(0))
            .output_tokens(Some(0))
            .apply()
            .await?;

        Ok(Some(Message::assistant().with_system_notification(
            SystemNotificationType::InlineMessage,
            "Conversation cleared",
        )))
    }

    async fn handle_skills_command(&self, session_id: &str) -> Result<Option<Message>> {
        let working_dir = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
            .ok()
            .map(|s| s.working_dir);
        let output = skill_slash_command::format_installed_skills(working_dir.as_deref());
        Ok(Some(Message::assistant().with_text(output)))
    }

    async fn handle_prompts_command(
        &self,
        params: &[&str],
        session_id: &str,
    ) -> Result<Option<Message>> {
        let extension_filter = params.first().map(|s| s.to_string());

        let prompts = self.list_extension_prompts(session_id).await;

        if let Some(filter) = &extension_filter {
            if !prompts.contains_key(filter) {
                let error_msg = format!("Extension '{}' not found", filter);
                return Ok(Some(Message::assistant().with_text(error_msg)));
            }
        }

        let filtered_prompts: HashMap<String, Vec<String>> = prompts
            .into_iter()
            .filter(|(ext, _)| extension_filter.as_ref().is_none_or(|f| f == ext))
            .map(|(extension, prompt_list)| {
                let names = prompt_list.into_iter().map(|p| p.name).collect();
                (extension, names)
            })
            .collect();

        let mut output = String::new();
        if filtered_prompts.is_empty() {
            output.push_str("No prompts available.\n");
        } else {
            output.push_str("Available prompts:\n\n");
            for (extension, prompt_names) in filtered_prompts {
                output.push_str(&format!("**{}**:\n", extension));
                for name in prompt_names {
                    output.push_str(&format!("  - {}\n", name));
                }
                output.push('\n');
            }
        }

        Ok(Some(Message::assistant().with_text(output)))
    }

    async fn handle_prompt_command(
        &self,
        params: &[&str],
        session_id: &str,
    ) -> Result<Option<Message>> {
        if params.is_empty() {
            return Ok(Some(
                Message::assistant().with_text("Prompt name argument is required"),
            ));
        }

        let prompt_name = params[0].to_string();
        let is_info = params.get(1).map(|s| *s == "--info").unwrap_or(false);

        if is_info {
            let prompts = self.list_extension_prompts(session_id).await;
            let mut prompt_info = None;

            for (extension, prompt_list) in prompts {
                if let Some(prompt) = prompt_list.iter().find(|p| p.name == prompt_name) {
                    let mut output = format!("**Prompt: {}**\n\n", prompt.name);
                    if let Some(desc) = &prompt.description {
                        output.push_str(&format!("Description: {}\n\n", desc));
                    }
                    output.push_str(&format!("Extension: {}\n\n", extension));

                    if let Some(args) = &prompt.arguments {
                        output.push_str("Arguments:\n");
                        for arg in args {
                            output.push_str(&format!("  - {}", arg.name));
                            if let Some(desc) = &arg.description {
                                output.push_str(&format!(": {}", desc));
                            }
                            output.push('\n');
                        }
                    }

                    prompt_info = Some(output);
                    break;
                }
            }

            return Ok(Some(Message::assistant().with_text(
                prompt_info.unwrap_or_else(|| format!("Prompt '{}' not found", prompt_name)),
            )));
        }

        let mut arguments = HashMap::new();
        for param in params.iter().skip(1) {
            if let Some((key, value)) = param.split_once('=') {
                let value = value.trim_matches('"');
                arguments.insert(key.to_string(), value.to_string());
            }
        }

        let arguments_value = serde_json::to_value(arguments)
            .map_err(|e| anyhow!("Failed to serialize arguments: {}", e))?;

        match self
            .get_prompt(session_id, &prompt_name, arguments_value)
            .await
        {
            Ok(prompt_result) => {
                for (i, prompt_message) in prompt_result.messages.into_iter().enumerate() {
                    let msg = Message::from(prompt_message);

                    let expected_role = if i % 2 == 0 {
                        rmcp::model::Role::User
                    } else {
                        rmcp::model::Role::Assistant
                    };

                    if msg.role != expected_role {
                        let error_msg = format!(
                            "Expected {:?} message at position {}, but found {:?}",
                            expected_role, i, msg.role
                        );
                        return Ok(Some(Message::assistant().with_text(error_msg)));
                    }

                    self.config
                        .session_manager
                        .clone()
                        .add_message(session_id, &msg)
                        .await?;
                }

                let last_message = self
                    .config
                    .session_manager
                    .get_session(session_id, true)
                    .await?
                    .conversation
                    .ok_or_else(|| anyhow!("No conversation found"))?
                    .messages()
                    .last()
                    .cloned()
                    .ok_or_else(|| anyhow!("No messages in conversation"))?;

                Ok(Some(last_message))
            }
            Err(e) => Ok(Some(
                Message::assistant().with_text(format!("Error getting prompt: {}", e)),
            )),
        }
    }

    async fn handle_recipe_command(
        &self,
        command: &str,
        params_str: &str,
        _session_id: &str,
    ) -> Result<Option<Message>> {
        match recipe_slash_command::resolve_command(command, params_str) {
            Ok(None) => Ok(None),
            Ok(Some((response, prompt))) => {
                self.apply_recipe_components(response, true).await;
                Ok(Some(Message::user().with_text(prompt)))
            }
            Err(text) => Ok(Some(Message::assistant().with_text(text))),
        }
    }

    async fn handle_skill_command(
        &self,
        command: &str,
        params_str: &str,
        session_id: &str,
    ) -> Result<Option<Message>> {
        let working_dir = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
            .ok()
            .map(|session| session.working_dir);

        match skill_slash_command::resolve_command(command, params_str, working_dir.as_deref()) {
            Ok(None) => Ok(None),
            Ok(Some(prompt)) => Ok(Some(Message::user().with_text(prompt))),
            Err(text) => Ok(Some(Message::assistant().with_text(text))),
        }
    }

    async fn handle_goal_command(&self, params_str: &str) -> Result<Option<Message>> {
        if params_str.is_empty() {
            let current = self.get_goal().await;
            let text = match current {
                Some(goal) => format!("Current goal: {goal}"),
                None => "No goal set. Use `/goal <description>` to set one.".to_string(),
            };
            return Ok(Some(Message::assistant().with_text(text)));
        }

        if params_str == "off" || params_str == "clear" || params_str == "none" {
            self.set_goal(None).await;
            return Ok(Some(
                Message::assistant().with_text("Goal cleared. The agent will finish normally."),
            ));
        }

        let goal = params_str.to_string();
        self.set_goal(Some(goal.clone())).await;
        Ok(Some(Message::assistant().with_text(format!(
            "Goal set. The agent will verify this goal is met before finishing:\n\n> {goal}"
        ))))
    }

    async fn handle_grind_command(&self, params_str: &str) -> Result<Option<Message>> {
        if params_str.is_empty() {
            let current = self.get_grind().await;
            let text = match current {
                Some(goal) => format!("Current grind goal: {goal}"),
                None => "No grind goal set. Use `/grind <description>` to set one.".to_string(),
            };
            return Ok(Some(Message::assistant().with_text(text)));
        }

        if params_str == "off" || params_str == "clear" {
            self.set_grind(None).await;
            return Ok(Some(
                Message::assistant().with_text("Grind cleared. The agent will finish normally."),
            ));
        }

        let goal = params_str.to_string();
        self.set_grind(Some(goal.clone())).await;
        Ok(Some(Message::assistant().with_text(format!(
            "Grind goal set. The agent will keep working until max_turns is reached:\n\n> {goal}"
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_slash_command_splits_on_literal_space() {
        let parsed = parse_slash_command("/speckit.plan hello world").unwrap();

        assert_eq!(parsed.command, "speckit.plan");
        assert_eq!(parsed.params_str, "hello world");
    }

    #[test]
    fn parse_slash_command_does_not_split_on_tab_or_newline() {
        let parsed = parse_slash_command("/speckit.plan\thello").unwrap();
        assert_eq!(parsed.command, "speckit.plan\thello");
        assert_eq!(parsed.params_str, "");

        let parsed = parse_slash_command("/speckit.plan\nhello").unwrap();
        assert_eq!(parsed.command, "speckit.plan\nhello");
        assert_eq!(parsed.params_str, "");
    }
}
