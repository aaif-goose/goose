use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell as ClapShell};
use goose::builtin_extension::register_builtin_extensions;
use goose::config::{Config, GooseMode};
#[cfg(feature = "telemetry")]
use goose::posthog::get_telemetry_choice;
use goose::recipe::Recipe;
use goose_mcp::mcp_server_runner::{serve, McpCommand};
use goose_mcp::{AutoVisualiserRouter, ComputerControllerServer, MemoryServer, TutorialServer};

#[cfg(feature = "telemetry")]
use crate::commands::configure::configure_telemetry_consent_dialog;
use crate::commands::configure::handle_configure;
use crate::commands::info::handle_info;
use crate::commands::project::{handle_project_default, handle_projects_interactive};
use crate::commands::recipe::{handle_deeplink, handle_list, handle_open, handle_validate};
use crate::commands::term::{
    handle_term_info, handle_term_init, handle_term_log, handle_term_run, Shell,
};

use crate::commands::schedule::{
    handle_schedule_add, handle_schedule_cron_help, handle_schedule_list, handle_schedule_remove,
    handle_schedule_run_now, handle_schedule_services_status, handle_schedule_services_stop,
    handle_schedule_sessions,
};
use crate::commands::session::{handle_session_list, handle_session_remove};
use crate::recipes::extract_from_cli::extract_recipe_info_from_cli;
use crate::recipes::recipe::{explain_recipe, render_recipe_as_yaml};
use crate::session::{build_session, SessionBuilderConfig};
use goose::agents::Container;
use goose::session::session_manager::SessionType;
use goose::session::SessionManager;
use std::io::Read;
use std::path::PathBuf;
use tracing::warn;

#[derive(Parser)]
#[command(name = "goose", author, version, display_name = "", about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Args, Debug, Clone)]
#[group(required = false, multiple = false)]
pub struct Identifier {
    #[arg(
        short = 'n',
        long,
        value_name = "NAME",
        help = "Name for the chat session (e.g., 'project-x')",
        long_help = "Specify a name for your chat session. When used with --resume, will resume this specific session if it exists."
    )]
    pub name: Option<String>,

    #[arg(
        long = "session-id",
        alias = "id",
        value_name = "SESSION_ID",
        help = "Session ID (e.g., '20250921_143022')",
        long_help = "Specify a session ID directly. When used with --resume, will resume this specific session if it exists."
    )]
    pub session_id: Option<String>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Legacy: Path for the chat session",
        long_help = "Legacy parameter for backward compatibility. Extracts session ID from the file path (e.g., '/path/to/20250325_200615.
jsonl' -> '20250325_200615')."
    )]
    pub path: Option<PathBuf>,
}

/// Session behavior options shared between Session and Run commands
#[derive(Args, Debug, Clone, Default)]
pub struct SessionOptions {
    #[arg(
        long,
        help = "Enable debug output mode with full content and no truncation",
        long_help = "When enabled, shows complete tool responses without truncation and full paths."
    )]
    pub debug: bool,

    #[arg(
        long = "max-tool-repetitions",
        value_name = "NUMBER",
        help = "Maximum number of consecutive identical tool calls allowed",
        long_help = "Set a limit on how many times the same tool can be called consecutively with identical parameters. Helps prevent infinite loops."
    )]
    pub max_tool_repetitions: Option<u32>,

    #[arg(
        long = "max-turns",
        value_name = "NUMBER",
        help = "Maximum number of turns allowed without user input (default: 1000)",
        long_help = "Set a limit on how many turns (iterations) the agent can take without asking for user input to continue."
    )]
    pub max_turns: Option<u32>,

    #[arg(
        long = "container",
        value_name = "CONTAINER_ID",
        help = "Docker container ID to run extensions inside",
        long_help = "Run extensions (stdio and built-in) inside the specified container. The extension must exist in the container. For built-in extensions, goose must be installed inside the container."
    )]
    pub container: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StreamableHttpOptions {
    pub url: String,
    pub timeout: u64,
}

fn parse_streamable_http_extension(input: &str) -> Result<StreamableHttpOptions, String> {
    let mut input_iter = input.split_whitespace();
    let (mut url, mut timeout) = (String::new(), goose::config::DEFAULT_EXTENSION_TIMEOUT);

    if let Some(url_str) = input_iter.next() {
        url.push_str(url_str);
    }

    for kv_pair in input_iter {
        if !kv_pair.contains('=') {
            continue;
        }

        let (key, value) = kv_pair.split_once('=').unwrap();

        // We Can have more keys here for setting other properties
        if key == "timeout" {
            if let Ok(seconds) = value.parse::<u64>() {
                timeout = seconds;
            }
        }
    }

    Ok(StreamableHttpOptions { url, timeout })
}

/// Extension configuration options shared between Session and Run commands
#[derive(Args, Debug, Clone, Default)]
pub struct ExtensionOptions {
    #[arg(
        long = "with-extension",
        value_name = "COMMAND",
        help = "Add stdio extensions (can be specified multiple times)",
        long_help = "Add stdio extensions from full commands with environment variables. Can be specified multiple times. Format: 'ENV1=val1 ENV2=val2 command args...'",
        action = clap::ArgAction::Append
    )]
    pub extensions: Vec<String>,

    #[arg(
        long = "with-streamable-http-extension",
        value_name = "URL",
        help = "Add streamable HTTP extensions (can be specified multiple times)",
        long_help = "Add streamable HTTP extensions from a URL. Can be specified multiple times. Format: 'url...' or 'url... timeout=100' to set up timeout other than default",
        action = clap::ArgAction::Append,
        value_parser = parse_streamable_http_extension
    )]
    pub streamable_http_extensions: Vec<StreamableHttpOptions>,

    #[arg(
        long = "with-builtin",
        value_name = "NAME",
        help = "Add builtin extensions by name (e.g., 'developer' or multiple: 'developer,github')",
        long_help = "Add one or more builtin extensions that are bundled with goose by specifying their names, comma-separated",
        value_delimiter = ','
    )]
    pub builtins: Vec<String>,

    #[arg(
        long = "no-profile",
        help = "Don't load your default extensions, only use CLI-specified extensions"
    )]
    pub no_profile: bool,
}

/// Input source and recipe options for the run command
#[derive(Args, Debug, Clone, Default)]
pub struct InputOptions {
    /// Path to instruction file containing commands
    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Path to instruction file containing commands. Use - for stdin.",
        conflicts_with = "input_text",
        conflicts_with = "recipe"
    )]
    pub instructions: Option<String>,

    /// Input text containing commands
    #[arg(
        short = 't',
        long = "text",
        value_name = "TEXT",
        help = "Input text to provide to goose directly",
        long_help = "Input text containing commands for goose. Use this in lieu of the instructions argument.",
        conflicts_with = "instructions",
        conflicts_with = "recipe"
    )]
    pub input_text: Option<String>,

    /// Recipe name or full path to the recipe file
    #[arg(
        short = None,
        long = "recipe",
        value_name = "RECIPE_NAME or FULL_PATH_TO_RECIPE_FILE",
        help = "Recipe name to get recipe file or the full path of the recipe file (use --explain to see recipe details)",
        long_help = "Recipe name to get recipe file or the full path of the recipe file that defines a custom agent configuration. Use --explain to see the recipe's title, description, and parameters.",
        conflicts_with = "instructions",
        conflicts_with = "input_text"
    )]
    pub recipe: Option<String>,

    /// Additional system prompt to customize agent behavior
    #[arg(
        long = "system",
        value_name = "TEXT",
        help = "Additional system prompt to customize agent behavior",
        long_help = "Provide additional system instructions to customize the agent's behavior",
        conflicts_with = "recipe"
    )]
    pub system: Option<String>,

    #[arg(
        long,
        value_name = "KEY=VALUE",
        help = "Dynamic parameters (e.g., --params username=alice --params channel_name=goose-channel)",
        long_help = "Key-value parameters to pass to the recipe file. Can be specified multiple times.",
        action = clap::ArgAction::Append,
        value_parser = parse_key_val,
    )]
    pub params: Vec<(String, String)>,

    /// Additional sub-recipe file paths
    #[arg(
        long = "sub-recipe",
        value_name = "RECIPE",
        help = "Sub-recipe name or file path (can be specified multiple times)",
        long_help = "Specify sub-recipes to include alongside the main recipe. Can be:\n  - Recipe names from GitHub (if GOOSE_RECIPE_GITHUB_REPO is configured)\n  - Local file paths to YAML files\nCan be specified multiple times to include multiple sub-recipes.",
        action = clap::ArgAction::Append
    )]
    pub additional_sub_recipes: Vec<String>,

    /// Show the recipe title, description, and parameters
    #[arg(
        long = "explain",
        help = "Show the recipe title, description, and parameters"
    )]
    pub explain: bool,

    /// Print the rendered recipe instead of running it
    #[arg(
        long = "render-recipe",
        help = "Print the rendered recipe instead of running it."
    )]
    pub render_recipe: bool,
}

/// Output configuration options for the run command
#[derive(Args, Debug, Clone)]
pub struct OutputOptions {
    /// Quiet mode - suppress non-response output
    #[arg(
        short = 'q',
        long = "quiet",
        help = "Quiet mode. Suppress non-response output, printing only the model response to stdout"
    )]
    pub quiet: bool,

    /// Output format (text, json, stream-json)
    #[arg(
        long = "output-format",
        value_name = "FORMAT",
        help = "Output format (text, json, stream-json)",
        default_value = "text",
        value_parser = clap::builder::PossibleValuesParser::new(["text", "json", "stream-json"])
    )]
    pub output_format: String,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            quiet: false,
            output_format: "text".to_string(),
        }
    }
}

/// Model/provider override options for the run command
#[derive(Args, Debug, Clone, Default)]
pub struct ModelOptions {
    /// Provider to use for this run (overrides environment variable)
    #[arg(
        long = "provider",
        value_name = "PROVIDER",
        help = "Specify the LLM provider to use (e.g., 'openai', 'anthropic')",
        long_help = "Override the GOOSE_PROVIDER environment variable for this run. Available providers include openai, anthropic, ollama, databricks, gemini-cli, claude-code, and others."
    )]
    pub provider: Option<String>,

    /// Model to use for this run (overrides environment variable)
    #[arg(
        long = "model",
        value_name = "MODEL",
        help = "Specify the model to use (e.g., 'gpt-4o', 'claude-sonnet-4-20250514')",
        long_help = "Override the GOOSE_MODEL environment variable for this run. The model must be supported by the specified provider."
    )]
    pub model: Option<String>,
}

/// Run execution behavior options
#[derive(Args, Debug, Clone, Default)]
pub struct RunBehavior {
    /// Continue in interactive mode after processing input
    #[arg(
        short = 's',
        long = "interactive",
        help = "Continue in interactive mode after processing initial input"
    )]
    pub interactive: bool,

    /// Run without storing a session file
    #[arg(
        long = "no-session",
        help = "Run without storing a session file",
        long_help = "Execute commands without creating or using a session file. Useful for automated runs.",
        conflicts_with_all = ["resume", "name", "path"]
    )]
    pub no_session: bool,

    /// Resume a previous run
    #[arg(
        short,
        long,
        action = clap::ArgAction::SetTrue,
        help = "Resume from a previous run",
        long_help = "Continue from a previous run, maintaining the execution state and context."
    )]
    pub resume: bool,

    /// Scheduled job ID (used internally for scheduled executions)
    #[arg(
        long = "scheduled-job-id",
        value_name = "ID",
        help = "ID of the scheduled job that triggered this execution (internal use)",
        long_help = "Internal parameter used when this run command is executed by a scheduled job. This associates the session with the schedule for tracking purposes.",
        hide = true
    )]
    pub scheduled_job_id: Option<String>,
}

async fn get_or_create_session_id(
    identifier: Option<Identifier>,
    resume: bool,
    no_session: bool,
    goose_mode: GooseMode,
) -> Result<Option<String>> {
    if no_session {
        return Ok(None);
    }

    let session_manager = SessionManager::instance();

    let resolved_id = if resume {
        let Some(id) = identifier else {
            let sessions = session_manager.list_sessions().await?;
            let session_id = sessions
                .first()
                .map(|s| s.id.clone())
                .ok_or_else(|| anyhow::anyhow!("No session found to resume"))?;
            return Ok(Some(session_id));
        };

        if let Some(session_id) = id.session_id {
            session_id
        } else if let Some(name) = id.name {
            let sessions = session_manager.list_sessions().await?;
            sessions
                .into_iter()
                .find(|s| s.name == name || s.id == name)
                .map(|s| s.id)
                .ok_or_else(|| anyhow::anyhow!("No session found with name '{}'", name))?
        } else if let Some(path) = id.path {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    anyhow::anyhow!("Could not extract session ID from path: {:?}", path)
                })?
        } else {
            return Err(anyhow::anyhow!("Invalid identifier"));
        }
    } else {
        let Some(id) = identifier else {
            let session = session_manager
                .create_session(
                    std::env::current_dir()?,
                    "CLI Session".to_string(),
                    SessionType::User,
                    goose_mode,
                )
                .await?;
            return Ok(Some(session.id));
        };

        if id.session_id.is_some() {
            return Err(anyhow::anyhow!("Cannot use --session-id without --resume"));
        }

        let has_user_provided_name = id.name.is_some();
        let name = id.name.unwrap_or_else(|| "CLI Session".to_string());
        let session = session_manager
            .create_session(
                std::env::current_dir()?,
                name.clone(),
                SessionType::User,
                goose_mode,
            )
            .await?;

        if has_user_provided_name {
            session_manager
                .update(&session.id)
                .user_provided_name(name)
                .apply()
                .await?;
        }

        return Ok(Some(session.id));
    };

    Ok(Some(resolved_id))
}

async fn lookup_session_id(identifier: Identifier) -> Result<String> {
    let session_manager = SessionManager::instance();

    if let Some(session_id) = identifier.session_id {
        Ok(session_id)
    } else if let Some(name) = identifier.name {
        let sessions = session_manager.list_sessions().await?;
        sessions
            .into_iter()
            .find(|s| s.name == name || s.id == name)
            .map(|s| s.id)
            .ok_or_else(|| anyhow::anyhow!("No session found with name '{}'", name))
    } else if let Some(path) = identifier.path {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Could not extract session ID from path: {:?}", path))
    } else {
        Err(anyhow::anyhow!("No identifier provided"))
    }
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    match s.split_once('=') {
        Some((key, value)) => Ok((key.to_string(), value.to_string())),
        None => Err(format!("invalid KEY=VALUE: {}", s)),
    }
}

#[derive(Subcommand)]
enum SessionCommand {
    #[command(about = "List all available sessions")]
    List {
        #[arg(
            short,
            long,
            help = "Output format (text, json)",
            default_value = "text"
        )]
        format: String,

        #[arg(
            long = "ascending",
            help = "Sort by date in ascending order (oldest first)",
            long_help = "Sort sessions by date in ascending order (oldest first). Default is descending order (newest first)."
        )]
        ascending: bool,

        #[arg(
            short = 'w',
            short_alias = 'p',
            long = "working_dir",
            help = "Filter sessions by working directory"
        )]
        working_dir: Option<PathBuf>,

        #[arg(short = 'l', long = "limit", help = "Limit the number of results")]
        limit: Option<usize>,
    },
    #[command(about = "Remove sessions. Runs interactively if no ID, name, or regex is provided.")]
    Remove {
        #[command(flatten)]
        identifier: Option<Identifier>,
        #[arg(
            short = 'r',
            long,
            help = "Regex for removing matched sessions (optional)"
        )]
        regex: Option<String>,
    },
    #[command(about = "Export a session")]
    Export {
        #[command(flatten)]
        identifier: Option<Identifier>,

        #[arg(
            short,
            long,
            help = "Output file path (default: stdout)",
            long_help = "Path to save the exported Markdown. If not provided, output will be sent to stdout"
        )]
        output: Option<PathBuf>,

        #[arg(
            long = "format",
            value_name = "FORMAT",
            help = "Output format (markdown, json, yaml)",
            default_value = "markdown"
        )]
        format: String,
    },
    #[command(name = "diagnostics")]
    Diagnostics {
        /// Session identifier for generating diagnostics
        #[command(flatten)]
        identifier: Option<Identifier>,

        /// Output path for the diagnostics zip file (optional, defaults to current directory)
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum SchedulerCommand {
    #[command(about = "Add a new scheduled job")]
    Add {
        #[arg(
            long = "schedule-id",
            alias = "id",
            help = "Unique ID for the recurring scheduled job"
        )]
        schedule_id: String,
        #[arg(
            long,
            help = "Cron expression for the schedule",
            long_help = "Cron expression for when to run the job. Examples:\n  '0 * * * *'     - Every hour at minute 0\n  '0 */2 * * *'   - Every 2 hours\n  '@hourly'       - Every hour (shorthand)\n  '0 9 * * *'     - Every day at 9:00 AM\n  '0 9 * * 1'     - Every Monday at 9:00 AM\n  '0 0 1 * *'     - First day of every month at midnight"
        )]
        cron: String,
        #[arg(
            long,
            help = "Recipe source (path to file, or base64 encoded recipe string)"
        )]
        recipe_source: String,
    },
    #[command(about = "List all scheduled jobs")]
    List {},
    #[command(about = "Remove a scheduled job by ID")]
    Remove {
        #[arg(
            long = "schedule-id",
            alias = "id",
            help = "ID of the scheduled job to remove (removes the recurring schedule)"
        )]
        schedule_id: String,
    },
    /// List sessions created by a specific schedule
    #[command(about = "List sessions created by a specific schedule")]
    Sessions {
        /// ID of the schedule
        #[arg(long = "schedule-id", alias = "id", help = "ID of the schedule")]
        schedule_id: String,
        #[arg(short = 'l', long, help = "Maximum number of sessions to return")]
        limit: Option<usize>,
    },
    #[command(about = "Run a scheduled job immediately")]
    RunNow {
        /// ID of the schedule to run
        #[arg(long = "schedule-id", alias = "id", help = "ID of the schedule to run")]
        schedule_id: String,
    },
    /// Check status of scheduler services (deprecated - no external services needed)
    #[command(about = "[Deprecated] Check status of scheduler services")]
    ServicesStatus {},
    /// Stop scheduler services (deprecated - no external services needed)
    #[command(about = "[Deprecated] Stop scheduler services")]
    ServicesStop {},
    /// Show cron expression examples and help
    #[command(about = "Show cron expression examples and help")]
    CronHelp {},
}

#[derive(Subcommand)]
enum GatewayCommand {
    #[command(about = "Show gateway status")]
    Status {},

    #[command(about = "Start a gateway")]
    Start {
        #[arg(help = "Gateway type (e.g., 'telegram')")]
        gateway_type: String,

        #[arg(
            long = "bot-token",
            help = "Bot token for the gateway platform",
            long_help = "Authentication token for the gateway platform (e.g., Telegram bot token)"
        )]
        bot_token: String,
    },

    #[command(about = "Stop a running gateway")]
    Stop {
        #[arg(help = "Gateway type to stop (e.g., 'telegram')")]
        gateway_type: String,
    },

    #[command(about = "Generate a pairing code for a gateway")]
    Pair {
        #[arg(help = "Gateway type to generate pairing code for")]
        gateway_type: String,
    },
}

#[derive(Subcommand)]
enum RecipeCommand {
    /// Validate a recipe file
    #[command(about = "Validate a recipe")]
    Validate {
        /// Recipe name to get recipe file to validate
        #[arg(help = "recipe name to get recipe file or full path to the recipe file to validate")]
        recipe_name: String,
    },

    /// Generate a deeplink for a recipe file
    #[command(about = "Generate a deeplink for a recipe")]
    Deeplink {
        /// Recipe name to get recipe file to generate deeplink
        #[arg(
            help = "recipe name to get recipe file or full path to the recipe file to generate deeplink"
        )]
        recipe_name: String,
        /// Recipe parameters in key=value format (can be specified multiple times)
        #[arg(
            short = 'p',
            long = "param",
            value_name = "KEY=VALUE",
            help = "Recipe parameter in key=value format (can be specified multiple times)"
        )]
        params: Vec<String>,
    },

    /// Open a recipe in Goose Desktop
    #[command(about = "Open a recipe in Goose Desktop")]
    Open {
        /// Recipe name to get recipe file to open
        #[arg(help = "recipe name or full path to the recipe file")]
        recipe_name: String,
        /// Recipe parameters in key=value format (can be specified multiple times)
        #[arg(
            short = 'p',
            long = "param",
            value_name = "KEY=VALUE",
            help = "Recipe parameter in key=value format (can be specified multiple times)"
        )]
        params: Vec<String>,
    },

    /// List available recipes
    #[command(about = "List available recipes")]
    List {
        /// Output format (text, json)
        #[arg(
            long = "format",
            value_name = "FORMAT",
            help = "Output format (text, json)",
            default_value = "text"
        )]
        format: String,

        /// Show verbose information including recipe descriptions
        #[arg(
            short,
            long,
            help = "Show verbose information including recipe descriptions"
        )]
        verbose: bool,
    },
}

#[derive(Subcommand)]
enum Command {
    /// Configure goose settings
    #[command(about = "Configure goose settings")]
    Configure {},

    /// Display goose configuration information
    #[command(about = "Display goose information")]
    Info {
        /// Show verbose information including current configuration
        #[arg(short, long, help = "Show verbose information including config.yaml")]
        verbose: bool,
    },

    #[command(about = "Check that your Goose setup is working")]
    Doctor {},

    /// Manage system prompts and behaviors
    #[command(about = "Run one of the mcp servers bundled with goose")]
    Mcp {
        #[arg(value_parser = clap::value_parser!(McpCommand))]
        server: McpCommand,
    },

    /// Run goose as an ACP (Agent Client Protocol) agent
    #[command(about = "Run goose as an ACP agent server on stdio")]
    Acp {
        /// Add builtin extensions by name
        #[arg(
            long = "with-builtin",
            value_name = "NAME",
            help = "Add builtin extensions by name (e.g., 'developer' or multiple: 'developer,github')",
            long_help = "Add one or more builtin extensions that are bundled with goose by specifying their names, comma-separated",
            value_delimiter = ','
        )]
        builtins: Vec<String>,
    },

    /// Start ACP server over HTTP and WebSocket
    #[command(about = "Start ACP server over HTTP and WebSocket")]
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        #[arg(long, default_value = "3284")]
        port: u16,

        #[arg(
            long = "with-builtin",
            value_name = "NAME",
            help = "Add builtin extensions by name (e.g., 'developer' or multiple: 'developer,github')",
            long_help = "Add one or more builtin extensions that are bundled with goose by specifying their names, comma-separated",
            value_delimiter = ',',
            action = clap::ArgAction::Append
        )]
        builtins: Vec<String>,
    },

    /// Start or resume interactive chat sessions
    #[command(
        about = "Start or resume interactive chat sessions",
        visible_alias = "s"
    )]
    Session {
        #[command(subcommand)]
        command: Option<SessionCommand>,

        #[command(flatten)]
        identifier: Option<Identifier>,

        /// Resume a previous session
        #[arg(
            short,
            long,
            help = "Resume a previous session (last used or specified by --name/--session-id)",
            long_help = "Continue from a previous session. If --name or --session-id is provided, resumes that specific session. Otherwise, resumes the most recently used session."
        )]
        resume: bool,

        /// Fork a previous session (creates new session with copied history)
        #[arg(
            long,
            requires = "resume",
            help = "Fork a previous session (creates new session with copied history)",
            long_help = "Create a new session by copying all messages from a previous session. Must be used with --resume. If --name or --session-id is provided, forks that specific session. Otherwise, forks the most recently used session."
        )]
        fork: bool,

        /// Show message history when resuming
        #[arg(
            long,
            help = "Show previous messages when resuming a session",
            requires = "resume"
        )]
        history: bool,

        #[command(flatten)]
        session_opts: SessionOptions,

        #[command(flatten)]
        extension_opts: ExtensionOptions,
    },

    /// Open the last project directory
    #[command(about = "Open the last project directory", visible_alias = "p")]
    Project {},

    /// List recent project directories
    #[command(about = "List recent project directories", visible_alias = "ps")]
    Projects,

    /// Execute commands from an instruction file
    #[command(about = "Execute commands from an instruction file or stdin")]
    Run {
        #[command(flatten)]
        input_opts: InputOptions,

        #[command(flatten)]
        identifier: Option<Identifier>,

        #[command(flatten)]
        run_behavior: RunBehavior,

        #[command(flatten)]
        session_opts: SessionOptions,

        #[command(flatten)]
        extension_opts: ExtensionOptions,

        #[command(flatten)]
        output_opts: OutputOptions,

        #[command(flatten)]
        model_opts: ModelOptions,
    },

    /// Recipe utilities for validation and deeplinking
    #[command(about = "Recipe utilities for validation and deeplinking")]
    Recipe {
        #[command(subcommand)]
        command: RecipeCommand,
    },

    /// Manage scheduled jobs
    #[command(about = "Manage scheduled jobs", visible_alias = "sched")]
    Schedule {
        #[command(subcommand)]
        command: SchedulerCommand,
    },

    /// Manage gateways for external platform integrations (e.g., Telegram)
    #[command(
        about = "Manage gateways for external platform integrations",
        visible_alias = "gw"
    )]
    Gateway {
        #[command(subcommand)]
        command: GatewayCommand,
    },

    /// Update the goose CLI version
    #[command(about = "Update the goose CLI version")]
    Update {
        /// Update to canary version
        #[arg(
            short,
            long,
            help = "Update to canary version",
            long_help = "Update to the latest canary version of the goose CLI, otherwise updates to the latest stable version."
        )]
        canary: bool,

        /// Enforce to re-configure goose during update
        #[arg(short, long, help = "Enforce to re-configure goose during update")]
        reconfigure: bool,
    },

    /// Terminal-integrated session (one session per terminal)
    #[command(
        about = "Terminal-integrated goose session",
        long_about = "Runs a goose session tied to your terminal window.\n\
                      Each terminal maintains its own persistent session that resumes automatically.\n\n\
                      Setup:\n  \
                        eval \"$(goose term init zsh)\"  # Add to ~/.zshrc\n\n\
                      Usage:\n  \
                        goose term run \"list files in this directory\"\n  \
                        @goose \"create a python script\"  # using alias\n  \
                        @g \"quick question\"  # short alias"
    )]
    Term {
        #[command(subcommand)]
        command: TermCommand,
    },
    /// Manage local inference models
    #[cfg(feature = "local-inference")]
    #[command(about = "Manage local inference models", visible_alias = "lm")]
    LocalModels {
        #[command(subcommand)]
        command: LocalModelsCommand,
    },

    /// Generate completions for various shells
    #[command(about = "Generate the autocompletion script for the specified shell")]
    Completion {
        #[arg(value_enum)]
        shell: ClapShell,

        #[arg(long, default_value = "goose", help = "Provide a custom binary name")]
        bin_name: String,
    },

    #[command(
        name = "validate-extensions",
        about = "Validate a bundled-extensions.json file",
        hide = true
    )]
    ValidateExtensions {
        #[arg(help = "Path to the bundled-extensions.json file")]
        file: PathBuf,
    },
}

#[cfg(feature = "local-inference")]
#[derive(Subcommand)]
enum LocalModelsCommand {
    /// Search HuggingFace for GGUF models
    #[command(about = "Search HuggingFace for GGUF models")]
    Search {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Download a model from HuggingFace
    #[command(about = "Download a GGUF model (e.g. bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M)")]
    Download {
        /// Model spec in user/repo:quantization format
        spec: String,
    },

    /// List downloaded local models
    #[command(about = "List downloaded local models")]
    List,

    /// Delete a downloaded model
    #[command(about = "Delete a downloaded local model")]
    Delete {
        /// Model ID to delete
        id: String,
    },
}

#[derive(Subcommand)]
enum TermCommand {
    /// Print shell initialization script
    #[command(
        about = "Print shell initialization script",
        long_about = "Prints shell configuration to set up terminal-integrated sessions.\n\
                      Each terminal gets a persistent goose session that automatically resumes.\n\n\
                      Setup:\n  \
                        echo 'eval \"$(goose term init zsh)\"' >> ~/.zshrc\n  \
                        source ~/.zshrc\n\n\
                      With --default (anything typed that isn't a command goes to goose):\n  \
                        echo 'eval \"$(goose term init zsh --default)\"' >> ~/.zshrc"
    )]
    Init {
        /// Shell type (bash, zsh, fish, powershell)
        #[arg(value_enum)]
        shell: Shell,

        #[arg(short, long, help = "Name for the terminal session")]
        name: Option<String>,

        /// Make goose the default handler for unknown commands
        #[arg(
            long = "default",
            help = "Make goose the default handler for unknown commands",
            long_help = "When enabled, anything you type that isn't a valid command will be sent to goose. Only supported for zsh and bash."
        )]
        default: bool,
    },

    /// Log a shell command (called by shell hook)
    #[command(about = "Log a shell command to the session", hide = true)]
    Log {
        /// The command that was executed
        command: String,
    },

    /// Run a prompt in the terminal session
    #[command(
        about = "Run a prompt in the terminal session",
        long_about = "Run a prompt in the terminal-integrated session.\n\n\
                      Examples:\n  \
                        goose term run list files in this directory\n  \
                        @goose list files  # using alias\n  \
                        @g why did that fail  # short alias"
    )]
    Run {
        /// The prompt to send to goose (multiple words allowed without quotes)
        #[arg(required = true, num_args = 1..)]
        prompt: Vec<String>,
    },

    /// Print session info for prompt integration
    #[command(
        about = "Print session info for prompt integration",
        long_about = "Prints compact session info (token usage, model) for shell prompt integration.\n\
                      Example output: ●○○○○ sonnet"
    )]
    Info,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum CliProviderVariant {
    OpenAi,
    Databricks,
    Ollama,
}

#[derive(Debug)]
pub struct InputConfig {
    pub contents: Option<String>,
    pub additional_system_prompt: Option<String>,
}

fn get_command_name(command: &Option<Command>) -> &'static str {
    match command {
        Some(Command::Configure {}) => "configure",
        Some(Command::Doctor {}) => "doctor",
        Some(Command::Info { .. }) => "info",
        Some(Command::Mcp { .. }) => "mcp",
        Some(Command::Acp { .. }) => "acp",
        Some(Command::Serve { .. }) => "serve",
        Some(Command::Session { .. }) => "session",
        Some(Command::Project {}) => "project",
        Some(Command::Projects) => "projects",
        Some(Command::Run { .. }) => "run",
        Some(Command::Gateway { .. }) => "gateway",
        Some(Command::Schedule { .. }) => "schedule",
        Some(Command::Update { .. }) => "update",
        Some(Command::Recipe { .. }) => "recipe",
        Some(Command::Term { .. }) => "term",
        #[cfg(feature = "local-inference")]
        Some(Command::LocalModels { .. }) => "local-models",
        Some(Command::Completion { .. }) => "completion",
        Some(Command::ValidateExtensions { .. }) => "validate-extensions",
        None => "default_session",
    }
}

async fn handle_mcp_command(server: McpCommand) -> Result<()> {
    let name = server.name();
    let _ = crate::logging::setup_logging(Some(&format!("mcp-{name}")));
    match server {
        McpCommand::AutoVisualiser => serve(AutoVisualiserRouter::new()).await?,
        McpCommand::ComputerController => serve(ComputerControllerServer::new()).await?,
        McpCommand::Memory => serve(MemoryServer::new()).await?,
        McpCommand::Tutorial => serve(TutorialServer::new()).await?,
    }
    Ok(())
}

async fn handle_serve_command(host: String, port: u16, builtins: Vec<String>) -> Result<()> {
    use goose::acp::server_factory::{AcpServer, AcpServerFactoryConfig};
    use goose::acp::transport::create_router;
    use goose::config::paths::Paths;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tracing::info;

    let builtins = if builtins.is_empty() {
        vec!["developer".to_string()]
    } else {
        builtins
    };

    let server = Arc::new(AcpServer::new(AcpServerFactoryConfig {
        builtins,
        data_dir: Paths::data_dir(),
        config_dir: Paths::config_dir(),
    }));
    let router = create_router(server);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    info!("Starting ACP server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

async fn handle_session_subcommand(command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::List {
            format,
            ascending,
            working_dir,
            limit,
        } => {
            handle_session_list(format, ascending, working_dir, limit).await?;
        }
        SessionCommand::Remove { identifier, regex } => {
            let (session_id, name) = if let Some(id) = identifier {
                (id.session_id, id.name)
            } else {
                (None, None)
            };
            handle_session_remove(session_id, name, regex).await?;
        }
        SessionCommand::Export {
            identifier,
            output,
            format,
        } => {
            let session_manager = SessionManager::instance();
            let session_identifier = if let Some(id) = identifier {
                lookup_session_id(id).await?
            } else {
                match crate::commands::session::prompt_interactive_session_selection(
                    &session_manager,
                )
                .await
                {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        return Ok(());
                    }
                }
            };
            crate::commands::session::handle_session_export(session_identifier, output, format)
                .await?;
        }
        SessionCommand::Diagnostics { identifier, output } => {
            let session_manager = SessionManager::instance();
            let session_id = if let Some(id) = identifier {
                lookup_session_id(id).await?
            } else {
                match crate::commands::session::prompt_interactive_session_selection(
                    &session_manager,
                )
                .await
                {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        return Ok(());
                    }
                }
            };
            crate::commands::session::handle_diagnostics(&session_id, output).await?;
        }
    }
    Ok(())
}

async fn handle_interactive_session(
    identifier: Option<Identifier>,
    resume: bool,
    fork: bool,
    history: bool,
    session_opts: SessionOptions,
    extension_opts: ExtensionOptions,
) -> Result<()> {
    #[cfg(feature = "telemetry")]
    if get_telemetry_choice().is_none() {
        configure_telemetry_consent_dialog()?;
    }

    let session_start = std::time::Instant::now();
    let session_type = if fork {
        "forked"
    } else if resume {
        "resumed"
    } else {
        "new"
    };

    tracing::info!(
        monotonic_counter.goose.session_starts = 1,
        session_type,
        interactive = true,
        "Session started"
    );

    if let Some(Identifier {
        session_id: Some(_),
        ..
    }) = &identifier
    {
        if !resume {
            eprintln!("Error: --session-id can only be used with --resume flag");
            std::process::exit(1);
        }
    }

    let goose_mode = Config::global().get_goose_mode().unwrap_or_default();
    let mut session_id = get_or_create_session_id(identifier, resume, false, goose_mode).await?;

    if fork {
        if let Some(id) = session_id {
            let session_manager = SessionManager::instance();
            let original = session_manager.get_session(&id, false).await?;
            let copied = session_manager.copy_session(&id, original.name).await?;
            session_id = Some(copied.id);
        }
    }

    let mut session: crate::CliSession = build_session(SessionBuilderConfig {
        session_id,
        resume,
        fork,
        no_session: false,
        extensions: extension_opts.extensions,
        streamable_http_extensions: extension_opts.streamable_http_extensions,
        builtins: extension_opts.builtins,
        no_profile: extension_opts.no_profile,
        recipe: None,
        additional_system_prompt: None,
        provider: None,
        model: None,
        debug: session_opts.debug,
        max_tool_repetitions: session_opts.max_tool_repetitions,
        max_turns: session_opts.max_turns,
        scheduled_job_id: None,
        interactive: true,
        quiet: false,
        output_format: "text".to_string(),
        container: session_opts.container.map(Container::new),
    })
    .await;

    if (resume || fork) && history {
        session.render_message_history();
    }

    let result = session.interactive(None).await;
    log_session_completion(&session, session_start, session_type, result.is_ok()).await;
    result
}

async fn log_session_completion(
    session: &crate::CliSession,
    session_start: std::time::Instant,
    session_type: &str,
    success: bool,
) {
    let session_duration = session_start.elapsed();
    let exit_type = if success { "normal" } else { "error" };

    let (total_tokens, message_count) = session
        .get_session()
        .await
        .map(|m| (m.total_tokens.unwrap_or(0), m.message_count))
        .unwrap_or((0, 0));

    tracing::info!(
        monotonic_counter.goose.session_completions = 1,
        session_type,
        exit_type,
        duration_ms = session_duration.as_millis() as u64,
        total_tokens,
        message_count,
        "Session completed"
    );

    tracing::info!(
        monotonic_counter.goose.session_duration_ms = session_duration.as_millis() as u64,
        session_type,
        "Session duration"
    );

    if total_tokens > 0 {
        tracing::info!(
            monotonic_counter.goose.session_tokens = total_tokens,
            session_type,
            "Session tokens"
        );
    }
}

fn parse_run_input(
    input_opts: &InputOptions,
    quiet: bool,
) -> Result<Option<(InputConfig, Option<Recipe>)>> {
    match (
        &input_opts.instructions,
        &input_opts.input_text,
        &input_opts.recipe,
    ) {
        (Some(file), _, _) if file == "-" => {
            let mut contents = String::new();
            std::io::stdin()
                .read_to_string(&mut contents)
                .expect("Failed to read from stdin");
            Ok(Some((
                InputConfig {
                    contents: Some(contents),
                    additional_system_prompt: input_opts.system.clone(),
                },
                None,
            )))
        }
        (Some(file), _, _) => {
            let contents = std::fs::read_to_string(file).unwrap_or_else(|err| {
                eprintln!(
                    "Instruction file not found — did you mean to use goose run --text?\n{}",
                    err
                );
                std::process::exit(1);
            });
            Ok(Some((
                InputConfig {
                    contents: Some(contents),
                    additional_system_prompt: None,
                },
                None,
            )))
        }
        (_, Some(text), _) => Ok(Some((
            InputConfig {
                contents: Some(text.clone()),
                additional_system_prompt: input_opts.system.clone(),
            },
            None,
        ))),
        (_, _, Some(recipe_name)) => {
            let recipe_display_name = std::path::Path::new(recipe_name)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(recipe_name);

            let recipe_version = crate::recipes::search_recipe::load_recipe_file(recipe_name)
                .ok()
                .and_then(|rf| {
                    goose::recipe::template_recipe::parse_recipe_content(
                        &rf.content,
                        Some(rf.parent_dir.display().to_string()),
                    )
                    .ok()
                    .map(|(r, _)| r.version)
                })
                .unwrap_or_else(|| "unknown".to_string());

            if input_opts.explain {
                explain_recipe(recipe_name, input_opts.params.clone())?;
                return Ok(None);
            }
            if input_opts.render_recipe {
                if let Err(err) = render_recipe_as_yaml(recipe_name, input_opts.params.clone()) {
                    eprintln!("{}: {}", console::style("Error").red().bold(), err);
                    std::process::exit(1);
                }
                return Ok(None);
            }

            tracing::info!(
                monotonic_counter.goose.recipe_runs = 1,
                recipe_name = %recipe_display_name,
                recipe_version = %recipe_version,
                session_type = "recipe",
                interface = "cli",
                "Recipe execution started"
            );

            let (input_config, recipe) = extract_recipe_info_from_cli(
                recipe_name.clone(),
                input_opts.params.clone(),
                input_opts.additional_sub_recipes.clone(),
                quiet,
            )?;
            Ok(Some((input_config, Some(recipe))))
        }
        (None, None, None) => {
            eprintln!("Error: Must provide either --instructions (-i), --text (-t), or --recipe. Use -i - for stdin.");
            std::process::exit(1);
        }
    }
}

async fn handle_run_command(
    input_opts: InputOptions,
    identifier: Option<Identifier>,
    run_behavior: RunBehavior,
    session_opts: SessionOptions,
    extension_opts: ExtensionOptions,
    output_opts: OutputOptions,
    model_opts: ModelOptions,
) -> Result<()> {
    #[cfg(feature = "telemetry")]
    if run_behavior.interactive && get_telemetry_choice().is_none() {
        configure_telemetry_consent_dialog()?;
    }

    let parsed = parse_run_input(&input_opts, output_opts.quiet)?;

    let Some((input_config, recipe)) = parsed else {
        return Ok(());
    };

    if let Some(Identifier {
        session_id: Some(_),
        ..
    }) = &identifier
    {
        if !run_behavior.resume {
            eprintln!("Error: --session-id can only be used with --resume flag");
            std::process::exit(1);
        }
    }

    let goose_mode = Config::global().get_goose_mode().unwrap_or_default();
    let session_id = get_or_create_session_id(
        identifier,
        run_behavior.resume,
        run_behavior.no_session,
        goose_mode,
    )
    .await?;

    let mut session = build_session(SessionBuilderConfig {
        session_id,
        resume: run_behavior.resume,
        fork: false,
        no_session: run_behavior.no_session,
        extensions: extension_opts.extensions,
        streamable_http_extensions: extension_opts.streamable_http_extensions,
        builtins: extension_opts.builtins,
        no_profile: extension_opts.no_profile,
        recipe: recipe.clone(),
        additional_system_prompt: input_config.additional_system_prompt,
        provider: model_opts.provider,
        model: model_opts.model,
        debug: session_opts.debug,
        max_tool_repetitions: session_opts.max_tool_repetitions,
        max_turns: session_opts.max_turns,
        scheduled_job_id: run_behavior.scheduled_job_id,
        interactive: run_behavior.interactive,
        quiet: output_opts.quiet,
        output_format: output_opts.output_format,
        container: session_opts.container.map(Container::new),
    })
    .await;

    if run_behavior.interactive {
        session.interactive(input_config.contents).await
    } else if let Some(contents) = input_config.contents {
        let session_start = std::time::Instant::now();
        let session_type = if recipe.is_some() { "recipe" } else { "run" };

        tracing::info!(
            monotonic_counter.goose.session_starts = 1,
            session_type,
            interactive = false,
            "Headless session started"
        );

        let result = session.headless(contents).await;
        log_session_completion(&session, session_start, session_type, result.is_ok()).await;
        result
    } else {
        Err(anyhow::anyhow!(
            "no text provided for prompt in headless mode"
        ))
    }
}

async fn handle_gateway_command(command: GatewayCommand) -> Result<()> {
    use crate::commands::gateway;

    match command {
        GatewayCommand::Status {} => gateway::handle_gateway_status().await,
        GatewayCommand::Start {
            gateway_type,
            bot_token,
        } => {
            let platform_config = serde_json::json!({ "bot_token": bot_token });
            gateway::handle_gateway_start(gateway_type, platform_config).await
        }
        GatewayCommand::Stop { gateway_type } => gateway::handle_gateway_stop(gateway_type).await,
        GatewayCommand::Pair { gateway_type } => gateway::handle_gateway_pair(gateway_type).await,
    }
}

async fn handle_schedule_command(command: SchedulerCommand) -> Result<()> {
    match command {
        SchedulerCommand::Add {
            schedule_id,
            cron,
            recipe_source,
        } => handle_schedule_add(schedule_id, cron, recipe_source).await,
        SchedulerCommand::List {} => handle_schedule_list().await,
        SchedulerCommand::Remove { schedule_id } => handle_schedule_remove(schedule_id).await,
        SchedulerCommand::Sessions { schedule_id, limit } => {
            handle_schedule_sessions(schedule_id, limit).await
        }
        SchedulerCommand::RunNow { schedule_id } => handle_schedule_run_now(schedule_id).await,
        SchedulerCommand::ServicesStatus {} => handle_schedule_services_status().await,
        SchedulerCommand::ServicesStop {} => handle_schedule_services_stop().await,
        SchedulerCommand::CronHelp {} => handle_schedule_cron_help().await,
    }
}

fn handle_recipe_subcommand(command: RecipeCommand) -> Result<()> {
    match command {
        RecipeCommand::Validate { recipe_name } => handle_validate(&recipe_name),
        RecipeCommand::Deeplink {
            recipe_name,
            params,
        } => {
            handle_deeplink(&recipe_name, &params)?;
            Ok(())
        }
        RecipeCommand::Open {
            recipe_name,
            params,
        } => handle_open(&recipe_name, &params),
        RecipeCommand::List { format, verbose } => handle_list(&format, verbose),
    }
}

async fn handle_term_subcommand(command: TermCommand) -> Result<()> {
    match command {
        TermCommand::Init {
            shell,
            name,
            default,
        } => handle_term_init(shell, name, default).await,
        TermCommand::Log { command } => handle_term_log(command).await,
        TermCommand::Run { prompt } => handle_term_run(prompt).await,
        TermCommand::Info => handle_term_info().await,
    }
}

#[cfg(feature = "local-inference")]
async fn handle_local_models_command(command: LocalModelsCommand) -> Result<()> {
    use goose::providers::local_inference::hf_models;
    use goose::providers::local_inference::local_model_registry::{
        get_registry, model_id_from_repo, LocalModelEntry,
    };

    match command {
        LocalModelsCommand::Search { query, limit } => {
            println!("Searching HuggingFace for '{}'...", query);
            let results = hf_models::search_gguf_models(&query, limit).await?;

            if results.is_empty() {
                println!("No GGUF models found.");
                return Ok(());
            }

            for model in &results {
                println!(
                    "\n{} (by {}) — {} downloads",
                    model.model_name, model.author, model.downloads
                );
                for file in &model.gguf_files {
                    let size = if file.size_bytes > 0 {
                        format!(
                            "{:.1}GB",
                            file.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                        )
                    } else {
                        "unknown".to_string()
                    };
                    println!("  {} — {}", file.quantization, size);
                }
                println!(
                    "  Download: goose local-models download {}:<quantization>",
                    model.repo_id
                );
            }
        }
        LocalModelsCommand::Download { spec } => {
            println!("Resolving {}...", spec);
            let (repo_id, file) = hf_models::resolve_model_spec(&spec).await?;
            let model_id = model_id_from_repo(&repo_id, &file.quantization);
            let local_path =
                goose::config::paths::Paths::in_data_dir("models").join(&file.filename);

            println!(
                "Downloading {} ({})...",
                model_id,
                if file.size_bytes > 0 {
                    format!(
                        "{:.1}GB",
                        file.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                    )
                } else {
                    "unknown size".to_string()
                }
            );

            // Register
            let entry = LocalModelEntry {
                id: model_id.clone(),
                repo_id: repo_id.clone(),
                filename: file.filename.clone(),
                quantization: file.quantization.clone(),
                local_path: local_path.clone(),
                source_url: file.download_url.clone(),
                settings: Default::default(),
                size_bytes: file.size_bytes,
                mmproj_path: None,
                mmproj_source_url: None,
                mmproj_size_bytes: 0,
                shard_files: vec![],
            };

            {
                let mut registry = get_registry()
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Failed to acquire registry lock"))?;
                registry.add_model(entry)?;
            }

            // Download
            let manager = goose::download_manager::get_download_manager();
            manager
                .download_model(
                    format!("{}-model", model_id),
                    file.download_url,
                    local_path,
                    None,
                )
                .await?;

            // Poll progress
            loop {
                if let Some(progress) = manager.get_progress(&format!("{}-model", model_id)) {
                    match progress.status {
                        goose::download_manager::DownloadStatus::Downloading => {
                            print!(
                                "\r  {:.1}% ({:.0}MB / {:.0}MB)",
                                progress.progress_percent,
                                progress.bytes_downloaded as f64 / (1024.0 * 1024.0),
                                progress.total_bytes as f64 / (1024.0 * 1024.0),
                            );
                            use std::io::Write;
                            std::io::stdout().flush().ok();
                        }
                        goose::download_manager::DownloadStatus::Completed => {
                            println!("\nDownloaded: {}", model_id);
                            break;
                        }
                        goose::download_manager::DownloadStatus::Failed => {
                            let err = progress.error.unwrap_or_default();
                            anyhow::bail!("Download failed: {}", err);
                        }
                        goose::download_manager::DownloadStatus::Cancelled => {
                            println!("\nDownload cancelled.");
                            break;
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
        LocalModelsCommand::List => {
            let registry = get_registry()
                .lock()
                .map_err(|_| anyhow::anyhow!("Failed to acquire registry lock"))?;
            let models = registry.list_models();

            if models.is_empty() {
                println!("No local models downloaded.");
                return Ok(());
            }

            println!("{:<50} {:<10} Downloaded", "ID", "Quant");
            println!("{}", "-".repeat(70));
            for m in models {
                println!(
                    "{:<50} {:<10} {}",
                    m.id,
                    m.quantization,
                    if m.is_downloaded() { "✓" } else { "✗" }
                );
            }
        }
        LocalModelsCommand::Delete { id } => {
            let mut registry = get_registry()
                .lock()
                .map_err(|_| anyhow::anyhow!("Failed to acquire registry lock"))?;

            if let Some(entry) = registry.get_model(&id) {
                if entry.local_path.exists() {
                    std::fs::remove_file(&entry.local_path)?;
                }
                registry.remove_model(&id)?;
                println!("Deleted model: {}", id);
            } else {
                println!("Model not found: {}", id);
            }
        }
    }

    Ok(())
}

async fn handle_default_session() -> Result<()> {
    if !Config::global().exists() {
        return handle_configure().await;
    }

    #[cfg(feature = "telemetry")]
    if get_telemetry_choice().is_none() {
        configure_telemetry_consent_dialog()?;
    }

    let goose_mode = Config::global().get_goose_mode().unwrap_or_default();
    let session_id = get_or_create_session_id(None, false, false, goose_mode).await?;

    let mut session = build_session(SessionBuilderConfig {
        session_id,
        resume: false,
        fork: false,
        no_session: false,
        extensions: Vec::new(),
        streamable_http_extensions: Vec::new(),
        builtins: Vec::new(),
        no_profile: false,
        recipe: None,
        additional_system_prompt: None,
        provider: None,
        model: None,
        debug: false,
        max_tool_repetitions: None,
        max_turns: None,
        scheduled_job_id: None,
        interactive: true,
        quiet: false,
        output_format: "text".to_string(),
        container: None,
    })
    .await;
    session.interactive(None).await
}

pub async fn cli() -> anyhow::Result<()> {
    register_builtin_extensions(goose_mcp::BUILTIN_EXTENSIONS.clone());

    let cli = Cli::parse();

    if let Err(e) = crate::project_tracker::update_project_tracker(None, None) {
        warn!("Warning: Failed to update project tracker: {}", e);
    }

    let command_name = get_command_name(&cli.command);
    tracing::info!(
        monotonic_counter.goose.cli_commands = 1,
        command = command_name,
        "CLI command executed"
    );

    match cli.command {
        Some(Command::Completion { shell, bin_name }) => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
            Ok(())
        }
        Some(Command::Configure {}) => handle_configure().await,
        Some(Command::Doctor {}) => crate::commands::doctor::handle_doctor().await,
        Some(Command::Info { verbose }) => handle_info(verbose),
        Some(Command::Mcp { server }) => handle_mcp_command(server).await,
        Some(Command::Acp { builtins }) => goose::acp::server::run(builtins).await,
        Some(Command::Serve {
            host,
            port,
            builtins,
        }) => handle_serve_command(host, port, builtins).await,
        Some(Command::Session {
            command: Some(cmd), ..
        }) => handle_session_subcommand(cmd).await,
        Some(Command::Session {
            command: None,
            identifier,
            resume,
            fork,
            history,
            session_opts,
            extension_opts,
        }) => {
            handle_interactive_session(
                identifier,
                resume,
                fork,
                history,
                session_opts,
                extension_opts,
            )
            .await
        }
        Some(Command::Project {}) => {
            handle_project_default()?;
            Ok(())
        }
        Some(Command::Projects) => {
            handle_projects_interactive()?;
            Ok(())
        }
        Some(Command::Run {
            input_opts,
            identifier,
            run_behavior,
            session_opts,
            extension_opts,
            output_opts,
            model_opts,
        }) => {
            handle_run_command(
                input_opts,
                identifier,
                run_behavior,
                session_opts,
                extension_opts,
                output_opts,
                model_opts,
            )
            .await
        }
        Some(Command::Gateway { command }) => handle_gateway_command(command).await,
        Some(Command::Schedule { command }) => handle_schedule_command(command).await,
        Some(Command::Update {
            canary,
            reconfigure,
        }) => {
            crate::commands::update::update(canary, reconfigure).await?;
            Ok(())
        }
        Some(Command::Recipe { command }) => handle_recipe_subcommand(command),
        Some(Command::Term { command }) => handle_term_subcommand(command).await,
        #[cfg(feature = "local-inference")]
        Some(Command::LocalModels { command }) => handle_local_models_command(command).await,
        Some(Command::ValidateExtensions { file }) => {
            use goose::agents::validate_extensions::validate_bundled_extensions;
            match validate_bundled_extensions(&file) {
                Ok(msg) => {
                    println!("{msg}");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        None => handle_default_session().await,
    }
}
