pub mod analyze;
pub mod apps;
pub mod chatrecall;
#[cfg(feature = "code-mode")]
pub mod code_execution;
pub mod developer;
pub mod ext_manager;
pub mod orchestrator;
pub mod skills;
pub mod summarize;
pub mod summon;
pub mod todo;
pub mod tom;

use std::collections::HashMap;
use std::path::PathBuf;

use crate::agents::mcp_client::McpClientTrait;
use crate::session::Session;
use once_cell::sync::Lazy;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    pub kind: SourceKind,
    pub description: String,
    pub path: PathBuf,
    pub content: String,
    pub supporting_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceKind {
    Subrecipe,
    Recipe,
    Skill,
    Agent,
    BuiltinSkill,
}

impl std::fmt::Display for SourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceKind::Subrecipe => write!(f, "subrecipe"),
            SourceKind::Recipe => write!(f, "recipe"),
            SourceKind::Skill => write!(f, "skill"),
            SourceKind::Agent => write!(f, "agent"),
            SourceKind::BuiltinSkill => write!(f, "builtin skill"),
        }
    }
}

impl Source {
    pub fn to_load_text(&self) -> String {
        format!(
            "## {} ({})\n\n{}\n\n### Content\n\n{}",
            self.name, self.kind, self.description, self.content
        )
    }
}

pub fn parse_frontmatter<T: for<'de> Deserialize<'de>>(
    content: &str,
) -> Result<Option<(T, String)>, serde_yaml::Error> {
    let parts: Vec<&str> = content.split("---").collect();
    if parts.len() < 3 {
        return Ok(None);
    }

    let yaml_content = parts[1].trim();
    let metadata: T = serde_yaml::from_str(yaml_content)?;

    let body = parts[2..].join("---").trim().to_string();
    Ok(Some((metadata, body)))
}

pub use ext_manager::MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE;

// These are used by integration tests in crates/goose/tests/
#[allow(unused_imports)]
pub use ext_manager::MANAGE_EXTENSIONS_TOOL_NAME;
#[allow(unused_imports)]
pub use ext_manager::SEARCH_AVAILABLE_EXTENSIONS_TOOL_NAME;

pub static PLATFORM_EXTENSIONS: Lazy<HashMap<&'static str, PlatformExtensionDef>> = Lazy::new(
    || {
        let mut map = HashMap::new();

        map.insert(
            analyze::EXTENSION_NAME,
            PlatformExtensionDef {
                name: analyze::EXTENSION_NAME,
                display_name: "Analyze",
                description:
                    "Analyze code structure with tree-sitter: directory overviews, file details, symbol call graphs",
                default_enabled: true,
                unprefixed_tools: true,
                hidden: false,
                client_factory: |ctx| Box::new(analyze::AnalyzeClient::new(ctx).unwrap()),
            },
        );

        map.insert(
            todo::EXTENSION_NAME,
            PlatformExtensionDef {
                name: todo::EXTENSION_NAME,
                display_name: "Todo",
                description:
                    "Enable a todo list for goose so it can keep track of what it is doing",
                default_enabled: true,
                unprefixed_tools: false,
                hidden: false,
                client_factory: |ctx| Box::new(todo::TodoClient::new(ctx).unwrap()),
            },
        );

        map.insert(
            apps::EXTENSION_NAME,
            PlatformExtensionDef {
                name: apps::EXTENSION_NAME,
                display_name: "Apps",
                description:
                    "Create and manage custom Goose apps through chat. Apps are HTML/CSS/JavaScript and run in sandboxed windows.",
                default_enabled: true,
                unprefixed_tools: false,
                hidden: false,
                client_factory: |ctx| Box::new(apps::AppsManagerClient::new(ctx).unwrap()),
            },
        );

        map.insert(
            chatrecall::EXTENSION_NAME,
            PlatformExtensionDef {
                name: chatrecall::EXTENSION_NAME,
                display_name: "Chat Recall",
                description:
                    "Search past conversations and load session summaries for contextual memory",
                default_enabled: false,
                unprefixed_tools: false,
                hidden: false,
                client_factory: |ctx| Box::new(chatrecall::ChatRecallClient::new(ctx).unwrap()),
            },
        );

        map.insert(
            "extensionmanager",
            PlatformExtensionDef {
                name: ext_manager::EXTENSION_NAME,
                display_name: "Extension Manager",
                description:
                    "Enable extension management tools for discovering, enabling, and disabling extensions",
                default_enabled: true,
                unprefixed_tools: false,
                hidden: false,
                client_factory: |ctx| Box::new(ext_manager::ExtensionManagerClient::new(ctx).unwrap()),
            },
        );

        map.insert(
            summon::EXTENSION_NAME,
            PlatformExtensionDef {
                name: summon::EXTENSION_NAME,
                display_name: "Summon",
                description: "Load knowledge and delegate tasks to subagents",
                default_enabled: true,
                unprefixed_tools: true,
                hidden: false,
                client_factory: |ctx| Box::new(summon::SummonClient::new(ctx).unwrap()),
            },
        );

        map.insert(
            summarize::EXTENSION_NAME,
            PlatformExtensionDef {
                name: summarize::EXTENSION_NAME,
                display_name: "Summarize",
                description: "Load files/directories and get an LLM summary in a single call",
                default_enabled: false,
                unprefixed_tools: false,
                hidden: false,
                client_factory: |ctx| Box::new(summarize::SummarizeClient::new(ctx).unwrap()),
            },
        );

        #[cfg(feature = "code-mode")]
        map.insert(
            code_execution::EXTENSION_NAME,
            PlatformExtensionDef {
                name: code_execution::EXTENSION_NAME,
                display_name: "Code Mode",
                description:
                    "Goose will make extension calls through code execution, saving tokens",
                default_enabled: false,
                unprefixed_tools: true,
                hidden: false,
                client_factory: |ctx| {
                    Box::new(
                        code_execution::CodeExecutionClient::new(
                            ctx,
                            code_execution::get_tool_disclosure(),
                        )
                        .unwrap(),
                    )
                },
            },
        );

        map.insert(
            developer::EXTENSION_NAME,
            PlatformExtensionDef {
                name: developer::EXTENSION_NAME,
                display_name: "Developer",
                description: "Write and edit files, and execute shell commands",
                default_enabled: true,
                unprefixed_tools: true,
                hidden: false,
                client_factory: |ctx| Box::new(developer::DeveloperClient::new(ctx).unwrap()),
            },
        );

        map.insert(
            orchestrator::EXTENSION_NAME,
            PlatformExtensionDef {
                name: orchestrator::EXTENSION_NAME,
                display_name: "Orchestrator",
                description:
                    "Manage agent sessions: list, view, start, send messages, interrupt, and stop agents",
                default_enabled: false,
                unprefixed_tools: false,
                hidden: true,
                client_factory: |ctx| Box::new(orchestrator::OrchestratorClient::new(ctx).unwrap()),
            },
        );

        map.insert(
            tom::EXTENSION_NAME,
            PlatformExtensionDef {
                name: tom::EXTENSION_NAME,
                display_name: "Top Of Mind",
                description:
                    "Inject custom context into every turn via GOOSE_MOIM_MESSAGE_TEXT and GOOSE_MOIM_MESSAGE_FILE environment variables",
                default_enabled: true,
                unprefixed_tools: false,
                hidden: false,
                client_factory: |ctx| Box::new(tom::TomClient::new(ctx).unwrap()),
            },
        );

        map.insert(
            skills::EXTENSION_NAME,
            PlatformExtensionDef {
                name: skills::EXTENSION_NAME,
                display_name: "Skills",
                description: "Discover and provide skill instructions from filesystem and builtins",
                default_enabled: true,
                unprefixed_tools: true,
                hidden: false,
                client_factory: |ctx| Box::new(skills::SkillsClient::new(ctx).unwrap()),
            },
        );

        map
    },
);

#[derive(Clone)]
pub struct PlatformExtensionContext {
    pub extension_manager:
        Option<std::sync::Weak<crate::agents::extension_manager::ExtensionManager>>,
    pub session_manager: std::sync::Arc<crate::session::SessionManager>,
    pub session: Option<std::sync::Arc<Session>>,
}

impl PlatformExtensionContext {
    pub fn result_with_platform_notification(
        &self,
        mut result: rmcp::model::CallToolResult,
        extension_name: impl Into<String>,
        event_type: impl Into<String>,
        mut additional_params: serde_json::Map<String, serde_json::Value>,
    ) -> rmcp::model::CallToolResult {
        additional_params.insert("extension".to_string(), extension_name.into().into());
        additional_params.insert("event_type".to_string(), event_type.into().into());

        let meta_value = serde_json::json!({
            "platform_notification": {
                "method": "platform_event",
                "params": additional_params
            }
        });

        if let Some(ref mut meta) = result.meta {
            if let Some(obj) = meta_value.as_object() {
                for (k, v) in obj {
                    meta.0.insert(k.clone(), v.clone());
                }
            }
        } else {
            result.meta = Some(rmcp::model::Meta(meta_value.as_object().unwrap().clone()));
        }

        result
    }
}

/// Definition for a platform extension that runs in-process with direct agent access.
#[derive(Debug, Clone)]
pub struct PlatformExtensionDef {
    pub name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub default_enabled: bool,
    /// If true, tools are exposed without extension prefix for intuitive first-class use.
    pub unprefixed_tools: bool,
    /// If true, the extension is not shown in the UI or discoverable via search_available_extensions.
    pub hidden: bool,
    pub client_factory: fn(PlatformExtensionContext) -> Box<dyn McpClientTrait>,
}
