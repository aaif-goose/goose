use std::path::Path;

use crate::slash_commands::{recipe_slash_command, skill_slash_command};

pub fn slash_commands_enabled() -> bool {
    crate::config::Config::global()
        .get_param::<bool>("GOOSE_SLASH_COMMANDS_ENABLED")
        .unwrap_or(true)
}

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
    CommandDef {
        name: "status",
        description: "Show session status: model, provider, mode, and token usage",
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

pub fn context_management_unsupported_message(command: &str, provider: &str) -> String {
    format!(
        "/{command} is not available for provider '{provider}' because it manages its own conversation context"
    )
}

pub fn is_known_slash_command(message_text: &str, working_dir: Option<&Path>) -> bool {
    let Some(parsed) = parse_slash_command(message_text) else {
        return false;
    };

    COMMANDS
        .iter()
        .any(|command| command.name == parsed.command)
        || recipe_slash_command::get_recipe_for_command(parsed.command).is_some()
        || skill_slash_command::list_commands(working_dir)
            .into_iter()
            .any(|command| command.name.eq_ignore_ascii_case(parsed.command))
}

fn is_clear_goal_param(params_str: &str) -> bool {
    matches!(params_str, "off" | "clear" | "none")
}

/// Whether a slash command should kick off an agent turn instead of just
/// returning a confirmation. Setting a `/goal` or `/grind` (with a description,
/// not the query or `off` forms) makes the agent start pursuing it immediately.
pub fn command_starts_turn(message_text: &str) -> bool {
    let Some(parsed) = parse_slash_command(message_text) else {
        return false;
    };
    matches!(parsed.command, "goal" | "grind")
        && !parsed.params_str.is_empty()
        && !is_clear_goal_param(parsed.params_str)
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

    #[test]
    fn command_starts_turn_only_for_goal_and_grind_with_description() {
        assert!(command_starts_turn("/goal make all tests pass"));
        assert!(command_starts_turn("/grind keep refactoring"));

        // Query and clear forms must not start a turn.
        assert!(!command_starts_turn("/goal"));
        assert!(!command_starts_turn("/goal off"));
        assert!(!command_starts_turn("/goal clear"));
        assert!(!command_starts_turn("/goal none"));
        assert!(!command_starts_turn("/grind"));
        assert!(!command_starts_turn("/grind off"));

        // Other commands and plain prompts never start a turn here.
        assert!(!command_starts_turn("/compact"));
        assert!(!command_starts_turn("just a normal message"));
    }

    #[test]
    fn status_is_registered_as_a_builtin_command() {
        assert!(list_commands()
            .iter()
            .any(|command| command.name == "status"));
    }
}
