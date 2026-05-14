use std::collections::HashMap;

use anyhow::{anyhow, Result};

use crate::context_mgmt::compact_messages;
use crate::conversation::message::{Message, SystemNotificationType};
use crate::recipe::build_recipe::build_recipe_from_template;
use crate::recipe::{RecipeParameter, RecipeParameterRequirement};
use crate::skills::loaded_skill_context_with_args;

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

pub struct ParsedSlashCommand<'a> {
    pub command: &'a str,
    pub params_str: &'a str,
}

fn parse_recipe_args(
    params_str: &str,
    required: &[&RecipeParameter],
    optional: &[&RecipeParameter],
) -> Result<Vec<(String, String)>> {
    use std::collections::HashSet;

    let tokens = crate::utils::split_command_args(params_str)?;
    let known_keys: HashSet<&str> = required
        .iter()
        .chain(optional.iter())
        .map(|p| p.key.as_str())
        .collect();

    let mut result = Vec::new();
    let mut required_idx = 0;
    let mut i = 0;

    while i < tokens.len() {
        let token = &tokens[i];
        if let Some(flag) = token.strip_prefix("--") {
            if !known_keys.contains(flag) {
                return Err(anyhow!("Unknown parameter: --{}", flag));
            }
            let value = tokens
                .get(i + 1)
                .ok_or_else(|| anyhow!("Missing value for --{}", flag))?;
            if value.starts_with("--") {
                return Err(anyhow!("Missing value for --{}", flag));
            }
            result.push((flag.to_string(), value.clone()));
            i += 2;
        } else {
            if required_idx >= required.len() {
                return Err(anyhow!("Unexpected positional argument: {}", token));
            }
            result.push((required[required_idx].key.clone(), token.clone()));
            required_idx += 1;
            i += 1;
        }
    }

    Ok(result)
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
            _ => {
                if let Some(message) = self
                    .handle_recipe_command(command, params_str, session_id)
                    .await?
                {
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
        use crate::skills::list_installed_skills;
        use goose_sdk::custom_requests::SourceType;

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
            .filter(|s| matches!(s.source_type, SourceType::Skill | SourceType::BuiltinSkill))
            .collect();

        let mut output = String::new();
        if skills.is_empty() {
            output.push_str("No skills installed.\n\n");
            output.push_str("Skills are loaded from SKILL.md files in:\n");
            output.push_str("  - ~/.agents/skills/ (global)\n");
            output.push_str("  - ~/.agents/plugins/*/skills/ (installed plugins)\n");
            output.push_str("  - .agents/skills/ (in current project)\n");
        } else {
            output.push_str(&format!("**Installed skills ({}):**\n\n", skills.len()));
            for skill in &skills {
                let kind_label = if skill.source_type == SourceType::BuiltinSkill {
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

        let empty_params: Vec<RecipeParameter> = Vec::new();
        let all_params = validation_result
            .parameters
            .as_ref()
            .unwrap_or(&empty_params);
        let required: Vec<&RecipeParameter> = all_params
            .iter()
            .filter(|p| {
                matches!(
                    p.requirement,
                    RecipeParameterRequirement::Required | RecipeParameterRequirement::UserPrompt
                )
            })
            .collect();
        let optional: Vec<&RecipeParameter> = all_params
            .iter()
            .filter(|p| matches!(p.requirement, RecipeParameterRequirement::Optional))
            .collect();

        let param_values: Vec<(String, String)> = if params_str.is_empty() {
            vec![]
        } else if required.len() == 1 && optional.is_empty() {
            vec![(required[0].key.clone(), params_str.to_string())]
        } else {
            parse_recipe_args(params_str, &required, &optional)?
        };

        let param_values_len = param_values.len();

        let recipe = match build_recipe_from_template(
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

        let skill = crate::skills::list_installed_skills(working_dir.as_deref())
            .into_iter()
            .find(|skill| skill.name.eq_ignore_ascii_case(command));

        let Some(skill) = skill else {
            return Ok(None);
        };

        let prompt =
            loaded_skill_context_with_args(&skill, (!params_str.is_empty()).then_some(params_str))?;

        Ok(Some(Message::user().with_text(prompt)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::RecipeParameterInputType;

    fn required_param(key: &str) -> RecipeParameter {
        RecipeParameter {
            key: key.to_string(),
            input_type: RecipeParameterInputType::String,
            requirement: RecipeParameterRequirement::Required,
            description: format!("{key} parameter"),
            default: None,
            options: None,
        }
    }

    fn optional_param(key: &str) -> RecipeParameter {
        RecipeParameter {
            key: key.to_string(),
            input_type: RecipeParameterInputType::String,
            requirement: RecipeParameterRequirement::Optional,
            description: format!("{key} parameter"),
            default: Some("default".to_string()),
            options: None,
        }
    }

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

    #[test]
    fn parse_recipe_args_maps_required_positionals_and_optional_flags() {
        let component = required_param("component");
        let from = required_param("from");
        let to = optional_param("to");
        let scope = optional_param("scope");
        let required = vec![&component, &from];
        let optional = vec![&to, &scope];

        let parsed = parse_recipe_args(
            r#""Button Group" old-lib --to new-lib"#,
            &required,
            &optional,
        )
        .unwrap();

        assert_eq!(
            parsed,
            vec![
                ("component".to_string(), "Button Group".to_string()),
                ("from".to_string(), "old-lib".to_string()),
                ("to".to_string(), "new-lib".to_string()),
            ]
        );
    }

    #[test]
    fn parse_recipe_args_allows_values_containing_equals() {
        let component = required_param("component");
        let note = optional_param("note");
        let required = vec![&component];
        let optional = vec![&note];

        let parsed = parse_recipe_args(r#"Button --note "a=b""#, &required, &optional).unwrap();

        assert_eq!(
            parsed,
            vec![
                ("component".to_string(), "Button".to_string()),
                ("note".to_string(), "a=b".to_string()),
            ]
        );
    }

    #[test]
    fn parse_recipe_args_errors_when_flag_value_is_another_flag() {
        let component = required_param("component");
        let from = required_param("from");
        let to = optional_param("to");
        let scope = optional_param("scope");
        let required = vec![&component, &from];
        let optional = vec![&to, &scope];

        let err =
            parse_recipe_args("Button old-lib --to --scope all", &required, &optional).unwrap_err();

        assert!(err.to_string().contains("Missing value for --to"));
    }

    #[test]
    fn parse_recipe_args_errors_on_extra_positionals() {
        let component = required_param("component");
        let from = required_param("from");
        let required = vec![&component, &from];

        let err = parse_recipe_args("Button old-lib extra", &required, &[]).unwrap_err();

        assert!(err
            .to_string()
            .contains("Unexpected positional argument: extra"));
    }

    #[test]
    fn parse_recipe_args_errors_on_unknown_flag() {
        let component = required_param("component");
        let required = vec![&component];

        let err = parse_recipe_args("Button --unknown value", &required, &[]).unwrap_err();

        assert!(err.to_string().contains("Unknown parameter: --unknown"));
    }
}
