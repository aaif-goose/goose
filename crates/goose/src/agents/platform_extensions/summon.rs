use crate::agents::extension::PlatformExtensionContext;
use crate::agents::final_output_tool::FinalOutputTool;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::subagent_task_config::DEFAULT_SUBAGENT_MAX_TURNS;
use crate::agents::tool_execution::ToolCallContext;
use crate::config::paths::Paths;
use crate::config::{Config, GooseMode};
use crate::conversation::message::Message;
use crate::providers;
use crate::recipe::build_recipe::build_recipe_from_template;
use crate::recipe::local_recipes::load_local_recipe_file;
use crate::recipe::{Recipe, RecipeParameter, Response, Settings, RECIPE_FILE_EXTENSIONS};
use crate::session::extension_data::{EnabledExtensionsState, ExtensionData, ExtensionState};
use crate::session::{Session, SessionType};
use crate::sources::parse_frontmatter;
use crate::utils::safe_truncate;
use anyhow::Result;
use async_trait::async_trait;
use goose_sdk_types::custom_requests::{SourceEntry, SourceType};
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    MetaObject, ServerCapabilities, Tool,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub static EXTENSION_NAME: &str = "summon";

const SUBAGENT_DESCRIPTION_BUDGET: usize = 160;

fn kind_plural(kind: SourceType) -> &'static str {
    match kind {
        SourceType::Subrecipe => "Subrecipes",
        SourceType::Recipe => "Recipes",
        SourceType::Agent => "Agents",
        _ => "Other",
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct DelegateParams {
    pub instructions: Option<String>,
    pub source: Option<String>,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub extensions: Option<Vec<String>>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_turns: Option<usize>,
    pub context: Option<String>,
    pub working_dir: Option<String>,
}

struct SubagentConfig {
    provider_name: String,
    model_config: goose_providers::model::ModelConfig,
    extensions: Vec<crate::config::ExtensionConfig>,
    working_dir: PathBuf,
    max_turns: usize,
}

fn merge_subrecipe_parameters(
    fixed_values: Option<&HashMap<String, String>>,
    provided_parameters: Option<&HashMap<String, serde_json::Value>>,
) -> HashMap<String, String> {
    let mut merged = fixed_values.cloned().unwrap_or_default();
    if let Some(provided_parameters) = provided_parameters {
        for (key, value) in provided_parameters {
            let value = match value {
                serde_json::Value::String(value) => value.clone(),
                other => other.to_string(),
            };
            merged.entry(key.clone()).or_insert(value);
        }
    }
    merged
}

#[derive(Debug, Deserialize)]
struct AgentMetadata {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

fn parse_agent_content(content: &str, path: &Path) -> Option<SourceEntry> {
    let (metadata, body): (AgentMetadata, String) = match parse_frontmatter(content) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => return None,
        Err(e) => {
            // Missing fields means this file has valid YAML but isn't an agent — skip silently.
            // Only warn on actual YAML syntax errors.
            if e.to_string().contains("missing field") {
                return None;
            }
            warn!("Failed to parse agent file {}: {}", path.display(), e);
            return None;
        }
    };

    let description = metadata.description.unwrap_or_else(|| {
        let model_info = metadata
            .model
            .as_ref()
            .map(|m| format!(" ({})", m))
            .unwrap_or_default();
        format!("Agent{}", model_info)
    });

    let mut properties = std::collections::HashMap::new();
    if let Some(model) = metadata.model {
        properties.insert("model".to_string(), serde_json::Value::String(model));
    }

    Some(SourceEntry {
        source_type: SourceType::Agent,
        name: metadata.name,
        description,
        content: body,
        path: path.to_string_lossy().into_owned(),
        global: false,
        writable: true,
        supporting_files: Vec::new(),
        properties,
    })
}

fn scan_recipes_from_dir(
    dir: &Path,
    kind: SourceType,
    suppress_config_warnings: bool,
    sources: &mut Vec<SourceEntry>,
    seen: &mut std::collections::HashSet<String>,
) {
    let Ok(source_dir) = dir.canonicalize() else {
        return;
    };
    let entries = match std::fs::read_dir(&source_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let path = source_dir.join(&file_name);

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !RECIPE_FILE_EXTENSIONS.contains(&ext) {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        if name.is_empty() || seen.contains(&name) {
            continue;
        }

        let content = match crate::skills::read_source_file(&source_dir, Path::new(&file_name)) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read recipe {}: {}", path.display(), error);
                continue;
            }
        };

        match Recipe::from_content(&content) {
            Ok(recipe) => {
                seen.insert(name.clone());
                sources.push(SourceEntry {
                    source_type: kind,
                    name,
                    description: recipe.description.clone(),
                    content: recipe.instructions.clone().unwrap_or_default(),
                    path: path.to_string_lossy().into_owned(),
                    global: false,
                    writable: true,
                    supporting_files: Vec::new(),
                    properties: std::collections::HashMap::new(),
                });
            }
            Err(e) => {
                // The working directory commonly contains project config like package.json
                // and tsconfig.json, which parse as valid JSON but lack Recipe fields. In that
                // case treat them as "not a recipe" rather than warning. Dedicated recipe
                // directories still warn so a real recipe with a typo is not silently dropped.
                if suppress_config_warnings && e.to_string().contains("missing field") {
                    continue;
                }
                warn!("Failed to parse recipe {}: {}", path.display(), e);
            }
        }
    }
}

fn scan_agents_from_dir(
    dir: &Path,
    sources: &mut Vec<SourceEntry>,
    seen: &mut std::collections::HashSet<String>,
) {
    let Ok(source_dir) = dir.canonicalize() else {
        return;
    };
    let entries = match std::fs::read_dir(&source_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let path = source_dir.join(&file_name);

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "md" {
            continue;
        }

        let content = match crate::skills::read_source_file(&source_dir, Path::new(&file_name)) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read agent file {}: {}", path.display(), e);
                continue;
            }
        };

        if let Some(source) = parse_agent_content(&content, &path) {
            if !seen.contains(&source.name) {
                seen.insert(source.name.clone());
                sources.push(source);
            }
        }
    }
}

pub fn discover_filesystem_sources(working_dir: &Path) -> Vec<SourceEntry> {
    let mut sources: Vec<SourceEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let home = dirs::home_dir();
    let config = Paths::config_dir();

    let local_recipe_dirs: Vec<PathBuf> = vec![
        working_dir.join(".goose/recipes"),
        working_dir.join(".agents/recipes"),
    ];

    let global_recipe_dirs: Vec<PathBuf> = std::env::var("GOOSE_RECIPE_PATH")
        .ok()
        .into_iter()
        .flat_map(|p| {
            let sep = if cfg!(windows) { ';' } else { ':' };
            p.split(sep).map(PathBuf::from).collect::<Vec<_>>()
        })
        .chain(
            [
                home.as_ref().map(|h| h.join(".goose/recipes")),
                Some(config.join("recipes")),
                home.as_ref().map(|h| h.join(".agents/recipes")),
            ]
            .into_iter()
            .flatten(),
        )
        .collect();

    let local_agent_dirs: Vec<PathBuf> = vec![
        working_dir.join(".goose/agents"),
        working_dir.join(".claude/agents"),
        working_dir.join(".agents/agents"),
    ];

    let global_agent_dirs: Vec<PathBuf> = [
        home.as_ref().map(|h| h.join(".goose/agents")),
        home.as_ref().map(|h| h.join(".agents/agents")),
        Some(config.join("agents")),
        home.as_ref().map(|h| h.join(".claude/agents")),
    ]
    .into_iter()
    .flatten()
    .collect();

    scan_recipes_from_dir(
        working_dir,
        SourceType::Recipe,
        true,
        &mut sources,
        &mut seen,
    );

    for dir in local_recipe_dirs {
        scan_recipes_from_dir(&dir, SourceType::Recipe, false, &mut sources, &mut seen);
    }

    for dir in local_agent_dirs {
        scan_agents_from_dir(&dir, &mut sources, &mut seen);
    }

    for dir in global_recipe_dirs {
        scan_recipes_from_dir(&dir, SourceType::Recipe, false, &mut sources, &mut seen);
    }

    for dir in global_agent_dirs {
        scan_agents_from_dir(&dir, &mut sources, &mut seen);
    }

    sources
}

fn build_instructions_with_context(context: &str, instructions: &str) -> String {
    let mut result = format!("# Reference Context\n\n{}", context);
    if !instructions.is_empty() {
        result.push_str(&format!("\n\n# Task Instructions\n\n{}", instructions));
    }
    result
}

fn build_subagent_instructions(sources: &[SourceEntry]) -> String {
    if sources.is_empty() {
        return String::new();
    }

    let names = sources
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let mut out = String::new();
    out.push_str(
        "\n\nThe following named subagents are available in this session and \
         can be invoked through the `delegate` tool (run as a subagent) or \
         the `load` tool (read their instructions into your own context):\n",
    );

    let mut current_kind: Option<SourceType> = None;
    for s in sources {
        if current_kind != Some(s.source_type) {
            out.push_str(&format!("\n{}:", kind_plural(s.source_type)));
            current_kind = Some(s.source_type);
        }
        out.push_str(&format!(
            "\n• {} — {}",
            s.name,
            safe_truncate(&s.description, SUBAGENT_DESCRIPTION_BUDGET)
        ));
    }

    out.push_str(&format!(
        "\n\nWhen to call a subagent (one of [{names}]):\n\
         • `@<name>` in the user's message — always call that subagent.\n\
         • The user mentions a subagent by name without `@` — infer from \
         context whether they want it invoked, and if so, call it.\n\
         • The user's request strongly matches a subagent's description — \
         call it.\n\n\
         Calling a subagent normally means `delegate(source: \"<name>\", \
         instructions: ...)`, which runs it as an isolated subagent and \
         returns its result. Use `load(source: \"<name>\")` instead if you \
         only want to read the subagent's instructions into your own \
         context.",
    ));

    out
}

pub struct SummonClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
    source_cache: Mutex<Option<CachedSources>>,
}

type CachedSources = (Instant, PathBuf, Vec<SourceEntry>);

impl SummonClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(EXTENSION_NAME, "1.0.0").with_title("Summon"));

        Ok(Self {
            info,
            context,
            source_cache: Mutex::new(None),
        })
    }

    async fn create_subagent_session(
        &self,
        working_dir: &Path,
        parent_session_id: &str,
        name: String,
    ) -> Result<crate::session::Session, String> {
        let session = self
            .context
            .session_manager
            .create_session(
                working_dir.to_path_buf(),
                name,
                SessionType::SubAgent,
                GooseMode::Auto,
            )
            .await
            .map_err(|e| format!("Failed to create subagent session: {}", e))?;

        if !parent_session_id.is_empty() {
            self.context
                .session_manager
                .update(&session.id)
                .parent_session_id(Some(parent_session_id.to_string()))
                .apply()
                .await
                .map_err(|e| format!("Failed to link subagent to parent session: {}", e))?;
        }

        Ok(session)
    }

    fn create_load_tool(&self) -> Tool {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Name of the source to load. If omitted, lists all available sources."
                }
            }
        });

        Tool::new(
            "load",
            "Load knowledge into your current context or discover available sources.\n\n\
             Call with no arguments to list all available sources (subrecipes, recipes, agents).\n\
             Call with a source name to load its content into your context.\n\n\
             Examples:\n\
             - load() → Lists available sources\n\
             - load(source: \"deploy\") → Loads the deploy recipe"
                .to_string(),
            schema.as_object().unwrap().clone(),
        )
    }

    fn create_delegate_tool(&self) -> Tool {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "instructions": {
                    "type": "string",
                    "description": "Task instructions. Required for ad-hoc tasks."
                },
                "source": {
                    "type": "string",
                    "description": "Name of a recipe or agent to run."
                },
                "parameters": {
                    "type": "object",
                    "additionalProperties": true,
                    "description": "Parameters for the source (only valid with source)."
                },
                "extensions": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Extensions to enable. Omit to inherit all, empty array for none."
                },
                "provider": {
                    "type": "string",
                    "description": "Override LLM provider."
                },
                "model": {
                    "type": "string",
                    "description": "Override model."
                },
                "temperature": {
                    "type": "number",
                    "description": "Override temperature."
                },
                "max_turns": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum turns for this delegate. Overrides recipe settings.max_turns and GOOSE_SUBAGENT_MAX_TURNS."
                },
                "context": {
                    "type": "string",
                    "description": "Reference context to inject into the delegate's system prompt. Use for background information, file contents, or constraints the delegate needs but that aren't part of the task instructions."
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for the delegate. Must be within the parent session's working directory. Defaults to the parent's working directory."
                }
            }
        });

        Tool::new(
            "delegate",
            "Delegate a task to a subagent that runs independently with its own context.\n\n\
             Modes:\n\
             1. Ad-hoc: Provide `instructions` for a custom task\n\
             2. Source-based: Provide `source` name to run a subrecipe, recipe, or agent\n\
             3. Combined: Pair a source with a task (e.g., source: \"deploy\", instructions: \"deploy to staging\")\n\n\
             Effective Delegation:\n\
             - Delegates know only instructions + source content\n\
             - Delegates cannot coordinate. Same-file work = conflicts.\n\
             - You get a delegate's result back once it finishes.\n\n\
             Research (read-only): delegates explore and report back.\n\
             Work (writes): partition files strictly - no two delegates touch the same file."
                .to_string(),
            schema.as_object().unwrap().clone(),
        )
    }

    fn working_dir(&self, ctx: &ToolCallContext) -> PathBuf {
        ctx.working_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }

    async fn get_sources(&self, session_id: &str, working_dir: &Path) -> Vec<SourceEntry> {
        let fs_sources = self.get_filesystem_sources(working_dir).await;

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut sources: Vec<SourceEntry> = Vec::new();

        self.add_subrecipes(session_id, &mut sources, &mut seen)
            .await;

        for source in fs_sources {
            if !seen.contains(&source.name) {
                seen.insert(source.name.clone());
                sources.push(source);
            }
        }

        sources.sort_by(|a, b| (&a.source_type, &a.name).cmp(&(&b.source_type, &b.name)));
        sources
    }

    async fn get_filesystem_sources(&self, working_dir: &Path) -> Vec<SourceEntry> {
        let mut cache = self.source_cache.lock().await;
        if let Some((cached_at, cached_dir, sources)) = cache.as_ref() {
            if cached_dir == working_dir && cached_at.elapsed() < Duration::from_secs(60) {
                return sources.clone();
            }
        }
        let sources = self.discover_filesystem_sources(working_dir);
        *cache = Some((Instant::now(), working_dir.to_path_buf(), sources.clone()));
        sources
    }

    async fn resolve_source(
        &self,
        session_id: &str,
        name: &str,
        working_dir: &Path,
    ) -> Result<Option<SourceEntry>, String> {
        let sources = self.get_sources(session_id, working_dir).await;

        Ok(sources.iter().find(|s| s.name == name).cloned())
    }

    async fn load_subrecipe_content(&self, session_id: &str, name: &str) -> Result<String, String> {
        let session = match self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
        {
            Ok(s) => s,
            Err(_) => return Ok(String::new()),
        };

        let sub_recipes = match session.recipe.as_ref().and_then(|r| r.sub_recipes.as_ref()) {
            Some(sr) => sr,
            None => return Ok(String::new()),
        };

        let sr = match sub_recipes.iter().find(|sr| sr.name == name) {
            Some(sr) => sr,
            None => return Ok(String::new()),
        };

        match load_local_recipe_file(&sr.path) {
            Ok(recipe_file) => Self::format_subrecipe_content(name, &recipe_file.content),
            Err(_) => Ok(String::new()),
        }
    }

    fn format_subrecipe_content(name: &str, raw_content: &str) -> Result<String, String> {
        let recipe = Recipe::from_content(raw_content)
            .map_err(|_| format!("Subrecipe '{}' is not a valid recipe", name))?;
        let mut content = recipe.instructions.unwrap_or_default();
        if let Some(params) = &recipe.parameters {
            if !params.is_empty() {
                content.push_str("\n\n");
                content.push_str(&Self::format_parameters(params));
            }
        }
        Ok(content)
    }

    fn discover_filesystem_sources(&self, working_dir: &Path) -> Vec<SourceEntry> {
        discover_filesystem_sources(working_dir)
    }

    async fn add_subrecipes(
        &self,
        session_id: &str,
        sources: &mut Vec<SourceEntry>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        let session = match self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
        {
            Ok(s) => s,
            Err(_) => return,
        };

        let sub_recipes = match session.recipe.as_ref().and_then(|r| r.sub_recipes.as_ref()) {
            Some(sr) => sr,
            None => return,
        };

        for sr in sub_recipes {
            if seen.contains(&sr.name) {
                continue;
            }
            seen.insert(sr.name.clone());

            let description = self.build_subrecipe_description(sr).await;

            sources.push(SourceEntry {
                source_type: SourceType::Subrecipe,
                name: sr.name.clone(),
                description,
                content: String::new(),
                path: sr.path.clone(),
                global: false,
                writable: true,
                supporting_files: Vec::new(),
                properties: std::collections::HashMap::new(),
            });
        }
    }

    async fn build_subrecipe_description(&self, sr: &crate::recipe::SubRecipe) -> String {
        if let Some(desc) = &sr.description {
            return desc.clone();
        }

        if let Ok(recipe_file) = load_local_recipe_file(&sr.path) {
            if let Ok(recipe) = Recipe::from_content(&recipe_file.content) {
                let mut desc = recipe.description.clone();

                if let Some(params) = &recipe.parameters {
                    if !params.is_empty() {
                        desc = format!("{}\n{}", desc, Self::format_parameters(params));
                    }
                }

                return desc;
            }
        }

        format!("Subrecipe from {}", sr.path)
    }

    fn format_parameters(params: &[RecipeParameter]) -> String {
        let mut out = String::from("Parameters:");
        for p in params {
            let mut detail = format!("\n  - {} ({}, {})", p.key, p.input_type, p.requirement);
            if let Some(default) = &p.default {
                detail.push_str(&format!(", default: \"{}\"", default));
            }
            if let Some(options) = &p.options {
                if !options.is_empty() {
                    detail.push_str(&format!(", options: [{}]", options.join(", ")));
                }
            }
            detail.push_str(&format!(": {}", p.description));
            out.push_str(&detail);
        }
        out
    }

    async fn handle_load(
        &self,
        session_id: &str,
        working_dir: &Path,
        arguments: Option<JsonObject>,
    ) -> Result<CallToolResult, String> {
        let source_name = arguments
            .as_ref()
            .and_then(|args| args.get("source"))
            .and_then(|v| v.as_str());

        if source_name.is_none() {
            return self
                .handle_load_discovery(session_id, working_dir)
                .await
                .map(CallToolResult::success);
        }

        let name = source_name.unwrap();

        self.handle_load_source(session_id, name, working_dir)
            .await
            .map(CallToolResult::success)
    }

    async fn handle_load_discovery(
        &self,
        session_id: &str,
        working_dir: &Path,
    ) -> Result<Vec<ContentBlock>, String> {
        {
            let mut cache = self.source_cache.lock().await;
            *cache = None;
        }

        let sources = self.get_sources(session_id, working_dir).await;

        if sources.is_empty() {
            return Ok(vec![ContentBlock::text(
                "No sources available for load/delegate.\n\n\
                 Sources are discovered from:\n\
                 • Current recipe's sub_recipes\n\
                 • .agents/recipes/, .agents/agents/ (project-level)\n\
                 • ~/.agents/agents/ (global)\n\
                 • GOOSE_RECIPE_PATH directories",
            )]);
        }

        let mut output = String::from("Available sources for load/delegate:\n");

        for kind in [SourceType::Subrecipe, SourceType::Recipe, SourceType::Agent] {
            let kind_sources: Vec<_> = sources.iter().filter(|s| s.source_type == kind).collect();
            if !kind_sources.is_empty() {
                output.push_str(&format!("\n{}:\n", kind_plural(kind)));
                for source in kind_sources {
                    output.push_str(&format!(
                        "• {} - {}\n",
                        source.name,
                        safe_truncate(&source.description, SUBAGENT_DESCRIPTION_BUDGET)
                    ));
                }
            }
        }

        output.push_str("\nUse load(source: \"name\") to load into context.\n");
        output.push_str("Use delegate(source: \"name\") to run as subagent.");

        Ok(vec![ContentBlock::text(output)])
    }

    async fn handle_load_source(
        &self,
        session_id: &str,
        name: &str,
        working_dir: &Path,
    ) -> Result<Vec<ContentBlock>, String> {
        let source = self.resolve_source(session_id, name, working_dir).await?;

        match source {
            Some(mut source) => {
                if source.source_type == SourceType::Subrecipe && source.content.is_empty() {
                    source.content = self
                        .load_subrecipe_content(session_id, &source.name)
                        .await?;
                }
                let content = source.to_load_text();

                let output = format!(
                    "# Loaded: {} ({})\n\n{}\n\n---\nThis knowledge is now available in your context.",
                    source.name, source.source_type, content
                );

                Ok(vec![ContentBlock::text(output)])
            }
            None => {
                let sources = self.get_sources(session_id, working_dir).await;

                let suggestions: Vec<&str> = sources
                    .iter()
                    .filter(|s| {
                        s.name.to_lowercase().contains(&name.to_lowercase())
                            || name.to_lowercase().contains(&s.name.to_lowercase())
                    })
                    .take(3)
                    .map(|s| s.name.as_str())
                    .collect();

                let error_msg = if suggestions.is_empty() {
                    format!(
                        "Source '{}' not found. Use load() to see available sources.",
                        name
                    )
                } else {
                    format!(
                        "Source '{}' not found. Did you mean: {}?",
                        name,
                        suggestions.join(", ")
                    )
                };

                Err(error_msg)
            }
        }
    }

    async fn handle_delegate(
        &self,
        session_id: &str,
        working_dir: &Path,
        arguments: Option<JsonObject>,
    ) -> Result<CallToolResult, String> {
        let params: DelegateParams = arguments
            .map(|args| serde_json::from_value(serde_json::Value::Object(args)))
            .transpose()
            .map_err(|e| format!("Invalid parameters: {}", e))?
            .unwrap_or_default();

        self.validate_delegate_params(&params)?;

        let mut session = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|e| format!("Failed to get session: {}", e))?;
        session.working_dir = working_dir.to_path_buf();

        if session.session_type == SessionType::SubAgent {
            return Err("Delegated tasks cannot spawn further delegations".to_string());
        }

        self.handle_foreground_delegate(params, &session).await
    }

    async fn handle_foreground_delegate(
        &self,
        params: DelegateParams,
        parent: &Session,
    ) -> Result<CallToolResult, String> {
        let mut recipe = self
            .build_delegate_recipe(&params, &parent.id, &parent.working_dir)
            .await?;
        let subagent_config = self
            .resolve_subagent_config(&params, &recipe, parent)
            .await
            .map_err(|e| format!("Failed to resolve subagent config: {e}"))?;
        crate::providers::get_from_registry(&subagent_config.provider_name)
            .await
            .map_err(|_| {
                format!(
                    "Provider '{}' cannot be reconstructed for a foreground subagent",
                    subagent_config.provider_name
                )
            })?;

        let max_turns = subagent_config.max_turns;
        recipe
            .settings
            .get_or_insert(Settings {
                goose_provider: None,
                goose_model: None,
                temperature: None,
                max_turns: None,
            })
            .max_turns = Some(max_turns);
        if recipe
            .response
            .as_ref()
            .and_then(|response| response.json_schema.as_ref())
            .is_none()
        {
            recipe.response = Some(Response {
                json_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {"summary": {"type": "string"}},
                    "required": ["summary"]
                })),
            });
        }
        FinalOutputTool::try_new(recipe.response.as_ref().unwrap().clone())
            .map_err(|e| format!("Invalid delegate response schema: {e}"))?;

        let mut extension_data = ExtensionData::default();
        EnabledExtensionsState::new(subagent_config.extensions)
            .to_extension_data(&mut extension_data)
            .map_err(|e| format!("Failed to save delegate extensions: {e}"))?;

        let child = self
            .create_subagent_session(
                &subagent_config.working_dir,
                &parent.id,
                "Delegated task".to_string(),
            )
            .await?;
        let task = recipe
            .prompt
            .clone()
            .unwrap_or_else(|| "Begin.".to_string());
        self.context
            .session_manager
            .update(&child.id)
            .recipe(Some(recipe))
            .provider_name(&subagent_config.provider_name)
            .model_config(subagent_config.model_config)
            .extension_data(extension_data)
            .apply()
            .await
            .map_err(|e| format!("Failed to save delegate configuration: {e}"))?;

        self.context
            .session_manager
            .add_message(
                &child.id,
                &Message::user().with_text(format!("Subagent ID: {}\n\n{task}", child.id)),
            )
            .await
            .map_err(|e| format!("Failed to save delegate task: {e}"))?;

        let mut meta = MetaObject::new();
        meta.0.insert(
            "subagent_session_id".to_string(),
            serde_json::Value::String(child.id.clone()),
        );
        meta.0.insert(
            "foreground_subagent".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "Delegated to foreground subagent {}",
            child.id
        ))])
        .with_meta(Some(meta)))
    }

    fn validate_delegate_params(&self, params: &DelegateParams) -> Result<(), String> {
        if params.instructions.is_none() && params.source.is_none() {
            return Err("Must provide 'instructions' or 'source' (or both)".to_string());
        }

        if params.parameters.is_some() && params.source.is_none() {
            return Err("'parameters' can only be used with 'source'".to_string());
        }

        if let Some(max) = params.max_turns {
            if max < 1 {
                return Err("'max_turns' must be at least 1".to_string());
            }
        }

        Ok(())
    }

    async fn build_delegate_recipe(
        &self,
        params: &DelegateParams,
        session_id: &str,
        working_dir: &Path,
    ) -> Result<Recipe, String> {
        let mut recipe = if let Some(source_name) = &params.source {
            self.build_source_recipe(source_name, params, session_id, working_dir)
                .await?
        } else {
            self.build_adhoc_recipe(params)?
        };

        if let Some(ref context) = params.context {
            let existing = recipe.instructions.unwrap_or_default();
            recipe.instructions = Some(build_instructions_with_context(context, &existing));
        }

        Ok(recipe)
    }

    fn build_adhoc_recipe(&self, params: &DelegateParams) -> Result<Recipe, String> {
        let task = params
            .instructions
            .as_ref()
            .ok_or("Instructions required for ad-hoc task")?;

        Recipe::builder()
            .version("1.0.0")
            .title("Delegated Task")
            .description("Ad-hoc delegated task")
            .prompt(task)
            .build()
            .map_err(|e| format!("Failed to build recipe: {}", e))
    }

    async fn build_source_recipe(
        &self,
        source_name: &str,
        params: &DelegateParams,
        session_id: &str,
        working_dir: &Path,
    ) -> Result<Recipe, String> {
        let source = self
            .resolve_source(session_id, source_name, working_dir)
            .await?
            .ok_or_else(|| format!("Source '{}' not found", source_name))?;

        let mut recipe = match source.source_type {
            SourceType::Recipe | SourceType::Subrecipe => {
                self.build_recipe_from_source(&source, params, session_id)
                    .await?
            }
            SourceType::Agent => self.build_recipe_from_agent(&source, params)?,
            _ => {
                return Err(format!(
                    "Source '{}' has kind '{}' which cannot be delegated from summon",
                    source_name, source.source_type
                ));
            }
        };

        if let Some(extra_instructions) = &params.instructions {
            if recipe.prompt.is_some() {
                let current_prompt = recipe.prompt.take().unwrap();
                recipe.prompt = Some(format!("{}\n\n{}", current_prompt, extra_instructions));
            } else {
                recipe.prompt = Some(extra_instructions.clone());
            }
        }

        Ok(recipe)
    }

    async fn build_recipe_from_source(
        &self,
        source: &SourceEntry,
        params: &DelegateParams,
        session_id: &str,
    ) -> Result<Recipe, String> {
        let session = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|e| format!("Failed to get session: {}", e))?;

        if source.source_type == SourceType::Subrecipe {
            let sub_recipes = session.recipe.as_ref().and_then(|r| r.sub_recipes.as_ref());

            if let Some(sub_recipes) = sub_recipes {
                if let Some(sr) = sub_recipes.iter().find(|sr| sr.name == source.name) {
                    let recipe_file = load_local_recipe_file(&sr.path).map_err(|e| {
                        format!("Failed to load subrecipe '{}': {}", source.name, e)
                    })?;

                    let merged =
                        merge_subrecipe_parameters(sr.values.as_ref(), params.parameters.as_ref());
                    let param_values: Vec<(String, String)> = merged.into_iter().collect();

                    return build_recipe_from_template(
                        recipe_file.content,
                        &recipe_file.parent_dir,
                        param_values,
                        None::<fn(&str, &str) -> Result<String, anyhow::Error>>,
                    )
                    .map_err(|e| format!("Failed to build subrecipe: {}", e));
                }
            }
        }

        let recipe_file = load_local_recipe_file(&source.path)
            .map_err(|e| format!("Failed to load recipe '{}': {}", source.name, e))?;

        let param_values: Vec<(String, String)> = params
            .parameters
            .as_ref()
            .map(|p| {
                p.iter()
                    .map(|(k, v)| {
                        let value_str = match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        (k.clone(), value_str)
                    })
                    .collect()
            })
            .unwrap_or_default();

        build_recipe_from_template(
            recipe_file.content,
            &recipe_file.parent_dir,
            param_values,
            None::<fn(&str, &str) -> Result<String, anyhow::Error>>,
        )
        .map_err(|e| format!("Failed to build recipe: {}", e))
    }

    fn build_recipe_from_agent(
        &self,
        source: &SourceEntry,
        params: &DelegateParams,
    ) -> Result<Recipe, String> {
        if source.path.is_empty() {
            return Err("Agent source has no path".to_string());
        }

        let model = source
            .properties
            .get("model")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);

        // max_turns is set later in resolve_subagent_config so it can incorporate params.max_turns
        // with the correct priority ordering; setting it here would cause it to be overridden
        // by the parent session's recipe instead.
        let settings = model.map(|m| Settings {
            goose_model: Some(m),
            goose_provider: params.provider.clone(),
            temperature: params.temperature,
            max_turns: None,
        });

        let mut builder = Recipe::builder()
            .version("1.0.0")
            .title(format!("Agent: {}", source.name))
            .description(source.description.clone())
            .instructions(&source.content);

        if let Some(settings) = settings {
            builder = builder.settings(settings);
        }

        if params.instructions.is_none() {
            builder = builder.prompt("Proceed with your expertise to produce a useful result.");
        }

        builder
            .build()
            .map_err(|e| format!("Failed to build recipe from agent: {}", e))
    }

    async fn resolve_subagent_config(
        &self,
        params: &DelegateParams,
        recipe: &Recipe,
        session: &Session,
    ) -> Result<SubagentConfig> {
        let mut extensions = EnabledExtensionsState::extensions_or_default(
            Some(&session.extension_data),
            Config::global(),
        );

        if let Some(filter) = &params.extensions {
            if filter.is_empty() {
                extensions = Vec::new();
            } else {
                let available_names: Vec<String> =
                    extensions.iter().map(|ext| ext.name()).collect();
                extensions.retain(|ext| filter.contains(&ext.name()));
                let unmatched: Vec<&str> = filter
                    .iter()
                    .filter(|name| !available_names.iter().any(|n| n == *name))
                    .map(String::as_str)
                    .collect();
                if !unmatched.is_empty() {
                    warn!(
                        "Delegate requested extensions not available in session: {:?}. Available: {:?}",
                        unmatched, available_names
                    );
                }
            }
        }

        let (provider_name, model_config) = self
            .resolve_provider_config(params, recipe, session)
            .await?;

        let max_turns = params
            .max_turns
            .or_else(|| recipe.settings.as_ref().and_then(|s| s.max_turns))
            .unwrap_or_else(|| self.resolve_max_turns(session));

        if max_turns == 0 || max_turns > u32::MAX as usize {
            anyhow::bail!(
                "max_turns must be between 1 and {} (got {})",
                u32::MAX,
                max_turns
            );
        }

        let effective_working_dir = match &params.working_dir {
            Some(dir) => resolve_working_dir(&session.working_dir, dir)?,
            None => session.working_dir.clone(),
        };

        Ok(SubagentConfig {
            provider_name,
            model_config: model_config.with_cache_ttl_clamped(),
            extensions,
            working_dir: effective_working_dir,
            max_turns,
        })
    }

    fn resolve_model_config(
        &self,
        params: &DelegateParams,
        recipe: &Recipe,
        session: &crate::session::Session,
        provider_name: &str,
        provider_default_model: Option<&str>,
    ) -> Result<goose_providers::model::ModelConfig, anyhow::Error> {
        let env_model = std::env::var("GOOSE_SUBAGENT_MODEL").ok();
        let env_provider = std::env::var("GOOSE_SUBAGENT_PROVIDER").ok();
        let recipe_settings = recipe.settings.as_ref();
        let configured = Config::global().all_values().ok();
        let configured_provider = configured
            .as_ref()
            .and_then(|values| values.get("GOOSE_SUBAGENT_PROVIDER"))
            .and_then(serde_json::Value::as_str);
        let configured_model = configured
            .as_ref()
            .and_then(|values| values.get("GOOSE_SUBAGENT_MODEL"))
            .and_then(serde_json::Value::as_str);
        let matches_provider =
            |candidate: Option<&str>| candidate.is_none() || candidate == Some(provider_name);
        let model = recipe_settings
            .and_then(|settings| settings.goose_model.clone())
            .filter(|_| {
                matches_provider(
                    recipe_settings.and_then(|settings| settings.goose_provider.as_deref()),
                )
            })
            .or_else(|| {
                env_model
                    .clone()
                    .filter(|_| matches_provider(env_provider.as_deref()))
            })
            .or_else(|| {
                params
                    .model
                    .clone()
                    .filter(|_| matches_provider(params.provider.as_deref()))
            })
            .or_else(|| {
                configured_model
                    .filter(|_| matches_provider(configured_provider))
                    .map(str::to_string)
            })
            .or_else(|| {
                session
                    .model_config
                    .as_ref()
                    .filter(|_| matches_provider(session.provider_name.as_deref()))
                    .map(|config| config.model_name.clone())
            })
            .or_else(|| {
                provider_default_model
                    .filter(|model| !model.is_empty())
                    .map(str::to_string)
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No model configured for provider '{}'; set GOOSE_SUBAGENT_MODEL",
                    provider_name
                )
            })?;

        let parent = session.model_config.as_ref();
        let mut model_config = if parent.is_some_and(|config| {
            matches_provider(session.provider_name.as_deref()) && config.model_name == model
        }) {
            parent.unwrap().clone()
        } else {
            let mut cfg = crate::model_config::model_config_from_user_config_with_session_settings(
                provider_name,
                &model,
                parent,
                None,
                None,
            )?;
            if let Some(parent) = parent {
                cfg.toolshim = parent.toolshim;
                cfg.toolshim_model = parent.toolshim_model.clone();
                cfg.temperature = cfg.temperature.or(parent.temperature);
            }
            cfg
        };

        if let Some(temp) = params.temperature {
            model_config = model_config.with_temperature(Some(temp));
        } else if let Some(temp) = recipe.settings.as_ref().and_then(|s| s.temperature) {
            model_config = model_config.with_temperature(Some(temp));
        }

        Ok(model_config)
    }

    async fn resolve_provider_config(
        &self,
        params: &DelegateParams,
        recipe: &Recipe,
        session: &crate::session::Session,
    ) -> Result<(String, goose_providers::model::ModelConfig)> {
        let env_provider = std::env::var("GOOSE_SUBAGENT_PROVIDER").ok();
        let provider_name = recipe
            .settings
            .as_ref()
            .and_then(|s| s.goose_provider.clone())
            .or_else(|| env_provider.clone())
            .or_else(|| params.provider.clone())
            .or_else(|| {
                Config::global()
                    .get_param::<String>("GOOSE_SUBAGENT_PROVIDER")
                    .ok()
            })
            .or_else(|| session.provider_name.clone())
            .ok_or_else(|| anyhow::anyhow!("No provider configured"))?;

        let provider_entry = providers::get_from_registry(&provider_name).await;
        let provider_default_model = provider_entry
            .as_ref()
            .ok()
            .map(|entry| entry.metadata().default_model.as_str());
        let model_config = self.resolve_model_config(
            params,
            recipe,
            session,
            &provider_name,
            provider_default_model,
        )?;
        Ok((provider_name, model_config))
    }

    fn resolve_max_turns(&self, session: &crate::session::Session) -> usize {
        session
            .recipe
            .as_ref()
            .and_then(|r| r.settings.as_ref())
            .and_then(|s| s.max_turns)
            .or_else(|| {
                std::env::var("GOOSE_SUBAGENT_MAX_TURNS")
                    .ok()
                    .and_then(|v| v.parse().ok())
            })
            .or_else(|| {
                Config::global()
                    .get_param::<usize>("GOOSE_SUBAGENT_MAX_TURNS")
                    .ok()
            })
            .unwrap_or(DEFAULT_SUBAGENT_MAX_TURNS)
    }
}

#[async_trait]
impl McpClientTrait for SummonClient {
    async fn list_tools(
        &self,
        session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let is_subagent = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
            .map(|s| s.session_type == SessionType::SubAgent)
            .unwrap_or(false);

        let mut tools = vec![self.create_load_tool()];

        if !is_subagent {
            tools.push(self.create_delegate_tool());
        }

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let session_id = &ctx.session_id;
        let working_dir = self.working_dir(ctx);
        match name {
            "load" => match self.handle_load(session_id, &working_dir, arguments).await {
                Ok(result) => Ok(result),
                Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "Error: {}",
                    error
                ))])),
            },
            "delegate" => {
                match self
                    .handle_delegate(session_id, &working_dir, arguments)
                    .await
                {
                    Ok(result) => Ok(result),
                    Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                        "Error: {}",
                        error
                    ))])),
                }
            }
            _ => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error: Unknown tool: {}",
                name
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn get_instructions(&self, session_id: &str, working_dir: &Path) -> Option<String> {
        let sources = self.get_sources(session_id, working_dir).await;
        let instructions = build_subagent_instructions(&sources);
        (!instructions.is_empty()).then_some(instructions)
    }
}

/// Resolve a requested `working_dir` override against the parent session
/// directory. Relative paths are joined to the parent dir; the result must
/// canonicalize to an existing directory contained within the parent dir.
fn resolve_working_dir(parent_dir: &Path, requested: &str) -> Result<PathBuf, anyhow::Error> {
    let requested_path = PathBuf::from(requested);
    let resolved = if requested_path.is_absolute() {
        requested_path
    } else {
        parent_dir.join(&requested_path)
    };
    let canonical = resolved
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("working_dir '{}' could not be resolved: {}", requested, e))?;
    let parent_canonical = parent_dir
        .canonicalize()
        .unwrap_or_else(|_| parent_dir.to_path_buf());
    if !canonical.starts_with(&parent_canonical) {
        anyhow::bail!(
            "working_dir '{}' is outside the parent session directory",
            requested
        );
    }
    if !canonical.is_dir() {
        anyhow::bail!("working_dir '{}' is not a directory", requested);
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::MessageContent;
    use crate::providers::ProviderBackend;
    use serial_test::serial;
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_context() -> PlatformExtensionContext {
        create_test_context_with_session_manager(Arc::new(
            crate::session::SessionManager::instance(),
        ))
    }

    fn create_test_context_with_session_manager(
        session_manager: Arc<crate::session::SessionManager>,
    ) -> PlatformExtensionContext {
        PlatformExtensionContext {
            extension_manager: None,
            providers: Default::default(),
            session_manager,
            scheduler: None,
            use_login_shell_path: false,
        }
    }

    #[tokio::test]
    #[serial]
    async fn foreground_delegate_persists_child_without_creating_provider() {
        let temp_dir = TempDir::new().unwrap();
        let child_dir = temp_dir.path().join("child");
        fs::create_dir(&child_dir).unwrap();
        let manager = Arc::new(crate::session::SessionManager::new(
            temp_dir.path().to_path_buf(),
        ));
        let parent = manager
            .create_session(
                temp_dir.path().to_path_buf(),
                "Parent".to_string(),
                SessionType::User,
                GooseMode::Auto,
            )
            .await
            .unwrap();
        manager
            .update(&parent.id)
            .provider_name("openai")
            .model_config(
                goose_providers::model::ModelConfig::new("test-model").with_cache_ttl("1h"),
            )
            .apply()
            .await
            .unwrap();
        let client =
            SummonClient::new(create_test_context_with_session_manager(manager.clone())).unwrap();
        let args = serde_json::json!({
            "instructions": "Review the change",
            "provider": "openai",
            "model": "test-model",
            "extensions": [],
            "working_dir": "child",
            "max_turns": 3
        })
        .as_object()
        .unwrap()
        .clone();

        let result = {
            let _env =
                env_lock::lock_env([("OPENAI_HOST", None), ("OPENAI_BASE_URL", Some("http://"))]);
            assert!(providers::create_standard("openai", Vec::new())
                .await
                .is_err());
            client
                .handle_delegate(&parent.id, temp_dir.path(), Some(args))
                .await
                .unwrap()
        };
        let meta = result.meta.as_ref().unwrap();
        assert_eq!(
            meta.0.get("foreground_subagent"),
            Some(&serde_json::json!(true))
        );
        let child_id = meta.0["subagent_session_id"].as_str().unwrap().to_string();

        manager
            .add_message(
                &parent.id,
                &Message::user().with_tool_response("delegate-call", Ok(result)),
            )
            .await
            .unwrap();
        let reloaded = crate::session::SessionManager::new(temp_dir.path().to_path_buf());
        let child = reloaded.get_session(&child_id, true).await.unwrap();
        assert_eq!(child.session_type, SessionType::SubAgent);
        assert_eq!(child.parent_session_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(child.working_dir, child_dir.canonicalize().unwrap());
        assert_eq!(child.provider_name.as_deref(), Some("openai"));
        assert_eq!(
            child.model_config.as_ref().unwrap().model_name,
            "test-model"
        );
        assert_eq!(
            child.model_config.as_ref().unwrap().cache_ttl().as_deref(),
            Some("5m")
        );
        let recipe = child.recipe.as_ref().unwrap();
        assert_eq!(recipe.settings.as_ref().unwrap().max_turns, Some(3));
        assert_eq!(
            recipe
                .response
                .as_ref()
                .unwrap()
                .json_schema
                .as_ref()
                .unwrap()["required"],
            serde_json::json!(["summary"])
        );
        assert!(
            EnabledExtensionsState::from_extension_data(&child.extension_data)
                .unwrap()
                .extensions
                .is_empty()
        );
        assert!(child.conversation.as_ref().unwrap().iter().any(|message| {
            message.content.iter().any(|content| {
                matches!(content,
                MessageContent::Text(text) if text.text.contains("Review the change"))
            })
        }));

        let parent = reloaded.get_session(&parent.id, true).await.unwrap();
        assert!(parent.conversation.as_ref().unwrap().iter().any(|message| {
            message.content.iter().any(|content| {
                matches!(content,
                    MessageContent::ToolResponse(response)
                        if response.tool_result.as_ref().is_ok_and(|result|
                            result.meta.as_ref().is_some_and(|meta|
                                meta.0.get("foreground_subagent") == Some(&serde_json::json!(true))
                                    && meta.0.get("subagent_session_id").and_then(serde_json::Value::as_str)
                                        == Some(child_id.as_str())
                            )
                        )
                )
            })
        }));

        let (reloaded_agent, _) =
            crate::agents::subagent_handler::from_foreground_subagent_session(
                Arc::new(reloaded),
                &child,
                false,
            )
            .await
            .unwrap();
        assert_eq!(
            reloaded_agent
                .standard_provider(&child.id)
                .await
                .unwrap()
                .get_name(),
            "openai"
        );
    }

    #[tokio::test]
    async fn instructions_follow_the_calling_working_dir() {
        let old_working_dir = TempDir::new().unwrap();
        let new_working_dir = TempDir::new().unwrap();
        for (working_dir, name) in [
            (old_working_dir.path(), "old-agent"),
            (new_working_dir.path(), "new-agent"),
        ] {
            let agents = working_dir.join(".goose/agents");
            fs::create_dir_all(&agents).unwrap();
            fs::write(
                agents.join(format!("{name}.md")),
                format!("---\nname: {name}\ndescription: {name}\n---\n{name}"),
            )
            .unwrap();
        }
        let client = SummonClient::new(create_test_context()).unwrap();

        let old = client
            .get_instructions("session", old_working_dir.path())
            .await
            .unwrap();
        let new = client
            .get_instructions("session", new_working_dir.path())
            .await
            .unwrap();

        assert!(old.contains("old-agent") && !old.contains("new-agent"));
        assert!(new.contains("new-agent") && !new.contains("old-agent"));
    }

    #[tokio::test]
    async fn leased_client_loads_sources_from_its_snapshot_directory() {
        let data_dir = TempDir::new().unwrap();
        let old_working_dir = TempDir::new().unwrap();
        let new_working_dir = TempDir::new().unwrap();
        for (working_dir, instructions) in [
            (old_working_dir.path(), "old instructions"),
            (new_working_dir.path(), "new instructions"),
        ] {
            let agents = working_dir.join(".goose/agents");
            fs::create_dir_all(&agents).unwrap();
            fs::write(
                agents.join("reviewer.md"),
                format!("---\nname: reviewer\ndescription: reviewer\n---\n{instructions}"),
            )
            .unwrap();
        }
        let session_manager = Arc::new(crate::session::SessionManager::new(
            data_dir.path().to_path_buf(),
        ));
        let session = session_manager
            .create_session(
                old_working_dir.path().to_path_buf(),
                "moving".to_string(),
                SessionType::Hidden,
                GooseMode::Auto,
            )
            .await
            .unwrap();
        let client = SummonClient::new(create_test_context_with_session_manager(Arc::clone(
            &session_manager,
        )))
        .unwrap();
        session_manager
            .update(&session.id)
            .working_dir(new_working_dir.path().to_path_buf())
            .apply()
            .await
            .unwrap();
        let ctx =
            ToolCallContext::new(session.id, Some(old_working_dir.path().to_path_buf()), None);

        let result = client
            .call_tool(
                &ctx,
                "load",
                Some(
                    serde_json::json!({"source": "reviewer"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = result.content[0].as_text().unwrap();

        assert!(text.text.contains("old instructions"));
        assert!(!text.text.contains("new instructions"));
    }

    #[test]
    fn test_agent_frontmatter_parsing() {
        let agent = r#"---
name: reviewer
model: sonnet
---
You review code."#;
        let source = parse_agent_content(agent, Path::new("")).unwrap();
        assert_eq!(source.name, "reviewer");
        assert!(source.description.contains("sonnet"));
        assert_eq!(
            source
                .properties
                .get("model")
                .and_then(|value| value.as_str()),
            Some("sonnet")
        );
    }

    #[test]
    fn test_resolve_working_dir_relative_subdir() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().canonicalize().unwrap();
        let subdir = parent.join("sub");
        fs::create_dir(&subdir).unwrap();

        let resolved = resolve_working_dir(&parent, "sub").unwrap();
        assert_eq!(resolved, subdir.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_working_dir_rejects_traversal_outside_parent() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().join("parent");
        let sibling = temp_dir.path().join("sibling");
        fs::create_dir(&parent).unwrap();
        fs::create_dir(&sibling).unwrap();

        let err = resolve_working_dir(&parent, "../sibling").unwrap_err();
        assert!(
            err.to_string()
                .contains("outside the parent session directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_resolve_working_dir_rejects_file_path() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().canonicalize().unwrap();
        let file = parent.join("a.txt");
        fs::write(&file, "hello").unwrap();

        let err = resolve_working_dir(&parent, "a.txt").unwrap_err();
        assert!(
            err.to_string().contains("is not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_resolve_working_dir_rejects_nonexistent_path() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().canonicalize().unwrap();

        let err = resolve_working_dir(&parent, "does-not-exist").unwrap_err();
        assert!(
            err.to_string().contains("could not be resolved"),
            "unexpected error: {err}"
        );
    }
    #[test]
    fn test_agent_scan_skips_non_agent_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let agents_dir = temp_dir.path().join("agents");
        fs::create_dir_all(&agents_dir).unwrap();
        fs::write(
            agents_dir.join("README.md"),
            "---\ntitle: Notes\n---\nThis is not an agent.",
        )
        .unwrap();
        fs::write(
            agents_dir.join("notes.md"),
            "---\nauthor: someone\ntags: [docs]\n---\nJust documentation.",
        )
        .unwrap();
        fs::write(
            agents_dir.join("reviewer.md"),
            "---\nname: reviewer\nmodel: sonnet\n---\nYou review code.",
        )
        .unwrap();
        fs::write(agents_dir.join("plain.md"), "No frontmatter at all.").unwrap();
        fs::write(
            agents_dir.join("broken.md"),
            "---\nname: [unterminated\n---\nBroken YAML.",
        )
        .unwrap();

        let mut sources = Vec::new();
        let mut seen = HashSet::new();
        scan_agents_from_dir(&agents_dir, &mut sources, &mut seen);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "reviewer");
    }

    #[cfg(unix)]
    #[test]
    fn agent_scan_rejects_symlinked_source_file() {
        let temp_dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        fs::write(
            outside.path().join("outside.md"),
            "---\nname: outside\n---\nUntrusted agent.",
        )
        .unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("outside.md"),
            temp_dir.path().join("outside.md"),
        )
        .unwrap();

        let mut sources = Vec::new();
        let mut seen = HashSet::new();
        scan_agents_from_dir(temp_dir.path(), &mut sources, &mut seen);

        assert!(sources.is_empty());
    }

    #[test]
    fn test_recipe_scan_skips_non_recipe_project_config_files() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{"scripts":{"test":"cargo test"}}"#,
        )
        .unwrap();
        fs::write(
            temp_dir.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"strict":true}}"#,
        )
        .unwrap();
        fs::write(
            temp_dir.path().join("valid.yaml"),
            "title: Valid\ndescription: Real recipe\ninstructions: Run valid steps",
        )
        .unwrap();

        let mut sources = Vec::new();
        let mut seen = HashSet::new();
        scan_recipes_from_dir(
            temp_dir.path(),
            SourceType::Recipe,
            true,
            &mut sources,
            &mut seen,
        );

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "valid");
        assert_eq!(sources[0].description, "Real recipe");
    }

    #[cfg(unix)]
    #[test]
    fn recipe_scan_rejects_symlinked_source_file() {
        let temp_dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        fs::write(
            outside.path().join("outside.yaml"),
            "title: Outside\ndescription: Outside recipe\ninstructions: Untrusted",
        )
        .unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("outside.yaml"),
            temp_dir.path().join("outside.yaml"),
        )
        .unwrap();

        let mut sources = Vec::new();
        let mut seen = HashSet::new();
        scan_recipes_from_dir(
            temp_dir.path(),
            SourceType::Recipe,
            false,
            &mut sources,
            &mut seen,
        );

        assert!(sources.is_empty());
    }

    #[tokio::test]
    async fn test_discover_recipes_and_agents() {
        let temp_dir = TempDir::new().unwrap();

        let recipes = temp_dir.path().join(".goose/recipes");
        fs::create_dir_all(&recipes).unwrap();
        fs::write(
            recipes.join("deploy.yaml"),
            "title: Deploy\ndescription: Deploy to production\ninstructions: Run deploy steps",
        )
        .unwrap();

        let agents = temp_dir.path().join(".goose/agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(
            agents.join("reviewer.md"),
            "---\nname: reviewer\nmodel: sonnet\ndescription: Code reviewer\n---\nYou review code.",
        )
        .unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let sources = client.discover_filesystem_sources(temp_dir.path());

        let recipe = sources
            .iter()
            .find(|s| s.name == "deploy" && s.source_type == SourceType::Recipe)
            .unwrap();
        assert_eq!(recipe.description, "Deploy to production");
        assert_eq!(recipe.content, "Run deploy steps");

        let agent = sources
            .iter()
            .find(|s| s.name == "reviewer" && s.source_type == SourceType::Agent)
            .unwrap();
        assert_eq!(agent.description, "Code reviewer");
        assert!(agent.content.contains("You review code"));
    }

    #[tokio::test]
    async fn test_recipe_deduplication_local_wins() {
        let temp_dir = TempDir::new().unwrap();

        let local = temp_dir.path().join(".goose/recipes");
        fs::create_dir_all(&local).unwrap();
        fs::write(
            local.join("deploy.yaml"),
            "title: Deploy\ndescription: Local deploy\ninstructions: local steps",
        )
        .unwrap();

        let also_local = temp_dir.path().join(".agents/recipes");
        fs::create_dir_all(&also_local).unwrap();
        fs::write(
            also_local.join("deploy.yaml"),
            "title: Deploy\ndescription: Agents deploy\ninstructions: agents steps",
        )
        .unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let sources = client.discover_filesystem_sources(temp_dir.path());

        let deploys: Vec<_> = sources.iter().filter(|s| s.name == "deploy").collect();
        assert_eq!(deploys.len(), 1);
    }

    #[tokio::test]
    async fn test_load_recipe_source() {
        let temp_dir = TempDir::new().unwrap();

        let recipes = temp_dir.path().join(".goose/recipes");
        fs::create_dir_all(&recipes).unwrap();
        fs::write(
            recipes.join("deploy.yaml"),
            "title: Deploy\ndescription: Deploy to production\ninstructions: Run deploy steps",
        )
        .unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let result = client
            .handle_load_source("test", "deploy", temp_dir.path())
            .await
            .unwrap();

        let text = &result[0].as_text().expect("expected text content").text;
        assert!(text.contains("deploy"));
        assert!(text.contains("Run deploy steps"));
        assert!(text.contains("now available in your context"));
    }

    #[test]
    fn test_invalid_external_subrecipe_content_is_not_returned() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("invalid.yaml");
        fs::write(&path, "api_key: SUPERSECRET\n").unwrap();

        let recipe_file = load_local_recipe_file(path.to_str().unwrap()).unwrap();
        let error =
            SummonClient::format_subrecipe_content("invalid", &recipe_file.content).unwrap_err();

        assert_eq!(error, "Subrecipe 'invalid' is not a valid recipe");
        assert!(!error.contains("SUPERSECRET"));
    }

    #[test]
    fn test_valid_external_subrecipe_content_still_loads() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("child.yaml");
        fs::write(
            &path,
            "title: Child\ndescription: External child\ninstructions: Run child steps",
        )
        .unwrap();

        let recipe_file = load_local_recipe_file(path.to_str().unwrap()).unwrap();
        let content =
            SummonClient::format_subrecipe_content("child", &recipe_file.content).unwrap();

        assert_eq!(content, "Run child steps");
    }

    #[tokio::test]
    async fn test_load_agent_source() {
        let temp_dir = TempDir::new().unwrap();

        let agents = temp_dir.path().join(".goose/agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(
            agents.join("reviewer.md"),
            "---\nname: reviewer\nmodel: sonnet\ndescription: Code reviewer\n---\nYou review code carefully.",
        )
        .unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let result = client
            .handle_load_source("test", "reviewer", temp_dir.path())
            .await
            .unwrap();

        let text = &result[0].as_text().expect("expected text content").text;
        assert!(text.contains("reviewer"));
        assert!(text.contains("You review code carefully"));
        assert!(text.contains("now available in your context"));
    }

    #[tokio::test]
    async fn test_load_nonexistent_source_suggests_similar() {
        let temp_dir = TempDir::new().unwrap();

        let recipes = temp_dir.path().join(".goose/recipes");
        fs::create_dir_all(&recipes).unwrap();
        fs::write(
            recipes.join("deploy.yaml"),
            "title: Deploy\ndescription: Deploy to production\ninstructions: steps",
        )
        .unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let err = client
            .handle_load_source("test", "deploy-prod", temp_dir.path())
            .await
            .unwrap_err();

        assert!(err.contains("not found"));
        assert!(err.contains("deploy"), "should suggest 'deploy': {}", err);
    }

    #[tokio::test]
    async fn test_load_completely_unknown_source() {
        let temp_dir = TempDir::new().unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let err = client
            .handle_load_source("test", "zzz-nonexistent", temp_dir.path())
            .await
            .unwrap_err();

        assert!(err.contains("not found"));
        assert!(err.contains("Use load()"));
    }

    #[tokio::test]
    async fn test_client_tools_and_unknown_tool() {
        let client = SummonClient::new(create_test_context()).unwrap();

        let result = client
            .list_tools("test", None, CancellationToken::new())
            .await
            .unwrap();
        let names: Vec<_> = result.tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(names.contains(&"load") && names.contains(&"delegate"));

        let ctx = ToolCallContext::new("test".to_string(), None, None);
        let result = client
            .call_tool(&ctx, "unknown", None, CancellationToken::new())
            .await
            .unwrap();
        assert!(result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_context_injected_into_adhoc_recipe() {
        let temp_dir = TempDir::new().unwrap();
        let client = SummonClient::new(create_test_context()).unwrap();

        let params = DelegateParams {
            instructions: Some("do the task".to_string()),
            context: Some("background info".to_string()),
            ..Default::default()
        };

        let recipe = client
            .build_delegate_recipe(&params, "test", temp_dir.path())
            .await
            .unwrap();

        assert_eq!(
            recipe.instructions.as_deref(),
            Some("# Reference Context\n\nbackground info")
        );
        assert_eq!(recipe.prompt.as_deref(), Some("do the task"));
    }

    #[test]
    fn test_subrecipe_fixed_values_take_precedence_over_delegate_parameters() {
        let fixed = HashMap::from([("fixed".to_string(), "parent-value".to_string())]);
        let provided = HashMap::from([
            (
                "fixed".to_string(),
                serde_json::Value::String("delegate-value".to_string()),
            ),
            (
                "caller".to_string(),
                serde_json::Value::String("caller-value".to_string()),
            ),
        ]);

        let merged = merge_subrecipe_parameters(Some(&fixed), Some(&provided));

        assert_eq!(
            merged.get("fixed").map(String::as_str),
            Some("parent-value")
        );
        assert_eq!(
            merged.get("caller").map(String::as_str),
            Some("caller-value")
        );
    }

    #[test]
    fn test_build_instructions_with_context_wraps_existing_instructions() {
        assert_eq!(
            build_instructions_with_context("background info", "Run deploy steps"),
            "# Reference Context\n\nbackground info\n\n# Task Instructions\n\nRun deploy steps"
        );
        assert_eq!(
            build_instructions_with_context("background info", ""),
            "# Reference Context\n\nbackground info"
        );
    }

    #[test]
    fn test_validate_delegate_params_rejects_zero_max_turns() {
        let context = create_test_context();
        let client = SummonClient::new(context).unwrap();

        let params = DelegateParams {
            instructions: Some("do something".to_string()),
            max_turns: Some(0),
            ..Default::default()
        };
        let result = client.validate_delegate_params(&params);
        assert_eq!(result, Err("'max_turns' must be at least 1".to_string()));
    }

    #[test]
    fn test_validate_delegate_params_accepts_positive_max_turns() {
        let context = create_test_context();
        let client = SummonClient::new(context).unwrap();

        let params = DelegateParams {
            instructions: Some("do something".to_string()),
            max_turns: Some(5),
            ..Default::default()
        };
        assert!(client.validate_delegate_params(&params).is_ok());
    }

    #[test]
    #[serial]
    fn test_resolve_max_turns_recipe_overrides_env_var() {
        let context = create_test_context();
        let client = SummonClient::new(context).unwrap();

        let session = crate::session::Session {
            recipe: Some(crate::recipe::Recipe {
                version: "1.0.0".to_string(),
                title: String::new(),
                description: String::new(),
                instructions: None,
                prompt: None,
                extensions: None,
                settings: Some(crate::recipe::Settings {
                    goose_provider: None,
                    goose_model: None,
                    temperature: None,
                    max_turns: Some(10),
                }),
                activities: None,
                author: None,
                parameters: None,
                response: None,
                sub_recipes: None,
                retry: None,
            }),
            ..Default::default()
        };

        // Set env var to a different value — recipe should still win
        let _env = env_lock::lock_env([("GOOSE_SUBAGENT_MAX_TURNS", Some("99"))]);
        let result = client.resolve_max_turns(&session);

        assert_eq!(
            result, 10,
            "recipe settings.max_turns should take priority over env var"
        );
    }

    #[test]
    #[serial]
    fn test_resolve_max_turns_falls_back_to_env_var() {
        let context = create_test_context();
        let client = SummonClient::new(context).unwrap();

        let session = crate::session::Session::default(); // no recipe

        let _env = env_lock::lock_env([("GOOSE_SUBAGENT_MAX_TURNS", Some("7"))]);
        let result = client.resolve_max_turns(&session);

        assert_eq!(
            result, 7,
            "should fall back to GOOSE_SUBAGENT_MAX_TURNS env var"
        );
    }

    #[test]
    #[serial]
    fn test_resolve_max_turns_falls_back_to_default() {
        let context = create_test_context();
        let client = SummonClient::new(context).unwrap();

        let session = crate::session::Session::default(); // no recipe

        std::env::remove_var("GOOSE_SUBAGENT_MAX_TURNS");
        let result = client.resolve_max_turns(&session);

        assert_eq!(
            result,
            crate::agents::subagent_task_config::DEFAULT_SUBAGENT_MAX_TURNS,
            "should fall back to DEFAULT_SUBAGENT_MAX_TURNS"
        );
    }

    fn empty_recipe() -> crate::recipe::Recipe {
        crate::recipe::Recipe {
            version: "1.0.0".to_string(),
            title: String::new(),
            description: String::new(),
            instructions: None,
            prompt: None,
            extensions: None,
            settings: None,
            activities: None,
            author: None,
            parameters: None,
            response: None,
            sub_recipes: None,
            retry: None,
        }
    }

    #[tokio::test]
    #[serial]
    async fn foreground_delegate_rejects_unregistered_standard_provider() {
        let temp_dir = TempDir::new().unwrap();
        let parent_provider: Arc<dyn crate::providers::base::Provider> = Arc::new(
            crate::providers::testprovider::TestProvider::new_replaying(
                temp_dir.path().join("records.json").display().to_string(),
            )
            .unwrap(),
        );
        let context = create_test_context();
        let providers = context.providers.clone();
        let client = SummonClient::new(context).unwrap();
        let session = crate::session::Session {
            id: "unregistered-parent".to_string(),
            provider_name: Some(parent_provider.get_name().to_string()),
            model_config: Some(goose_providers::model::ModelConfig::new("test-model")),
            working_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        providers
            .set_standard_provider(&session.id, Arc::clone(&parent_provider))
            .await;

        let params = DelegateParams {
            instructions: Some("Review the change".to_string()),
            extensions: Some(Vec::new()),
            provider: Some(parent_provider.get_name().to_string()),
            model: Some("test-model".to_string()),
            ..Default::default()
        };
        let error = client
            .handle_foreground_delegate(params, &session)
            .await
            .unwrap_err();
        assert!(error.contains("cannot be reconstructed for a foreground subagent"));
    }

    #[tokio::test]
    #[serial]
    async fn foreground_delegate_rejects_sharing_unregistered_acp() {
        let temp_dir = TempDir::new().unwrap();
        let acp = Arc::new(crate::acp::AcpProvider::new_test_stub());
        let context = create_test_context();
        let providers = context.providers.clone();
        let client = SummonClient::new(context).unwrap();
        let session = crate::session::Session {
            id: "acp-parent".to_string(),
            provider_name: Some(acp.name().to_string()),
            model_config: Some(goose_providers::model::ModelConfig::new("test-model")),
            working_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let params = DelegateParams {
            instructions: Some("Review the change".to_string()),
            extensions: Some(Vec::new()),
            provider: Some(acp.name().to_string()),
            model: Some("test-model".to_string()),
            ..Default::default()
        };
        assert!(providers::get_from_registry(acp.name()).await.is_err());

        providers
            .set_backend(&session.id, ProviderBackend::Acp(acp))
            .await;
        let error = client
            .handle_foreground_delegate(params, &session)
            .await
            .unwrap_err();
        assert!(error.contains("cannot be reconstructed for a foreground subagent"));
    }

    const PARENT_MODEL: &str = "claude-3-5-sonnet-20241022";
    const OVERRIDE_MODEL: &str = "claude-opus-4-6";
    const PROVIDER: &str = "anthropic";

    fn session_with(parent: goose_providers::model::ModelConfig) -> crate::session::Session {
        crate::session::Session {
            provider_name: Some(PROVIDER.to_string()),
            model_config: Some(parent),
            ..Default::default()
        }
    }

    fn resolve_with_override(
        model: Option<&str>,
        parent: goose_providers::model::ModelConfig,
    ) -> goose_providers::model::ModelConfig {
        let client = SummonClient::new(create_test_context()).unwrap();
        let params = DelegateParams {
            model: model.map(String::from),
            ..Default::default()
        };
        client
            .resolve_model_config(
                &params,
                &empty_recipe(),
                &session_with(parent),
                PROVIDER,
                None,
            )
            .expect("resolve_model_config")
    }

    fn parent_config() -> goose_providers::model::ModelConfig {
        goose_providers::model::ModelConfig::new(PARENT_MODEL).with_canonical_limits(PROVIDER)
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_applies_canonical_limits_to_overridden_model() {
        let _env = env_lock::lock_env([
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_SUBAGENT_MODEL", None::<&str>),
        ]);

        let parent = parent_config();
        let overridden = goose_providers::model::ModelConfig::new(OVERRIDE_MODEL)
            .with_canonical_limits(PROVIDER);
        assert_ne!(parent.reasoning, overridden.reasoning);

        let resolved = resolve_with_override(Some(OVERRIDE_MODEL), parent);

        assert_eq!(resolved.model_name, OVERRIDE_MODEL);
        assert_eq!(resolved.max_tokens, overridden.max_tokens);
        assert_eq!(resolved.reasoning, overridden.reasoning);
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_does_not_inherit_provider_specific_request_params() {
        let _env = env_lock::lock_env([
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_SUBAGENT_MODEL", None::<&str>),
        ]);

        // Parent session is a Claude model with anthropic_beta in request_params.
        // When delegate() overrides to a different model (e.g. Gemini), provider-
        // specific params like anthropic_beta must not bleed through — they would
        // cause a 400 INVALID_ARGUMENT from the target API.
        let mut parent = parent_config();
        parent.request_params = Some(HashMap::from([(
            "anthropic_beta".to_string(),
            serde_json::json!("custom-beta-header"),
        )]));

        let resolved = resolve_with_override(Some(OVERRIDE_MODEL), parent);

        assert_eq!(
            resolved
                .request_params
                .as_ref()
                .and_then(|p| p.get("anthropic_beta")),
            None,
            "anthropic_beta must not be inherited by a child session with a different model"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_inherits_thinking_effort_on_override() {
        let _env = env_lock::lock_env([
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_SUBAGENT_MODEL", None::<&str>),
        ]);

        // Reasoning controls are model-family-agnostic and should be inherited,
        // while provider-specific params like anthropic_beta must not.
        let mut parent = parent_config();
        parent.request_params = Some(HashMap::from([
            ("thinking_effort".to_string(), serde_json::json!("high")),
            ("budget_tokens".to_string(), serde_json::json!(8192)),
            (
                "anthropic_beta".to_string(),
                serde_json::json!("custom-beta-header"),
            ),
        ]));

        let resolved = resolve_with_override(Some(OVERRIDE_MODEL), parent);

        assert_eq!(
            resolved
                .request_params
                .as_ref()
                .and_then(|p| p.get("thinking_effort")),
            Some(&serde_json::json!("high")),
            "thinking_effort should be inherited across model families"
        );
        assert_eq!(
            resolved
                .request_params
                .as_ref()
                .and_then(|p| p.get("budget_tokens")),
            Some(&serde_json::json!(8192)),
            "budget_tokens should be inherited across model families"
        );
        assert_eq!(
            resolved
                .request_params
                .as_ref()
                .and_then(|p| p.get("anthropic_beta")),
            None,
            "anthropic_beta must not be inherited alongside reasoning controls"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_env_var_overrides_params_model() {
        let _env = env_lock::lock_env([
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_SUBAGENT_MODEL", Some(OVERRIDE_MODEL)),
        ]);

        let client = SummonClient::new(create_test_context()).unwrap();
        let params = DelegateParams {
            model: Some("params-model".to_string()),
            ..Default::default()
        };
        let result = client
            .resolve_model_config(
                &params,
                &empty_recipe(),
                &session_with(parent_config()),
                PROVIDER,
                None,
            )
            .expect("resolve_model_config");
        assert_eq!(
            result.model_name, OVERRIDE_MODEL,
            "GOOSE_SUBAGENT_MODEL must take priority over params.model"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_recipe_overrides_env_var() {
        let _env = env_lock::lock_env([
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_SUBAGENT_MODEL", Some(OVERRIDE_MODEL)),
        ]);

        let client = SummonClient::new(create_test_context()).unwrap();
        let mut recipe = empty_recipe();
        recipe.settings = Some(crate::recipe::Settings {
            goose_provider: None,
            goose_model: Some("recipe-model".to_string()),
            temperature: None,
            max_turns: None,
        });
        let result = client
            .resolve_model_config(
                &DelegateParams::default(),
                &recipe,
                &session_with(parent_config()),
                PROVIDER,
                None,
            )
            .expect("resolve_model_config");
        assert_eq!(
            result.model_name, "recipe-model",
            "recipe settings.goose_model must take priority over GOOSE_SUBAGENT_MODEL"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_provider_config_recipe_overrides_env_var() {
        let _env = env_lock::lock_env([
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_SUBAGENT_PROVIDER", Some("openai")),
            ("GOOSE_SUBAGENT_MODEL", None::<&str>),
            ("ANTHROPIC_API_KEY", Some("test-key")),
        ]);

        let client = SummonClient::new(create_test_context()).unwrap();
        let mut recipe = empty_recipe();
        recipe.settings = Some(crate::recipe::Settings {
            goose_provider: Some(PROVIDER.to_string()),
            goose_model: None,
            temperature: None,
            max_turns: None,
        });
        let (provider_name, _) = client
            .resolve_provider_config(
                &DelegateParams::default(),
                &recipe,
                &session_with(parent_config()),
            )
            .await
            .expect("resolve_provider_config");
        assert_eq!(
            provider_name, PROVIDER,
            "recipe settings.goose_provider must take priority over GOOSE_SUBAGENT_PROVIDER"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_recipe_provider_rejects_env_model_of_other_provider() {
        let _env = env_lock::lock_env([
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_SUBAGENT_PROVIDER", Some("openai")),
            ("GOOSE_SUBAGENT_MODEL", Some("gpt-5.2")),
            ("ANTHROPIC_API_KEY", Some("test-key")),
        ]);

        let client = SummonClient::new(create_test_context()).unwrap();
        let mut recipe = empty_recipe();
        recipe.settings = Some(crate::recipe::Settings {
            goose_provider: Some(PROVIDER.to_string()),
            goose_model: None,
            temperature: None,
            max_turns: None,
        });
        let session = crate::session::Session::default();
        let (_, result) = client
            .resolve_provider_config(&DelegateParams::default(), &recipe, &session)
            .await
            .expect("resolve_provider_config");

        assert_ne!(
            result.model_name, "gpt-5.2",
            "env model for another provider must not be sent to the recipe provider"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_env_provider_uses_provider_default_model() {
        let _env = env_lock::lock_env([
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_SUBAGENT_PROVIDER", Some(PROVIDER)),
            ("GOOSE_SUBAGENT_MODEL", None::<&str>),
            ("ANTHROPIC_API_KEY", Some("test-key")),
        ]);

        let client = SummonClient::new(create_test_context()).unwrap();
        let params = DelegateParams::default();
        let default_model = providers::get_from_registry(PROVIDER)
            .await
            .unwrap()
            .metadata()
            .default_model
            .clone();
        let session = crate::session::Session::default();
        let (_, result) = client
            .resolve_provider_config(&params, &empty_recipe(), &session)
            .await
            .expect("resolve_provider_config");

        assert_eq!(result.model_name, default_model);
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_env_provider_keeps_matching_params_model() {
        let _env = env_lock::lock_env([
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_SUBAGENT_PROVIDER", Some(PROVIDER)),
            ("GOOSE_SUBAGENT_MODEL", None::<&str>),
            ("ANTHROPIC_API_KEY", Some("test-key")),
        ]);

        let client = SummonClient::new(create_test_context()).unwrap();
        let params = DelegateParams {
            provider: Some(PROVIDER.to_string()),
            model: Some(OVERRIDE_MODEL.to_string()),
            ..Default::default()
        };
        let (_, result) = client
            .resolve_provider_config(&params, &empty_recipe(), &session_with(parent_config()))
            .await
            .expect("resolve_provider_config");

        assert_eq!(result.model_name, OVERRIDE_MODEL);
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_dynamic_provider_requires_model() {
        let _env = env_lock::lock_env([
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_SUBAGENT_MODEL", None::<&str>),
        ]);

        let default_model = providers::get_from_registry("lmstudio")
            .await
            .unwrap()
            .metadata()
            .default_model
            .clone();
        assert!(default_model.is_empty());

        let client = SummonClient::new(create_test_context()).unwrap();
        let params = DelegateParams {
            provider: Some("openai".to_string()),
            model: Some("openai-model".to_string()),
            ..Default::default()
        };
        let session = crate::session::Session {
            provider_name: Some("openai".to_string()),
            model_config: Some(goose_providers::model::ModelConfig::new(
                "parent-openai-model",
            )),
            ..Default::default()
        };
        let error = client
            .resolve_model_config(
                &params,
                &empty_recipe(),
                &session,
                "lmstudio",
                Some(&default_model),
            )
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("No model configured for provider 'lmstudio'"));
    }
}
