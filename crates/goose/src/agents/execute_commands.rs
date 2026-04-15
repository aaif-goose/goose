use std::collections::HashMap;

use anyhow::{anyhow, Result};

use crate::context_mgmt::compact_messages;
use crate::conversation::message::{Message, SystemNotificationType};
use crate::recipe::build_recipe::build_recipe_from_template_with_positional_params;

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
];

pub fn list_commands() -> &'static [CommandDef] {
    COMMANDS
}

impl Agent {
    pub async fn execute_command(
        &self,
        message_text: &str,
        session_id: &str,
    ) -> Result<Option<Message>> {
        let mut trimmed = message_text.trim().to_string();

        if COMPACT_TRIGGERS.contains(&trimmed.as_str()) {
            trimmed = COMPACT_TRIGGERS[0].to_string();
        }

        if !trimmed.starts_with('/') {
            return Ok(None);
        }

        let command_str = trimmed.strip_prefix('/').unwrap_or(&trimmed);
        let (command, params_str) = command_str
            .split_once(' ')
            .map(|(cmd, p)| (cmd, p.trim()))
            .unwrap_or((command_str, ""));

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
            _ => {
                self.handle_recipe_command(command, params_str, session_id)
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
        use super::platform_extensions::skills::list_installed_skills;
        use super::platform_extensions::SourceKind;

        let working_dir = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
            .ok()
            .map(|s| s.working_dir);
        let sources = list_installed_skills(working_dir.as_deref());
        let skills: Vec<_> = sources
            .iter()
            .filter(|s| matches!(s.kind, SourceKind::Skill | SourceKind::BuiltinSkill))
            .collect();

        let mut output = String::new();
        if skills.is_empty() {
            output.push_str("No skills installed.\n\n");
            output.push_str("Skills are loaded from SKILL.md files in:\n");
            output.push_str("  - ~/.agents/skills/ (global)\n");
            output.push_str("  - .agents/skills/ (in current project)\n");
        } else {
            output.push_str(&format!("**Installed skills ({}):**\n\n", skills.len()));
            for skill in &skills {
                let kind_label = if skill.kind == SourceKind::BuiltinSkill {
                    " *(builtin)*"
                } else {
                    ""
                };
                output.push_str(&format!(
                    "- **{}**{}: {}\n",
                    skill.name, kind_label, skill.description
                ));
            }
        }

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
        let full_command = format!("/{}", command);
        let recipe_path = match crate::slash_commands::get_recipe_for_command(&full_command) {
            Some(path) => path,
            None => return Ok(None),
        };

        if !recipe_path.exists() {
            return Ok(None);
        }

        let recipe_content = std::fs::read_to_string(&recipe_path)
            .map_err(|e| anyhow!("Failed to read recipe file: {}", e))?;

        let recipe_dir = recipe_path
            .parent()
            .ok_or_else(|| anyhow!("Recipe path has no parent directory"))?;

        let recipe_dir_str = recipe_dir.display().to_string();
        let validation_result =
            crate::recipe::validate_recipe::validate_recipe_template_from_content(
                &recipe_content,
                Some(recipe_dir_str),
            )
            .map_err(|e| anyhow!("Failed to parse recipe: {}", e))?;

        let param_values: Vec<String> = if params_str.is_empty() {
            vec![]
        } else {
            let params_without_default = validation_result
                .parameters
                .as_ref()
                .map(|params| params.iter().filter(|p| p.default.is_none()).count())
                .unwrap_or(0);

            if params_without_default <= 1 {
                vec![params_str.to_string()]
            } else {
                let param_names: Vec<String> = validation_result
                    .parameters
                    .as_ref()
                    .map(|params| {
                        params
                            .iter()
                            .filter(|p| p.default.is_none())
                            .map(|p| p.key.clone())
                            .collect()
                    })
                    .unwrap_or_default();

                let error_message = format!(
                    "The /{} recipe requires {} parameters: {}.\n\n\
                    Slash command recipes only support 1 parameter.\n\n\
                    **To use this recipe:**\n\
                    • **CLI:** `goose run --recipe {} {}`\n\
                    • **Desktop:** Launch from the recipes sidebar to fill in parameters",
                    command,
                    params_without_default,
                    param_names
                        .iter()
                        .map(|name| format!("**{}**", name))
                        .collect::<Vec<_>>()
                        .join(", "),
                    command,
                    param_names
                        .iter()
                        .map(|name| format!("--params {}=\"...\"", name))
                        .collect::<Vec<_>>()
                        .join(" ")
                );

                return Err(anyhow!(error_message));
            }
        };

        let param_values_len = param_values.len();

        let recipe = match build_recipe_from_template_with_positional_params(
            recipe_content,
            recipe_dir,
            param_values,
            None::<fn(&str, &str) -> Result<String>>,
        ) {
            Ok(recipe) => recipe,
            Err(crate::recipe::build_recipe::RecipeError::MissingParams { parameters }) => {
                return Ok(Some(Message::assistant().with_text(format!(
                    "Recipe requires {} parameter(s): {}. Provided: {}",
                    parameters.len(),
                    parameters.join(", "),
                    param_values_len
                ))));
            }
            Err(e) => return Err(anyhow!("Failed to build recipe: {}", e)),
        };

        self.apply_recipe_components(recipe.response.clone(), true)
            .await;

        let prompt = [recipe.instructions.as_deref(), recipe.prompt.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(Some(Message::user().with_text(prompt)))
    }
}
