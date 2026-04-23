mod builder;
mod completion;
mod editor;
mod elicitation;
mod export;
mod input;
mod output;
pub mod streaming_buffer;
mod task_execution_display;
mod thinking;

use crate::session::task_execution_display::{
    format_task_execution_notification, TASK_EXECUTION_NOTIFICATION_TYPE,
};
use goose::conversation::Conversation;
use std::io::Write;
use std::str::FromStr;
use tokio::signal::ctrl_c;
use tokio_util::task::AbortOnDropHandle;

pub use self::export::message_to_markdown;
pub use builder::{build_session, SessionBuilderConfig};
use console::Color;
use goose::agents::AgentEvent;
use goose::agents::SUBAGENT_TOOL_REQUEST_TYPE;
use goose::permission::permission_confirmation::PrincipalType;
use goose::permission::Permission;
use goose::permission::PermissionConfirmation;
use goose::providers::base::Provider;
use goose::utils::safe_truncate;

use anyhow::{Context, Result};
use completion::GooseCompleter;
use goose::agents::extension::{Envs, ExtensionConfig, PLATFORM_EXTENSIONS};
use goose::agents::types::RetryConfig;
use goose::agents::{Agent, SessionConfig, COMPACT_TRIGGERS};
use goose::config::extensions::name_to_key;
use goose::config::{Config, GooseMode};
use input::InputResult;
use rmcp::model::PromptMessage;
use rmcp::model::ServerNotification;
use rmcp::model::{ErrorCode, ErrorData};
use strum::VariantNames;

use goose::config::paths::Paths;
use goose::conversation::message::{ActionRequiredData, Message, MessageContent};
use rustyline::EditMode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio;
use tokio_util::sync::CancellationToken;
use tracing::warn;

#[derive(Serialize, Deserialize, Debug)]
struct JsonOutput {
    messages: Vec<Message>,
    metadata: JsonMetadata,
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonMetadata {
    total_tokens: Option<i32>,
    status: String,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEvent {
    Message {
        message: Message,
    },
    Notification {
        extension_id: String,
        #[serde(flatten)]
        data: NotificationData,
    },
    Error {
        error: String,
    },
    Complete {
        total_tokens: Option<i32>,
    },
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
enum NotificationData {
    Log {
        message: String,
    },
    Progress {
        progress: f64,
        total: Option<f64>,
        message: Option<String>,
    },
}

pub enum RunMode {
    Normal,
    Plan,
}

struct HistoryManager {
    history_file: PathBuf,
    old_history_file: PathBuf,
}

impl HistoryManager {
    fn new() -> Self {
        Self {
            history_file: Paths::state_dir().join("history.txt"),
            old_history_file: Paths::config_dir().join("history.txt"),
        }
    }

    fn load(
        &self,
        editor: &mut rustyline::Editor<GooseCompleter, rustyline::history::DefaultHistory>,
    ) {
        if let Some(parent) = self.history_file.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!("Warning: Failed to create history directory: {}", e);
                }
            }
        }

        let history_files = [&self.history_file, &self.old_history_file];
        if let Some(file) = history_files.iter().find(|f| f.exists()) {
            if let Err(err) = editor.load_history(file) {
                eprintln!("Warning: Failed to load command history: {}", err);
            }
        }
    }

    fn save(
        &self,
        editor: &mut rustyline::Editor<GooseCompleter, rustyline::history::DefaultHistory>,
    ) {
        if let Err(err) = editor.save_history(&self.history_file) {
            eprintln!("Warning: Failed to save command history: {}", err);
        } else if self.old_history_file.exists() {
            if let Err(err) = std::fs::remove_file(&self.old_history_file) {
                eprintln!("Warning: Failed to remove old history file: {}", err);
            }
        }
    }
}

pub struct CliSession {
    agent: Agent,
    messages: Conversation,
    session_id: String,
    completion_cache: Arc<std::sync::RwLock<CompletionCache>>,
    debug: bool,
    run_mode: RunMode,
    scheduled_job_id: Option<String>,
    max_turns: Option<u32>,
    edit_mode: Option<EditMode>,
    retry_config: Option<RetryConfig>,
    output_format: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintStatus {
    Default,
    Interrupted,
    MaybeExit,
}

// Cache structure for completion data
pub struct CompletionCache {
    pub prompts: HashMap<String, Vec<String>>,
    pub prompt_info: HashMap<String, output::PromptInfo>,
    pub last_updated: Instant,
    pub hint_status: HintStatus,
}

impl CompletionCache {
    fn new() -> Self {
        Self {
            prompts: HashMap::new(),
            prompt_info: HashMap::new(),
            last_updated: Instant::now(),
            hint_status: HintStatus::Default,
        }
    }
}

pub enum PlannerResponseType {
    Plan,
    ClarifyingQuestions,
}

/// Decide if the planner's response is a plan or a clarifying question
///
/// This function is called after the planner has generated a response
/// to the user's message. The response is either a plan or a clarifying
/// question.
pub async fn classify_planner_response(
    session_id: &str,
    message_text: String,
    provider: Arc<dyn Provider>,
) -> Result<PlannerResponseType> {
    let prompt = format!(
        "The text below is the output from an AI model which can either provide a plan or list of clarifying questions. Based on the text below, decide if the output is a \"plan\" or \"clarifying questions\".\n---\n{message_text}"
    );

    let message = Message::user().with_text(&prompt);
    let model_config = provider.get_model_config();
    let (result, _usage) = provider
        .complete(
            &model_config,
            session_id,
            "Reply only with the classification label: \"plan\" or \"clarifying questions\"",
            &[message],
            &[],
        )
        .await?;

    let predicted = result.as_concat_text();
    if predicted.to_lowercase().contains("plan") {
        Ok(PlannerResponseType::Plan)
    } else {
        Ok(PlannerResponseType::ClarifyingQuestions)
    }
}

pub fn split_quoted(input: &str) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_double_quote = false;
    let mut in_single_quote = false;

    for c in input.chars() {
        match c {
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            c if c.is_whitespace() && !in_double_quote && !in_single_quote => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }

    if in_double_quote || in_single_quote {
        return Err(anyhow::anyhow!("Unmatched quote in command"));
    }

    if !current.is_empty() {
        parts.push(current);
    }

    Ok(parts)
}

impl CliSession {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        agent: Agent,
        session_id: String,
        debug: bool,
        scheduled_job_id: Option<String>,
        max_turns: Option<u32>,
        edit_mode: Option<EditMode>,
        retry_config: Option<RetryConfig>,
        output_format: String,
    ) -> Self {
        let messages = agent
            .config
            .session_manager
            .get_session(&session_id, true)
            .await
            .map(|session| session.conversation.unwrap_or_default())
            .unwrap();

        CliSession {
            agent,
            messages,
            session_id,
            completion_cache: Arc::new(std::sync::RwLock::new(CompletionCache::new())),
            debug,
            run_mode: RunMode::Normal,
            scheduled_job_id,
            max_turns,
            edit_mode,
            retry_config,
            output_format,
        }
    }

    pub fn session_id(&self) -> &String {
        &self.session_id
    }

    /// Parse a stdio extension command string into an ExtensionConfig
    /// Format: "ENV1=val1 ENV2=val2 command args..."
    pub fn parse_stdio_extension(extension_command: &str) -> Result<ExtensionConfig> {
        let mut parts = split_quoted(extension_command)?;
        let mut envs = HashMap::new();

        while let Some(part) = parts.first() {
            if !part.contains('=') {
                break;
            }
            let env_part = parts.remove(0);
            let (key, value) = env_part.split_once('=').unwrap();
            envs.insert(key.to_string(), value.to_string());
        }

        if parts.is_empty() {
            return Err(anyhow::anyhow!("No command provided in extension string"));
        }

        let cmd = parts.remove(0);
        let name = std::path::Path::new(&cmd)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("unnamed")
            .to_string();

        Ok(ExtensionConfig::Stdio {
            name,
            cmd,
            args: parts,
            envs: Envs::new(envs),
            env_keys: Vec::new(),
            description: goose::config::DEFAULT_EXTENSION_DESCRIPTION.to_string(),
            timeout: Some(goose::config::DEFAULT_EXTENSION_TIMEOUT),
            bundled: None,
            available_tools: Vec::new(),
        })
    }

    pub fn parse_streamable_http_extension(extension_url: &str, timeout: u64) -> ExtensionConfig {
        let name = url::Url::parse(extension_url)
            .ok()
            .map(|u| {
                let mut s = String::new();
                if let Some(host) = u.host_str() {
                    s.push_str(host);
                }
                if let Some(port) = u.port() {
                    s.push('_');
                    s.push_str(&port.to_string());
                }
                let path = u.path().trim_matches('/');
                if !path.is_empty() {
                    s.push('_');
                    s.push_str(path);
                }
                name_to_key(&s)
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unnamed".to_string());

        ExtensionConfig::StreamableHttp {
            name,
            uri: extension_url.to_string(),
            envs: Envs::new(HashMap::new()),
            env_keys: Vec::new(),
            headers: HashMap::new(),
            description: goose::config::DEFAULT_EXTENSION_DESCRIPTION.to_string(),
            timeout: Some(timeout),
            socket: None,
            bundled: None,
            available_tools: Vec::new(),
        }
    }

    /// Parse builtin extension names (comma-separated) into ExtensionConfigs
    pub fn parse_builtin_extensions(builtin_name: &str) -> Vec<ExtensionConfig> {
        builtin_name
            .split(',')
            .map(|name| {
                let extension_name = name.trim();
                if PLATFORM_EXTENSIONS.contains_key(extension_name) {
                    ExtensionConfig::Platform {
                        name: extension_name.to_string(),
                        description: extension_name.to_string(),
                        display_name: None,
                        bundled: None,
                        available_tools: Vec::new(),
                    }
                } else {
                    ExtensionConfig::Builtin {
                        name: extension_name.to_string(),
                        display_name: None,
                        timeout: None,
                        bundled: None,
                        description: extension_name.to_string(),
                        available_tools: Vec::new(),
                    }
                }
            })
            .collect()
    }

    async fn add_and_persist_extensions(&mut self, configs: Vec<ExtensionConfig>) -> Result<()> {
        for config in configs {
            self.agent
                .add_extension(config, &self.session_id)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to start extension: {}", e))?;
        }

        self.invalidate_completion_cache().await;

        Ok(())
    }

    pub async fn add_extension(&mut self, extension_command: String) -> Result<()> {
        let config = Self::parse_stdio_extension(&extension_command)?;
        self.add_and_persist_extensions(vec![config]).await
    }

    pub async fn add_streamable_http_extension(&mut self, extension_url: String) -> Result<()> {
        let config = Self::parse_streamable_http_extension(
            &extension_url,
            goose::config::DEFAULT_EXTENSION_TIMEOUT,
        );
        self.add_and_persist_extensions(vec![config]).await
    }

    pub async fn add_builtin(&mut self, builtin_name: String) -> Result<()> {
        let configs = Self::parse_builtin_extensions(&builtin_name);
        self.add_and_persist_extensions(configs).await
    }

    pub async fn list_prompts(
        &mut self,
        extension: Option<String>,
    ) -> Result<HashMap<String, Vec<String>>> {
        let prompts = self.agent.list_extension_prompts(&self.session_id).await;

        // Early validation if filtering by extension
        if let Some(filter) = &extension {
            if !prompts.contains_key(filter) {
                return Err(anyhow::anyhow!("Extension '{}' not found", filter));
            }
        }

        // Convert prompts into filtered map of extension names to prompt names
        Ok(prompts
            .into_iter()
            .filter(|(ext, _)| extension.as_ref().is_none_or(|f| f == ext))
            .map(|(extension, prompt_list)| {
                let names = prompt_list.into_iter().map(|p| p.name).collect();
                (extension, names)
            })
            .collect())
    }

    pub async fn get_prompt_info(&mut self, name: &str) -> Result<Option<output::PromptInfo>> {
        let prompts = self.agent.list_extension_prompts(&self.session_id).await;

        // Find which extension has this prompt
        for (extension, prompt_list) in prompts {
            if let Some(prompt) = prompt_list.iter().find(|p| p.name == name) {
                return Ok(Some(output::PromptInfo {
                    name: prompt.name.clone(),
                    description: prompt.description.clone(),
                    arguments: prompt.arguments.clone(),
                    extension: Some(extension),
                }));
            }
        }

        Ok(None)
    }

    pub async fn get_prompt(&mut self, name: &str, arguments: Value) -> Result<Vec<PromptMessage>> {
        Ok(self
            .agent
            .get_prompt(&self.session_id, name, arguments)
            .await?
            .messages)
    }

    /// Process a single message and get the response
    pub(crate) async fn process_message(
        &mut self,
        message: Message,
        cancel_token: CancellationToken,
        interactive: bool,
    ) -> Result<()> {
        let cancel_token = cancel_token.clone();
        self.push_message(message);
        self.process_agent_response(interactive, cancel_token)
            .await?;
        Ok(())
    }

    /// Start an interactive session, optionally with an initial message
    pub async fn interactive(&mut self, prompt: Option<String>) -> Result<()> {
        if let Some(prompt) = prompt {
            let msg = Message::user().with_text(&prompt);
            self.process_message(msg, CancellationToken::default(), true)
                .await?;
        }

        self.update_completion_cache().await?;

        let mut editor = self.create_editor()?;
        let history_manager = HistoryManager::new();
        history_manager.load(&mut editor);

        loop {
            self.display_context_usage().await?;

            let conversation_strings: Vec<String> = self
                .messages
                .iter()
                .map(|msg| {
                    let role = match msg.role {
                        rmcp::model::Role::User => "User",
                        rmcp::model::Role::Assistant => "Assistant",
                    };
                    format!("## {}: {}", role, msg.as_concat_text())
                })
                .collect();

            output::run_status_hook("waiting");
            let input = input::get_input(&mut editor, Some(&conversation_strings))?;
            if matches!(input, InputResult::Exit) {
                break;
            }
            self.handle_input(input, &history_manager, &mut editor, &conversation_strings)
                .await?;
        }

        println!(
            "\n  {} {}",
            console::style("●").red(),
            console::style(format!("session closed · {}", &self.session_id)).dim()
        );

        Ok(())
    }

    fn create_editor(
        &self,
    ) -> Result<rustyline::Editor<GooseCompleter, rustyline::history::DefaultHistory>> {
        let builder =
            rustyline::Config::builder().completion_type(rustyline::CompletionType::Circular);
        let builder = match self.edit_mode {
            Some(mode) => builder.edit_mode(mode),
            None => builder.edit_mode(EditMode::Emacs),
        };
        let config = builder.build();
        let mut editor =
            rustyline::Editor::<GooseCompleter, rustyline::history::DefaultHistory>::with_config(
                config,
            )?;
        let completer = GooseCompleter::new(self.completion_cache.clone());
        editor.set_helper(Some(completer));
        Ok(editor)
    }

    async fn handle_input(
        &mut self,
        input: InputResult,
        history: &HistoryManager,
        editor: &mut rustyline::Editor<GooseCompleter, rustyline::history::DefaultHistory>,
        conversation_messages: &[String],
    ) -> Result<()> {
        match input {
            InputResult::Message(content) => {
                self.handle_message_input(&content, history, editor).await?;
            }
            InputResult::Exit => unreachable!("Exit is handled in the main loop"),
            InputResult::AddExtension(cmd) => {
                history.save(editor);
                match self.add_extension(cmd.clone()).await {
                    Ok(_) => output::render_extension_success(&cmd),
                    Err(e) => output::render_extension_error(&cmd, &e.to_string()),
                }
            }
            InputResult::AddBuiltin(names) => {
                history.save(editor);
                match self.add_builtin(names.clone()).await {
                    Ok(_) => output::render_builtin_success(&names),
                    Err(e) => output::render_builtin_error(&names, &e.to_string()),
                }
            }
            InputResult::ToggleTheme => {
                history.save(editor);
                self.handle_toggle_theme();
            }
            InputResult::ToggleFullToolOutput => {
                history.save(editor);
                self.handle_toggle_full_tool_output();
            }
            InputResult::SelectTheme(theme_name) => {
                history.save(editor);
                self.handle_select_theme(&theme_name);
            }
            InputResult::Retry => {}
            InputResult::ListPrompts(extension) => {
                history.save(editor);
                match self.list_prompts(extension).await {
                    Ok(prompts) => output::render_prompts(&prompts),
                    Err(e) => output::render_error(&e.to_string()),
                }
            }
            InputResult::GooseMode(mode) => {
                history.save(editor);
                self.handle_goose_mode(&mode).await?;
            }
            InputResult::Plan(options) => {
                self.handle_plan_mode(options).await?;
            }
            InputResult::EndPlan => {
                self.run_mode = RunMode::Normal;
                output::render_exit_plan_mode();
            }
            InputResult::Clear => {
                history.save(editor);
                self.handle_clear().await?;
            }
            InputResult::PromptCommand(opts) => {
                history.save(editor);
                self.handle_prompt_command(opts).await?;
            }
            InputResult::Recipe(filepath_opt) => {
                history.save(editor);
                self.handle_recipe(filepath_opt).await;
            }
            InputResult::Compact => {
                history.save(editor);
                self.handle_compact().await?;
            }
            InputResult::Edit(prefill) => {
                history.save(editor);
                match crate::session::editor::resolve_editor_command() {
                    Some(editor_cmd) => {
                        let messages: Vec<&str> =
                            conversation_messages.iter().map(|s| s.as_str()).collect();
                        match crate::session::editor::get_editor_input(
                            &editor_cmd,
                            &messages,
                            prefill.as_deref(),
                        ) {
                            Ok((message, true)) => {
                                self.handle_message_input(&message, history, editor).await?;
                            }
                            Ok((_, false)) => {}
                            Err(e) => {
                                output::render_error(&format!("Failed to open editor: {}", e));
                            }
                        }
                    }
                    None => {
                        output::render_error(
                            "No editor found. Set one with:\n  \
                                 goose configure set goose_prompt_editor \"vim\"\n  \
                                 or set $VISUAL or $EDITOR in your shell.",
                        );
                    }
                }
            }
            InputResult::LoadSkills(names) => {
                history.save(editor);
                self.handle_load_skills(&names).await?;
            }
            InputResult::ListSkills => {
                history.save(editor);
                self.handle_list_skills().await?;
            }
        }
        Ok(())
    }

    async fn handle_message_input(
        &mut self,
        content: &str,
        history: &HistoryManager,
        editor: &mut rustyline::Editor<GooseCompleter, rustyline::history::DefaultHistory>,
    ) -> Result<()> {
        match self.run_mode {
            RunMode::Normal => {
                history.save(editor);
                self.push_message(Message::user().with_text(content));

                if let Err(e) = crate::project_tracker::update_project_tracker(
                    Some(content),
                    Some(&self.session_id),
                ) {
                    eprintln!(
                        "Warning: Failed to update project tracker with instruction: {}",
                        e
                    );
                }

                let _provider = self.agent.provider().await?;

                println!();
                output::run_status_hook("thinking");
                output::show_thinking();
                let start_time = Instant::now();
                self.process_agent_response(true, CancellationToken::default())
                    .await?;
                output::hide_thinking();

                let elapsed = start_time.elapsed();
                let elapsed_str = format_elapsed_time(elapsed);
                println!("{}", console::style(format!("  ⏱ {}", elapsed_str)).dim());
            }
            RunMode::Plan => {
                let mut plan_messages = self.messages.clone();
                plan_messages.push(Message::user().with_text(content));
                let reasoner = get_reasoner().await?;
                self.plan_with_reasoner_model(plan_messages, reasoner)
                    .await?;
            }
        }
        Ok(())
    }

    fn handle_toggle_theme(&self) {
        let current = output::get_theme();
        let new_theme = match current {
            output::Theme::Ansi => {
                println!("Switching to Light theme");
                output::Theme::Light
            }
            output::Theme::Light => {
                println!("Switching to Dark theme");
                output::Theme::Dark
            }
            output::Theme::Dark => {
                println!("Switching to Ansi theme");
                output::Theme::Ansi
            }
        };
        output::set_theme(new_theme);
    }

    fn handle_select_theme(&self, theme_name: &str) {
        let new_theme = match theme_name {
            "light" => {
                println!("Switching to Light theme");
                output::Theme::Light
            }
            "dark" => {
                println!("Switching to Dark theme");
                output::Theme::Dark
            }
            "ansi" => {
                println!("Switching to Ansi theme");
                output::Theme::Ansi
            }
            _ => output::Theme::Dark,
        };
        output::set_theme(new_theme);
    }

    fn handle_toggle_full_tool_output(&self) {
        let enabled = output::toggle_full_tool_output();
        if enabled {
            println!(
                "{}",
                console::style(
                    "✓ Full tool output enabled - tool parameters will no longer be truncated"
                )
                .green()
            );
        } else {
            println!(
                "{}",
                console::style(
                    "✓ Full tool output disabled - tool parameters will be truncated to fit terminal width"
                )
                .dim()
            );
        }
    }

    async fn handle_goose_mode(&self, mode: &str) -> Result<()> {
        let config = Config::global();
        let mode = match GooseMode::from_str(&mode.to_lowercase()) {
            Ok(mode) => mode,
            Err(_) => {
                output::render_error(&format!(
                    "Invalid mode '{mode}'. Mode must be one of: {}",
                    GooseMode::VARIANTS.join(", ")
                ));
                return Ok(());
            }
        };
        self.agent.update_goose_mode(mode, &self.session_id).await?;
        config.set_goose_mode(mode)?;
        output::goose_mode_message(&format!("Goose mode set to '{mode}'"));
        Ok(())
    }

    async fn handle_plan_mode(&mut self, options: input::PlanCommandOptions) -> Result<()> {
        self.run_mode = RunMode::Plan;
        output::render_enter_plan_mode();

        if options.message_text.is_empty() {
            return Ok(());
        }

        let mut plan_messages = self.messages.clone();
        plan_messages.push(Message::user().with_text(&options.message_text));

        let reasoner = get_reasoner().await?;
        self.plan_with_reasoner_model(plan_messages, reasoner).await
    }

    async fn handle_clear(&mut self) -> Result<()> {
        if let Err(e) = self
            .agent
            .config
            .session_manager
            .replace_conversation(&self.session_id, &Conversation::default())
            .await
        {
            output::render_error(&format!("Failed to clear session: {}", e));
            return Ok(());
        }

        if let Err(e) = self
            .agent
            .config
            .session_manager
            .update(&self.session_id)
            .total_tokens(Some(0))
            .input_tokens(Some(0))
            .output_tokens(Some(0))
            .apply()
            .await
        {
            output::render_error(&format!("Failed to reset token counts: {}", e));
            return Ok(());
        }

        self.messages.clear();
        tracing::info!("Chat context cleared by user.");
        output::render_message(
            &Message::assistant().with_text("Chat context cleared.\n"),
            self.debug,
        );
        Ok(())
    }

    async fn handle_recipe(&mut self, filepath_opt: Option<String>) {
        println!("{}", console::style("Generating Recipe").green());

        output::show_thinking();
        let recipe = self
            .agent
            .create_recipe(&self.session_id, self.messages.clone())
            .await;
        output::hide_thinking();

        match recipe {
            Ok(recipe) => {
                let filepath_str = filepath_opt.as_deref().unwrap_or("recipe.yaml");
                match self.save_recipe(&recipe, filepath_str) {
                    Ok(path) => println!(
                        "{}",
                        console::style(format!("Saved recipe to {}", path.display())).green()
                    ),
                    Err(e) => println!("{}", console::style(e).red()),
                }
            }
            Err(e) => {
                println!(
                    "{}: {:?}",
                    console::style("Failed to generate recipe").red(),
                    e
                );
            }
        }
    }

    async fn handle_load_skills(&mut self, names: &[String]) -> Result<()> {
        // NOTE: We don't validate the skill names here because the load_skill tool will
        // handle that and provide feedback to the user if any skill names are invalid.
        let message = format!(
            "Use the load_skill tool to load the following skills: {}.",
            names
                .iter()
                .map(|n| format!("\"{}\"", n))
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.push_message(Message::user().with_text(&message));
        output::show_thinking();
        let result = self
            .process_agent_response(true, CancellationToken::default())
            .await;
        output::hide_thinking();
        result?;

        Ok(())
    }

    async fn handle_list_skills(&mut self) -> Result<()> {
        use comfy_table::{presets, Cell, ContentArrangement, Table};
        use goose::agents::platform_extensions::skills::list_installed_skills;
        let cwd = std::env::current_dir().unwrap_or_default();
        let skills = list_installed_skills(Some(&cwd));

        if skills.is_empty() {
            println!("{}", console::style("No skills available.").yellow());
            return Ok(());
        }

        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.load_preset(presets::ASCII_FULL);
        table.set_header(vec!["Skill", "Description"]);

        let mut sorted_skills = skills;
        sorted_skills.sort_by(|a, b| a.name.cmp(&b.name));

        for skill in &sorted_skills {
            table.add_row(vec![Cell::new(&skill.name), Cell::new(&skill.description)]);
        }

        println!("{table}");
        Ok(())
    }

    async fn handle_compact(&mut self) -> Result<()> {
        let prompt = "Are you sure you want to compact this conversation? This will condense the message history.";
        let should_summarize = match cliclack::confirm(prompt).initial_value(true).interact() {
            Ok(choice) => choice,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    false
                } else {
                    return Err(e.into());
                }
            }
        };

        if should_summarize {
            self.push_message(Message::user().with_text(COMPACT_TRIGGERS[0]));
            output::show_thinking();
            self.process_agent_response(true, CancellationToken::default())
                .await?;
            output::hide_thinking();
        } else {
            println!("{}", console::style("Compaction cancelled.").yellow());
        }
        Ok(())
    }

    async fn plan_with_reasoner_model(
        &mut self,
        plan_messages: Conversation,
        reasoner: Arc<dyn Provider>,
    ) -> Result<(), anyhow::Error> {
        let plan_prompt = self.agent.get_plan_prompt(&self.session_id).await?;
        output::show_thinking();
        let model_config = reasoner.get_model_config();
        let (plan_response, _usage) = reasoner
            .complete(
                &model_config,
                &self.session_id,
                &plan_prompt,
                plan_messages.messages(),
                &[],
            )
            .await?;
        output::render_message(&plan_response, self.debug);
        output::hide_thinking();
        let planner_response_type = classify_planner_response(
            &self.session_id,
            plan_response.as_concat_text(),
            self.agent.provider().await?,
        )
        .await?;

        match planner_response_type {
            PlannerResponseType::Plan => {
                println!();
                let should_act = match cliclack::confirm(
                    "Do you want to clear message history & act on this plan?",
                )
                .initial_value(true)
                .interact()
                {
                    Ok(choice) => choice,
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::Interrupted {
                            false // If interrupted, set should_act to false
                        } else {
                            return Err(e.into());
                        }
                    }
                };
                if should_act {
                    output::render_act_on_plan();
                    self.run_mode = RunMode::Normal;
                    // set goose mode: auto if that isn't already the case
                    let config = Config::global();
                    let curr_goose_mode = config.get_goose_mode().unwrap_or_default();
                    if curr_goose_mode != GooseMode::Auto {
                        config.set_goose_mode(GooseMode::Auto).unwrap();
                    }

                    // clear the messages before acting on the plan
                    self.messages.clear();
                    // add the plan response as a user message
                    let plan_message = Message::user().with_text(plan_response.as_concat_text());
                    self.push_message(plan_message);
                    // act on the plan
                    output::show_thinking();
                    self.process_agent_response(true, CancellationToken::default())
                        .await?;
                    output::hide_thinking();

                    // Reset run & goose mode
                    if curr_goose_mode != GooseMode::Auto {
                        config.set_goose_mode(curr_goose_mode)?;
                    }
                } else {
                    // add the plan response (assistant message) & carry the conversation forward
                    // in the next round, the user might wanna slightly modify the plan
                    self.push_message(plan_response);
                }
            }
            PlannerResponseType::ClarifyingQuestions => {
                // add the plan response (assistant message) & carry the conversation forward
                // in the next round, the user will answer the clarifying questions
                self.push_message(plan_response);
            }
        }

        Ok(())
    }

    /// Process a single message and exit
    pub async fn headless(&mut self, prompt: String) -> Result<()> {
        let message = Message::user().with_text(&prompt);
        self.process_message(message, CancellationToken::default(), false)
            .await?;
        Ok(())
    }

    async fn process_agent_response(
        &mut self,
        interactive: bool,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        let is_json_mode = self.output_format == "json";
        let is_stream_json_mode = self.output_format == "stream-json";

        let session_config = SessionConfig {
            id: self.session_id.clone(),
            schedule_id: self.scheduled_job_id.clone(),
            max_turns: self.max_turns,
            retry_config: self.retry_config.clone(),
        };
        let user_message = self
            .messages
            .last()
            .ok_or_else(|| anyhow::anyhow!("No user message"))?;

        let cancel_token_interrupt = cancel_token.clone();
        let handle = tokio::spawn(async move {
            if ctrl_c().await.is_ok() {
                cancel_token_interrupt.cancel();
            }
        });
        let _drop_handle = AbortOnDropHandle::new(handle);

        let mut stream = self
            .agent
            .reply(
                user_message.clone(),
                session_config.clone(),
                Some(cancel_token.clone()),
            )
            .await?;

        let mut progress_bars = output::McpSpinners::new();
        let cancel_token_clone = cancel_token.clone();
        let mut markdown_buffer = streaming_buffer::MarkdownBuffer::new();
        let mut prompted_credits_urls: HashSet<String> = HashSet::new();
        let mut thinking_header_shown = false;

        use futures::StreamExt;
        loop {
            tokio::select! {
                result = stream.next() => {
                    match result {
                        Some(Ok(AgentEvent::Message(message))) => {
                            if let Some((id, security_prompt)) = find_tool_confirmation(&message) {
                                let permission = if interactive {
                                    prompt_tool_confirmation(&security_prompt)?
                                } else {
                                    // Non-interactive/headless mode: refuse to run in
                                    // Approve/SmartApprove modes since auto-allowing would
                                    // bypass the safety contract those modes are meant to enforce.
                                    let config = Config::global();
                                    let goose_mode = config.get_goose_mode().unwrap_or(GooseMode::Auto);
                                    if goose_mode == GooseMode::Approve || goose_mode == GooseMode::SmartApprove {
                                        cancel_token_clone.cancel();
                                        drop(stream);
                                        return Err(anyhow::anyhow!(
                                            "Tool approval required in non-interactive mode with GooseMode::{goose_mode}. \
                                             This is an invalid configuration — Approve/SmartApprove modes require an \
                                             interactive terminal. Use GooseMode::Auto for headless sessions."
                                        ));
                                    }
                                    tracing::warn!(
                                        "Tool confirmation required in non-interactive mode, auto-allowing"
                                    );
                                    Permission::AllowOnce
                                };

                                if permission == Permission::Cancel {
                                    output::render_text("Tool call cancelled. Returning to chat...", Some(Color::Yellow), true);
                                    self.agent.handle_confirmation(id.clone(), PermissionConfirmation {
                                        principal_type: PrincipalType::Tool,
                                        permission: Permission::DenyOnce,
                                    }).await;
                                    let mut response_message = Message::user();
                                    response_message.content.push(MessageContent::tool_response(
                                        id,
                                        Err(ErrorData {
                                            code: ErrorCode::INVALID_REQUEST,
                                            message: std::borrow::Cow::from("Tool call cancelled by user"),
                                            data: None,
                                        }),
                                    ));
                                    self.messages.push(response_message);
                                    cancel_token_clone.cancel();
                                    drop(stream);
                                    break;
                                }
                                self.agent.handle_confirmation(id, PermissionConfirmation {
                                    principal_type: PrincipalType::Tool,
                                    permission,
                                }).await;
                            } else if let Some((elicitation_id, elicitation_message, schema)) = find_elicitation_request(&message) {
                                if !interactive {
                                    // Non-interactive/headless mode: cannot collect user input
                                    tracing::warn!(
                                        "Elicitation requested in non-interactive mode, cancelling"
                                    );
                                    cancel_token_clone.cancel();
                                    drop(stream);
                                    return Err(anyhow::anyhow!(
                                        "Elicitation requested but no interactive terminal is available to collect user input"
                                    ));
                                }

                                output::hide_thinking();
                                let _ = progress_bars.hide();

                                match elicitation::collect_elicitation_input(&elicitation_message, &schema) {
                                    Ok(Some(user_data)) => {
                                        let user_data_value = serde_json::to_value(user_data)
                                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                                        let response_message = Message::user()
                                            .with_content(MessageContent::action_required_elicitation_response(
                                                elicitation_id,
                                                user_data_value,
                                            ))
                                            .with_visibility(false, true);
                                        self.messages.push(response_message.clone());
                                        // Elicitation responses return an empty stream - the response
                                        // unblocks the waiting tool call via ActionRequiredManager
                                        let _ = self.agent.reply(response_message, session_config.clone(), Some(cancel_token.clone())).await?;
                                    }
                                    Ok(None) => {
                                        output::render_text("Information request cancelled.", Some(Color::Yellow), true);
                                        cancel_token_clone.cancel();
                                        drop(stream);
                                        break;
                                    }
                                    Err(e) => {
                                        output::render_error(&format!("Failed to collect input: {}", e));
                                        cancel_token_clone.cancel();
                                        drop(stream);
                                        break;
                                    }
                                }
                            } else {
                                log_tool_metrics(&message, &self.messages);
                                self.messages.push(message.clone());

                                if interactive { output::hide_thinking() };
                                let _ = progress_bars.hide();

                                if is_stream_json_mode {
                                    emit_stream_event(&StreamEvent::Message { message: message.clone() });
                                } else if !is_json_mode {
                                    output::render_message_streaming(&message, &mut markdown_buffer, &mut thinking_header_shown, self.debug);
                                    maybe_open_credits_top_up_url(
                                        &message,
                                        interactive,
                                        &mut prompted_credits_urls,
                                    );
                                }
                            }
                        }
                        Some(Ok(AgentEvent::McpNotification((extension_id, notification)))) => {
                            handle_mcp_notification(
                                &extension_id,
                                &notification,
                                &mut progress_bars,
                                is_stream_json_mode,
                                interactive,
                                is_json_mode,
                                self.debug,
                            );
                        }
                        Some(Ok(AgentEvent::HistoryReplaced(updated_conversation))) => {
                            self.messages = updated_conversation;
                        }
                        Some(Err(e)) => {
                            handle_agent_error(&e, is_stream_json_mode);
                            cancel_token_clone.cancel();
                            drop(stream);
                            if let Err(e) = self.handle_interrupted_messages(false).await {
                                eprintln!("Error handling interruption: {}", e);
                            } else if !is_stream_json_mode {
                                output::render_error(
                                    "The error above was an exception we were not able to handle.\n\
                                    These errors are often related to connection or authentication\n\
                                    We've removed the conversation up to the most recent user message\n\
                                    - depending on the error you may be able to continue",
                                );
                            }
                            break;
                        }
                        None => break,
                    }
                }
                _ = cancel_token_clone.cancelled() => {
                    drop(stream);
                    if let Err(e) = self.handle_interrupted_messages(true).await {
                        eprintln!("Error handling interruption: {}", e);
                    }
                    break;
                }
            }
        }

        if !is_json_mode && !is_stream_json_mode {
            output::flush_markdown_buffer_current_theme(&mut markdown_buffer);
        }

        if is_json_mode {
            let metadata = match self
                .agent
                .config
                .session_manager
                .get_session(&self.session_id, false)
                .await
            {
                Ok(session) => JsonMetadata {
                    total_tokens: session.total_tokens,
                    status: "completed".to_string(),
                },
                Err(_) => JsonMetadata {
                    total_tokens: None,
                    status: "completed".to_string(),
                },
            };
            let json_output = JsonOutput {
                messages: self.messages.messages().to_vec(),
                metadata,
            };
            println!("{}", serde_json::to_string_pretty(&json_output)?);
        } else if is_stream_json_mode {
            let total_tokens = self
                .agent
                .config
                .session_manager
                .get_session(&self.session_id, false)
                .await
                .ok()
                .and_then(|s| s.total_tokens);
            emit_stream_event(&StreamEvent::Complete { total_tokens });
        } else {
            println!();
        }

        Ok(())
    }

    async fn handle_interrupted_messages(&mut self, interrupt: bool) -> Result<()> {
        if interrupt {
            let mut cache = self.completion_cache.write().unwrap();
            cache.hint_status = HintStatus::Interrupted;
        }

        let tool_requests = self
            .messages
            .last()
            .filter(|msg| msg.role == rmcp::model::Role::Assistant)
            .map_or(Vec::new(), |msg| {
                msg.content
                    .iter()
                    .filter_map(|content| {
                        if let MessageContent::ToolRequest(req) = content {
                            Some((req.id.clone(), req.tool_call.clone()))
                        } else {
                            None
                        }
                    })
                    .collect()
            });

        let interrupt_prompt = "Yes — what would you like me to do?";

        if !tool_requests.is_empty() {
            let mut response_message = Message::user();

            let notification = if interrupt {
                "Interrupted by the user to make a correction".to_string()
            } else {
                "An uncaught error happened during tool use".to_string()
            };
            for (req_id, _) in &tool_requests {
                response_message.content.push(MessageContent::tool_response(
                    req_id.clone(),
                    Err(ErrorData {
                        code: ErrorCode::INTERNAL_ERROR,
                        message: std::borrow::Cow::from(notification.clone()),
                        data: None,
                    }),
                ));
            }
            self.push_message(response_message);
            self.push_message(Message::assistant().with_text(interrupt_prompt));
            output::render_message(
                &Message::assistant().with_text(interrupt_prompt),
                self.debug,
            );
        } else if let Some(last_msg) = self.messages.last() {
            if last_msg.role == rmcp::model::Role::User {
                match last_msg.content.first() {
                    Some(MessageContent::ToolResponse(_)) => {
                        self.push_message(Message::assistant().with_text(interrupt_prompt));
                        output::render_message(
                            &Message::assistant().with_text(interrupt_prompt),
                            self.debug,
                        );
                    }
                    Some(_) => {
                        self.messages.pop();
                        let assistant_msg = Message::assistant().with_text(interrupt_prompt);
                        self.push_message(assistant_msg.clone());
                        output::render_message(&assistant_msg, self.debug);
                    }
                    None => {
                        // Empty message content — nothing to do, just continue gracefully
                    }
                }
            }
        }
        Ok(())
    }

    /// Update the completion cache with fresh data
    /// This should be called before the interactive session starts
    pub async fn update_completion_cache(&mut self) -> Result<()> {
        // Get fresh data
        let prompts = self.agent.list_extension_prompts(&self.session_id).await;

        // Update the cache with write lock
        let mut cache = self.completion_cache.write().unwrap();
        cache.prompts.clear();
        cache.prompt_info.clear();

        for (extension, prompt_list) in prompts {
            let names: Vec<String> = prompt_list.iter().map(|p| p.name.clone()).collect();
            cache.prompts.insert(extension.clone(), names);

            for prompt in prompt_list {
                cache.prompt_info.insert(
                    prompt.name.clone(),
                    output::PromptInfo {
                        name: prompt.name.clone(),
                        description: prompt.description.clone(),
                        arguments: prompt.arguments.clone(),
                        extension: Some(extension.clone()),
                    },
                );
            }
        }

        cache.last_updated = Instant::now();
        Ok(())
    }

    /// Invalidate the completion cache
    /// This should be called when extensions are added or removed
    async fn invalidate_completion_cache(&self) {
        let mut cache = self.completion_cache.write().unwrap();
        cache.prompts.clear();
        cache.prompt_info.clear();
        cache.last_updated = Instant::now();
    }

    pub fn message_history(&self) -> Conversation {
        self.messages.clone()
    }

    /// Render all past messages from the session history
    pub fn render_message_history(&self) {
        if self.messages.is_empty() {
            return;
        }

        println!(
            "\n  {} {}",
            console::style("↻").cyan(),
            console::style(format!("{} messages restored", self.messages.len())).dim()
        );

        // Render each message
        for message in self.messages.iter() {
            output::render_message(message, self.debug);
        }

        println!();
    }

    pub async fn get_session(&self) -> Result<goose::session::Session> {
        self.agent
            .config
            .session_manager
            .get_session(&self.session_id, false)
            .await
    }

    // Get the session's total token usage
    pub async fn get_total_token_usage(&self) -> Result<Option<i32>> {
        let metadata = self.get_session().await?;
        Ok(metadata.total_tokens)
    }

    /// Display enhanced context usage with session totals
    pub async fn display_context_usage(&self) -> Result<()> {
        let provider = self.agent.provider().await?;
        let model_config = provider.get_model_config();
        let context_limit = model_config.context_limit();

        let config = Config::global();
        let show_cost = config
            .get_param::<bool>("GOOSE_CLI_SHOW_COST")
            .unwrap_or(false);

        let provider_name = config
            .get_goose_provider()
            .unwrap_or_else(|_| "unknown".to_string());

        match self.get_session().await {
            Ok(metadata) => {
                let total_tokens = metadata.total_tokens.unwrap_or(0) as usize;

                output::display_context_usage(total_tokens, context_limit);

                if show_cost {
                    let input_tokens = metadata.input_tokens.unwrap_or(0) as usize;
                    let output_tokens = metadata.output_tokens.unwrap_or(0) as usize;
                    output::display_cost_usage(
                        &provider_name,
                        &model_config.model_name,
                        input_tokens,
                        output_tokens,
                    );
                }
            }
            Err(_) => {
                output::display_context_usage(0, context_limit);
            }
        }

        Ok(())
    }

    /// Handle prompt command execution
    async fn handle_prompt_command(&mut self, opts: input::PromptCommandOptions) -> Result<()> {
        // name is required
        if opts.name.is_empty() {
            output::render_error("Prompt name argument is required");
            return Ok(());
        }

        if opts.info {
            match self.get_prompt_info(&opts.name).await? {
                Some(info) => output::render_prompt_info(&info),
                None => output::render_error(&format!("Prompt '{}' not found", opts.name)),
            }
        } else {
            // Convert the arguments HashMap to a Value
            let arguments = serde_json::to_value(opts.arguments)
                .map_err(|e| anyhow::anyhow!("Failed to serialize arguments: {}", e))?;

            match self.get_prompt(&opts.name, arguments).await {
                Ok(messages) => {
                    let start_len = self.messages.len();
                    let mut valid = true;
                    let num_messages = messages.len();
                    for (i, prompt_message) in messages.into_iter().enumerate() {
                        let msg = Message::from(prompt_message);
                        // ensure we get a User - Assistant - User type pattern
                        let expected_role = if i % 2 == 0 {
                            rmcp::model::Role::User
                        } else {
                            rmcp::model::Role::Assistant
                        };

                        if msg.role != expected_role {
                            output::render_error(&format!(
                                "Expected {:?} message at position {}, but found {:?}",
                                expected_role, i, msg.role
                            ));
                            valid = false;
                            // get rid of everything we added to messages
                            self.messages.truncate(start_len);
                            break;
                        }

                        if msg.role == rmcp::model::Role::User {
                            output::render_message(&msg, self.debug);
                        }
                        self.push_message(msg);
                    }

                    if valid {
                        if num_messages > 1 {
                            for i in 0..(num_messages - 1) {
                                let msg = &self.messages.messages()[start_len + i];
                                self.agent
                                    .config
                                    .session_manager
                                    .add_message(&self.session_id, msg)
                                    .await?;
                            }
                        }

                        output::show_thinking();
                        self.process_agent_response(true, CancellationToken::default())
                            .await?;
                        output::hide_thinking();
                    }
                }
                Err(e) => output::render_error(&e.to_string()),
            }
        }

        Ok(())
    }

    /// Save a recipe to a file
    ///
    /// # Arguments
    /// * `recipe` - The recipe to save
    /// * `filepath_str` - The path to save the recipe to
    ///
    /// # Returns
    /// * `Result<PathBuf, String>` - The path the recipe was saved to or an error message
    fn save_recipe(
        &self,
        recipe: &goose::recipe::Recipe,
        filepath_str: &str,
    ) -> anyhow::Result<PathBuf> {
        let path_buf = PathBuf::from(filepath_str);
        let mut path = path_buf.clone();

        // Update the final path if it's relative
        if path_buf.is_relative() {
            // If the path is relative, resolve it relative to the current working directory
            let cwd = std::env::current_dir().context("Failed to get current directory")?;
            path = cwd.join(&path_buf);
        }

        // Check if parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                return Err(anyhow::anyhow!(
                    "Directory '{}' does not exist",
                    parent.display()
                ));
            }
        }

        // Try creating the file
        let file = std::fs::File::create(path.as_path())
            .context(format!("Failed to create file '{}'", path.display()))?;

        // Write YAML
        serde_yaml::to_writer(file, recipe).context("Failed to save recipe")?;

        Ok(path)
    }

    fn push_message(&mut self, message: Message) {
        self.messages.push(message);
    }
}

fn maybe_open_credits_top_up_url(
    message: &Message,
    interactive: bool,
    prompted_credits_urls: &mut HashSet<String>,
) {
    if !interactive || !std::io::stdout().is_terminal() {
        return;
    }

    let Some(url) = output::get_credits_top_up_url(message) else {
        return;
    };

    if !prompted_credits_urls.insert(url.clone()) {
        return;
    }

    let should_open = cliclack::confirm("Open the top-up URL in your browser?")
        .initial_value(false)
        .interact()
        .unwrap_or(false);

    if should_open && webbrowser::open(&url).is_err() {
        output::render_text(
            "Could not open browser automatically. Visit the URL above.",
            Some(Color::Yellow),
            true,
        );
    }
}

fn emit_stream_event(event: &StreamEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        println!("{}", json);
    }
}

/// Prompt user for tool call confirmation, returns the Permission selected
fn prompt_tool_confirmation(security_prompt: &Option<String>) -> Result<Permission> {
    output::hide_thinking();

    let prompt = if let Some(security_message) = security_prompt {
        println!("\n{}", security_message);
        "Do you allow this tool call?".to_string()
    } else {
        "Goose would like to call the above tool, do you allow?".to_string()
    };

    let permission_result = if security_prompt.is_none() {
        cliclack::select(prompt)
            .item(Permission::AllowOnce, "Allow", "Allow the tool call once")
            .item(
                Permission::AlwaysAllow,
                "Always Allow",
                "Always allow the tool call",
            )
            .item(Permission::DenyOnce, "Deny", "Deny the tool call")
            .item(
                Permission::Cancel,
                "Cancel",
                "Cancel the AI response and tool call",
            )
            .interact()
    } else {
        cliclack::select(prompt)
            .item(Permission::AllowOnce, "Allow", "Allow the tool call once")
            .item(Permission::DenyOnce, "Deny", "Deny the tool call")
            .item(
                Permission::Cancel,
                "Cancel",
                "Cancel the AI response and tool call",
            )
            .interact()
    };

    match permission_result {
        Ok(p) => Ok(p),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::Interrupted {
                Ok(Permission::Cancel)
            } else {
                Err(e.into())
            }
        }
    }
}

/// Extract tool confirmation request from a message
fn find_tool_confirmation(message: &Message) -> Option<(String, Option<String>)> {
    message.content.iter().find_map(|content| {
        if let MessageContent::ActionRequired(action) = content {
            if let ActionRequiredData::ToolConfirmation { id, prompt, .. } = &action.data {
                return Some((id.clone(), prompt.clone()));
            }
        }
        None
    })
}

/// Extract elicitation request from a message
fn find_elicitation_request(message: &Message) -> Option<(String, String, Value)> {
    message.content.iter().find_map(|content| {
        if let MessageContent::ActionRequired(action) = content {
            if let ActionRequiredData::Elicitation {
                id,
                message,
                requested_schema,
            } = &action.data
            {
                return Some((id.clone(), message.clone(), requested_schema.clone()));
            }
        }
        None
    })
}

/// Handle MCP notification event (logging or progress)
fn handle_mcp_notification(
    extension_id: &str,
    notification: &ServerNotification,
    progress_bars: &mut output::McpSpinners,
    is_stream_json_mode: bool,
    interactive: bool,
    is_json_mode: bool,
    debug: bool,
) {
    match notification {
        ServerNotification::LoggingMessageNotification(log_notif) => {
            if let Some(obj) = log_notif.params.data.as_object() {
                if obj.get("type").and_then(|v| v.as_str()) == Some(SUBAGENT_TOOL_REQUEST_TYPE) {
                    if let (Some(subagent_id), Some(tool_call)) = (
                        obj.get("subagent_id").and_then(|v| v.as_str()),
                        obj.get("tool_call").and_then(|v| v.as_object()),
                    ) {
                        let tool_name = tool_call
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let arguments = tool_call
                            .get("arguments")
                            .and_then(|v| v.as_object())
                            .cloned();

                        if interactive {
                            let _ = progress_bars.hide();
                        }
                        if is_stream_json_mode {
                            emit_stream_event(&StreamEvent::Notification {
                                extension_id: extension_id.to_string(),
                                data: NotificationData::Log {
                                    message: output::format_subagent_tool_call_message(
                                        subagent_id,
                                        tool_name,
                                    ),
                                },
                            });
                            return;
                        }
                        if !is_json_mode {
                            output::render_subagent_tool_call(
                                subagent_id,
                                tool_name,
                                arguments.as_ref(),
                                debug,
                            );
                            return;
                        }
                    }
                }
            }

            let (formatted, subagent_id, notif_type) =
                format_logging_notification(&log_notif.params.data, debug);

            if is_stream_json_mode {
                emit_stream_event(&StreamEvent::Notification {
                    extension_id: extension_id.to_string(),
                    data: NotificationData::Log {
                        message: formatted.clone(),
                    },
                });
            } else {
                display_log_notification(
                    &formatted,
                    subagent_id.as_deref(),
                    notif_type.as_deref(),
                    progress_bars,
                    interactive,
                    is_json_mode,
                );
            }
        }
        ServerNotification::ProgressNotification(prog_notif) => {
            if is_stream_json_mode {
                emit_stream_event(&StreamEvent::Notification {
                    extension_id: extension_id.to_string(),
                    data: NotificationData::Progress {
                        progress: prog_notif.params.progress,
                        total: prog_notif.params.total,
                        message: prog_notif.params.message.clone(),
                    },
                });
            } else {
                progress_bars.update(
                    &prog_notif.params.progress_token.0.to_string(),
                    prog_notif.params.progress,
                    prog_notif.params.total,
                    prog_notif.params.message.as_deref(),
                );
            }
        }
        _ => (),
    }
}

/// Format a logging notification from MCP, returns (formatted_message, subagent_id, notification_type)
fn format_logging_notification(
    data: &Value,
    debug: bool,
) -> (String, Option<String>, Option<String>) {
    match data {
        Value::String(s) => (s.clone(), None, None),
        Value::Object(o) => {
            if let Some(Value::String(msg)) = o.get("message") {
                let subagent_id = o.get("subagent_id").and_then(|v| v.as_str());
                let notification_type = o.get("type").and_then(|v| v.as_str());

                let formatted = match notification_type {
                    Some("subagent_created") | Some("completed") | Some("terminated") => {
                        format!("🤖 {}", msg)
                    }
                    Some("tool_usage") | Some("tool_completed") | Some("tool_error") => {
                        format!("🔧 {}", msg)
                    }
                    Some("message_processing") | Some("turn_progress") => {
                        format!("💭 {}", msg)
                    }
                    Some("response_generated") => {
                        let config = Config::global();
                        let min_priority = config
                            .get_param::<f32>("GOOSE_CLI_MIN_PRIORITY")
                            .ok()
                            .unwrap_or(output::DEFAULT_MIN_PRIORITY);

                        if min_priority > 0.1 && !debug {
                            if let Some(response_content) = msg.strip_prefix("Responded: ") {
                                format!("🤖 Responded: {}", safe_truncate(response_content, 100))
                            } else {
                                format!("🤖 {}", msg)
                            }
                        } else {
                            format!("🤖 {}", msg)
                        }
                    }
                    _ => msg.to_string(),
                };
                (
                    formatted,
                    subagent_id.map(str::to_string),
                    notification_type.map(str::to_string),
                )
            } else if let Some(Value::String(output)) = o.get("output") {
                let notification_type = o.get("type").and_then(|v| v.as_str()).map(str::to_string);
                (output.to_owned(), None, notification_type)
            } else if let Some(result) = format_task_execution_notification(data) {
                result
            } else {
                (data.to_string(), None, None)
            }
        }
        v => (v.to_string(), None, None),
    }
}

/// Display a logging notification based on its type and context
fn display_log_notification(
    formatted_message: &str,
    subagent_id: Option<&str>,
    notification_type: Option<&str>,
    progress_bars: &mut output::McpSpinners,
    interactive: bool,
    is_json_mode: bool,
) {
    if subagent_id.is_some() {
        if interactive {
            let _ = progress_bars.hide();
            if !is_json_mode {
                println!("{}", console::style(formatted_message).green().dim());
            }
        } else if !is_json_mode {
            progress_bars.log(formatted_message);
        }
    } else if let Some(ntype) = notification_type {
        if ntype == TASK_EXECUTION_NOTIFICATION_TYPE {
            if interactive {
                let _ = progress_bars.hide();
            }
            if !is_json_mode {
                for line in formatted_message.lines() {
                    println!("    {}", console::style(line).dim());
                }
                std::io::stdout().flush().unwrap();
            }
        } else if ntype == "shell_output" {
            let config = Config::global();
            let min_priority = config
                .get_param::<f32>("GOOSE_CLI_MIN_PRIORITY")
                .ok()
                .unwrap_or(output::DEFAULT_MIN_PRIORITY);

            if min_priority < 0.1 {
                if interactive {
                    let _ = progress_bars.hide();
                }
                if !is_json_mode {
                    println!("    {}", console::style(formatted_message).dim());
                }
            }
        }
    } else if output::is_showing_thinking() {
        output::set_thinking_message(&formatted_message.to_string());
    } else {
        progress_bars.log(formatted_message);
    }
}

/// Log tool request/response metrics
fn log_tool_metrics(message: &Message, messages: &Conversation) {
    for content in &message.content {
        if let MessageContent::ToolRequest(tool_request) = content {
            if let Ok(tool_call) = &tool_request.tool_call {
                tracing::info!(
                    monotonic_counter.goose.tool_calls = 1,
                    tool_name = %tool_call.name,
                    "Tool call started"
                );
            }
        }
        if let MessageContent::ToolResponse(tool_response) = content {
            let tool_name = messages
                .iter()
                .rev()
                .find_map(|msg| {
                    msg.content.iter().find_map(|c| {
                        if let MessageContent::ToolRequest(req) = c {
                            if req.id == tool_response.id {
                                req.tool_call.as_ref().ok().map(|tc| tc.name.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_else(|| "unknown".to_string().into());

            let result_status = if tool_response.tool_result.is_ok() {
                "success"
            } else {
                "error"
            };
            tracing::info!(
                monotonic_counter.goose.tool_completions = 1,
                tool_name = %tool_name,
                result = %result_status,
                "Tool call completed"
            );
        }
    }
}

/// Handle and display an agent error
fn handle_agent_error(e: &anyhow::Error, is_stream_json_mode: bool) {
    let error_msg = e.to_string();

    if is_stream_json_mode {
        emit_stream_event(&StreamEvent::Error {
            error: error_msg.clone(),
        });
    }

    if e.downcast_ref::<goose::providers::errors::ProviderError>()
        .map(|provider_error| {
            matches!(
                provider_error,
                goose::providers::errors::ProviderError::ContextLengthExceeded(_)
            )
        })
        .unwrap_or(false)
    {
        if !is_stream_json_mode {
            output::render_text(
                "Compaction requested. Should have happened in the agent!",
                Some(Color::Yellow),
                true,
            );
        }
        warn!("Compaction requested. Should have happened in the agent!");
    }

    if !is_stream_json_mode {
        eprintln!("Error: {}", error_msg);
    }
}

async fn get_reasoner() -> Result<Arc<dyn Provider>, anyhow::Error> {
    use goose::model::ModelConfig;
    use goose::providers::create;

    let config = Config::global();

    // Try planner-specific provider first, fall back to default provider
    let provider = if let Ok(provider) = config.get_param::<String>("GOOSE_PLANNER_PROVIDER") {
        provider
    } else {
        println!("WARNING: GOOSE_PLANNER_PROVIDER not found. Using default provider...");
        config
            .get_goose_provider()
            .expect("No provider configured. Run 'goose configure' first")
    };

    // Try planner-specific model first, fall back to default model
    let model = if let Ok(model) = config.get_param::<String>("GOOSE_PLANNER_MODEL") {
        model
    } else {
        println!("WARNING: GOOSE_PLANNER_MODEL not found. Using default model...");
        config
            .get_goose_model()
            .expect("No model configured. Run 'goose configure' first")
    };

    let model_config =
        ModelConfig::new_with_context_env(model, &provider, Some("GOOSE_PLANNER_CONTEXT_LIMIT"))?;
    let extensions = goose::config::extensions::get_enabled_extensions_with_config(config);
    let reasoner = create(&provider, model_config, extensions).await?;

    Ok(reasoner)
}

/// Format elapsed time duration
/// Shows seconds if less than 60, otherwise shows minutes:seconds
fn format_elapsed_time(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    if total_secs < 60 {
        format!("{:.2}s", duration.as_secs_f64())
    } else {
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        format!("{}m {:02}s", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose::agents::extension::Envs;
    use goose::config::ExtensionConfig;
    use std::time::Duration;
    use test_case::test_case;

    #[test]
    fn test_format_elapsed_time_under_60_seconds() {
        // Test sub-second duration
        let duration = Duration::from_millis(500);
        assert_eq!(format_elapsed_time(duration), "0.50s");

        // Test exactly 1 second
        let duration = Duration::from_secs(1);
        assert_eq!(format_elapsed_time(duration), "1.00s");

        // Test 45.75 seconds
        let duration = Duration::from_millis(45750);
        assert_eq!(format_elapsed_time(duration), "45.75s");

        // Test 59.99 seconds
        let duration = Duration::from_millis(59990);
        assert_eq!(format_elapsed_time(duration), "59.99s");
    }

    #[test]
    fn test_format_elapsed_time_minutes() {
        // Test exactly 60 seconds (1 minute)
        let duration = Duration::from_secs(60);
        assert_eq!(format_elapsed_time(duration), "1m 00s");

        // Test 61 seconds (1 minute 1 second)
        let duration = Duration::from_secs(61);
        assert_eq!(format_elapsed_time(duration), "1m 01s");

        // Test 90 seconds (1 minute 30 seconds)
        let duration = Duration::from_secs(90);
        assert_eq!(format_elapsed_time(duration), "1m 30s");

        // Test 119 seconds (1 minute 59 seconds)
        let duration = Duration::from_secs(119);
        assert_eq!(format_elapsed_time(duration), "1m 59s");

        // Test 120 seconds (2 minutes)
        let duration = Duration::from_secs(120);
        assert_eq!(format_elapsed_time(duration), "2m 00s");

        // Test 605 seconds (10 minutes 5 seconds)
        let duration = Duration::from_secs(605);
        assert_eq!(format_elapsed_time(duration), "10m 05s");

        // Test 3661 seconds (61 minutes 1 second)
        let duration = Duration::from_secs(3661);
        assert_eq!(format_elapsed_time(duration), "61m 01s");
    }

    #[test]
    fn test_format_elapsed_time_edge_cases() {
        // Test zero duration
        let duration = Duration::from_secs(0);
        assert_eq!(format_elapsed_time(duration), "0.00s");

        // Test very small duration (1 millisecond)
        let duration = Duration::from_millis(1);
        assert_eq!(format_elapsed_time(duration), "0.00s");

        // Test fractional seconds are truncated for minute display
        // 60.5 seconds should still show as 1m 00s (not 1m 00.5s)
        let duration = Duration::from_millis(60500);
        assert_eq!(format_elapsed_time(duration), "1m 00s");
    }

    #[test_case(
        "/usr/bin/my-server",
        ExtensionConfig::Stdio {
            name: "my-server".into(),
            cmd: "/usr/bin/my-server".into(),
            args: vec![],
            envs: Envs::default(),
            env_keys: vec![],
            description: goose::config::DEFAULT_EXTENSION_DESCRIPTION.to_string(),
            timeout: Some(goose::config::DEFAULT_EXTENSION_TIMEOUT),
            bundled: None,
            available_tools: vec![],
        }
        ; "name_from_cmd_basename"
    )]
    #[test_case(
        "MY_SECRET=s3cret npx -y @modelcontextprotocol/server-everything",
        ExtensionConfig::Stdio {
            name: "npx".into(),
            cmd: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-everything".into()],
            envs: Envs::new([("MY_SECRET".into(), "s3cret".into())].into()),
            env_keys: vec![],
            description: goose::config::DEFAULT_EXTENSION_DESCRIPTION.to_string(),
            timeout: Some(goose::config::DEFAULT_EXTENSION_TIMEOUT),
            bundled: None,
            available_tools: vec![],
        }
        ; "env_prefix_name_from_cmd"
    )]
    #[test_case(
        r#""/Applications/IntelliJ IDEA.app/Contents/jbr/Contents/Home/bin/java" -classpath "/path/with spaces/lib.jar" Main"#,
        ExtensionConfig::Stdio {
            name: "java".into(),
            cmd: "/Applications/IntelliJ IDEA.app/Contents/jbr/Contents/Home/bin/java".into(),
            args: vec!["-classpath".into(), "/path/with spaces/lib.jar".into(), "Main".into()],
            envs: Envs::default(),
            env_keys: vec![],
            description: goose::config::DEFAULT_EXTENSION_DESCRIPTION.to_string(),
            timeout: Some(goose::config::DEFAULT_EXTENSION_TIMEOUT),
            bundled: None,
            available_tools: vec![],
        }
        ; "quoted_path_with_spaces"
    )]
    fn test_parse_stdio_extension(input: &str, expected: ExtensionConfig) {
        assert_eq!(CliSession::parse_stdio_extension(input).unwrap(), expected);
    }

    #[test]
    fn test_parse_stdio_extension_no_command() {
        assert!(CliSession::parse_stdio_extension("").is_err());
    }

    #[test]
    fn test_split_quoted_windows_paths() {
        assert_eq!(
            split_quoted(r"C:\tools\mcp.exe --arg value").unwrap(),
            vec![r"C:\tools\mcp.exe", "--arg", "value"]
        );
        assert_eq!(
            split_quoted(r#""C:\Program Files\server\mcp.exe" --arg"#).unwrap(),
            vec![r"C:\Program Files\server\mcp.exe", "--arg"]
        );
    }

    #[test]
    fn test_split_quoted_unmatched_quote() {
        assert!(split_quoted(r#""unmatched"#).is_err());
    }

    #[test_case(
        "https://mcp.kiwi.com", 300,
        ExtensionConfig::StreamableHttp {
            name: "mcp_kiwi_com".into(),
            uri: "https://mcp.kiwi.com".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::new(),
            description: goose::config::DEFAULT_EXTENSION_DESCRIPTION.to_string(),
            timeout: Some(300),
            socket: None,
            bundled: None,
            available_tools: vec![],
        }
        ; "name_from_host"
    )]
    #[test_case(
        "http://localhost:8080/api", 300,
        ExtensionConfig::StreamableHttp {
            name: "localhost_8080_api".into(),
            uri: "http://localhost:8080/api".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::new(),
            description: goose::config::DEFAULT_EXTENSION_DESCRIPTION.to_string(),
            timeout: Some(300),
            socket: None,
            bundled: None,
            available_tools: vec![],
        }
        ; "port_and_path"
    )]
    #[test_case(
        "http://localhost:9090/other", 300,
        ExtensionConfig::StreamableHttp {
            name: "localhost_9090_other".into(),
            uri: "http://localhost:9090/other".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::new(),
            description: goose::config::DEFAULT_EXTENSION_DESCRIPTION.to_string(),
            timeout: Some(300),
            socket: None,
            bundled: None,
            available_tools: vec![],
        }
        ; "different_port_and_path"
    )]
    fn test_parse_streamable_http_extension(url: &str, timeout: u64, expected: ExtensionConfig) {
        assert_eq!(
            CliSession::parse_streamable_http_extension(url, timeout),
            expected
        );
    }
}
