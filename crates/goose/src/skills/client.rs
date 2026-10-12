use super::discover_skills_with_config;
use super::loaded_skill_context_with_args;
use crate::agents::extension_manager::ExtensionLease;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::platform_extensions::PlatformExtensionContext;
use crate::agents::ToolCallContext;
use crate::config::Config;
use async_trait::async_trait;
use goose_sdk_types::custom_requests::{SourceEntry, SourceType};
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, ServerNotification, Tool,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "skills";

pub struct SkillsClient {
    info: InitializeResult,
    exclude_builtin_skills: bool,
    config: &'static Config,
    extension_manager: Option<std::sync::Weak<crate::agents::extension_manager::ExtensionManager>>,
}

impl Default for SkillsClient {
    fn default() -> Self {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(EXTENSION_NAME, "1.0.0").with_title("Skills"));

        Self {
            info,
            exclude_builtin_skills: false,
            config: Config::global(),
            extension_manager: None,
        }
    }
}

impl SkillsClient {
    pub fn new(context: PlatformExtensionContext) -> Self {
        Self {
            extension_manager: context.extension_manager,
            ..Self::default()
        }
    }

    /// Controls whether Goose's bundled skills are exposed by this client.
    /// Bundled skills are enabled by default.
    pub fn with_builtin_skills(mut self, enabled: bool) -> Self {
        self.exclude_builtin_skills = !enabled;
        self
    }

    #[cfg(test)]
    fn with_config(mut self, config: &'static Config) -> Self {
        self.config = config;
        self
    }

    async fn discover_skills(
        &self,
        session_id: &str,
        working_dir: &Path,
        lease: Option<&ExtensionLease>,
    ) -> Vec<SourceEntry> {
        let filesystem = discover_skills_with_config(Some(working_dir), self.config)
            .into_iter()
            .filter(|skill| {
                !self.exclude_builtin_skills || skill.source_type != SourceType::BuiltinSkill
            })
            .collect();
        let mut mcp = match lease {
            Some(lease) => lease.list_mcp_skills().await,
            None => Vec::new(),
        };
        if let Some(lease) = lease {
            mcp.extend(
                super::mcp::held_skill_records(session_id)
                    .into_iter()
                    .filter(|skill| lease.is_enabled(&skill.extension_name)),
            );
        }
        super::mcp::merge_skill_entries(filesystem, mcp)
    }

    fn tool_error(message: impl std::fmt::Display) -> CallToolResult {
        CallToolResult::error(vec![ContentBlock::text(message.to_string())])
    }

    async fn load_mcp_skill(
        &self,
        session_id: &str,
        skill: &SourceEntry,
        args: Option<&str>,
        lease: &ExtensionLease,
    ) -> CallToolResult {
        let Some(extension_name) = skill
            .properties
            .get("mcpExtension")
            .and_then(|value| value.as_str())
        else {
            return Self::tool_error(format!("Skill '{}' has no serving extension.", skill.name));
        };
        let manifest = match lease.send_skill_get(extension_name, &skill.path).await {
            Ok(manifest) => manifest,
            Err(error) => return Self::tool_error(error),
        };
        self.load_mcp_skill_with_manifest(session_id, skill, args, lease, manifest)
            .await
    }

    async fn load_mcp_skill_with_manifest(
        &self,
        session_id: &str,
        skill: &SourceEntry,
        args: Option<&str>,
        lease: &ExtensionLease,
        manifest: super::mcp::SkillRecord,
    ) -> CallToolResult {
        if let Err(error) = super::mcp::ensure_loadable(&manifest) {
            return Self::tool_error(error);
        }
        let extension_name = manifest.extension_name.as_str();
        let markdown = match lease.read_skill_text(extension_name, &skill.path).await {
            Ok(markdown) => markdown,
            Err(error) => return Self::tool_error(error),
        };
        if let Err(error) = super::mcp::verify_skill_markdown(&manifest, &markdown) {
            return Self::tool_error(error);
        }
        let Some(body) = super::mcp::skill_markdown_body(&markdown) else {
            return Self::tool_error(super::mcp::SkillLoadError::MissingBody {
                uri: manifest.uri.clone(),
            });
        };
        let mut loaded = skill.clone();
        loaded.content = body.to_string();
        loaded.supporting_files.clear();
        match loaded_skill_context_with_args(&loaded, args) {
            Ok(rendered) => {
                let tagged = super::mcp::origin_tagged_context(
                    &manifest.extension_name,
                    &manifest.uri,
                    &rendered,
                );
                super::mcp::hold_skill(session_id, manifest);
                CallToolResult::success(vec![ContentBlock::text(tagged)])
            }
            Err(error) => Self::tool_error(format!("Failed to parse skill arguments: {error}")),
        }
    }

    async fn load_mcp_supporting_file(
        &self,
        session_id: &str,
        skill: &SourceEntry,
        relative_path: &str,
        lease: &ExtensionLease,
    ) -> CallToolResult {
        let Some(extension_name) = skill
            .properties
            .get("mcpExtension")
            .and_then(|value| value.as_str())
        else {
            return Self::tool_error(format!("Skill '{}' has no serving extension.", skill.name));
        };
        let manifest = match super::mcp::held_skill(session_id, extension_name, &skill.path) {
            Some(manifest) => manifest,
            None => {
                return Self::tool_error(format!(
                    "Load '{}' before reading its supporting files.",
                    skill.name
                ));
            }
        };
        let uri = match super::mcp::resolve_relative_uri(&manifest, relative_path) {
            Ok(uri) => uri,
            Err(error) => return Self::tool_error(error),
        };
        if let Err(error) = super::mcp::listed_file(&manifest, &uri) {
            return Self::tool_error(error);
        }
        let content = match lease.read_skill_resource(extension_name, &uri).await {
            Ok(content) => content,
            Err(error) => return Self::tool_error(error),
        };
        if let Err(error) = super::mcp::verify_resource_contents(&manifest, &uri, &content) {
            return Self::tool_error(error);
        }
        CallToolResult::success(vec![ContentBlock::resource(content)])
    }
}

#[async_trait]
impl McpClientTrait for SkillsClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the skill to load. MCP skills use extension::skill. Use skill-name/path for a supporting file, or extension::URI for a server-provided skill URI."
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments to provide when loading the skill."
                }
            }
        });

        let tool = Tool::new(
            "load_skill",
            "Load a skill's full content into your context so you can follow its instructions.\n\n\
             Skills are listed in your system instructions. When you need to use one, \
             load it first to get the detailed instructions.\n\n\
             Examples:\n\
             - load_skill(name: \"gdrive\") → Loads the gdrive skill instructions\n\
             - load_skill(name: \"my-skill\", args: \"the arguments for the skill\") → Loads a skill with arguments\n\
             - load_skill(name: \"my-skill/template.md\") → Loads a supporting file"
                .to_string(),
            schema.as_object().unwrap().clone(),
        );

        Ok(ListToolsResult {
            tools: vec![tool],
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
        if name != "load_skill" {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Unknown tool: {}",
                name
            ))]));
        }

        let skill_name = arguments
            .as_ref()
            .and_then(|args| args.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if skill_name.is_empty() {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "Missing required parameter: name",
            )]));
        }
        let args = arguments
            .as_ref()
            .and_then(|args| args.get("args"))
            .and_then(|v| v.as_str());

        let working_dir = ctx
            .working_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let lease = ctx.extension_lease().map(Arc::as_ref);
        if let Some((extension_name, uri)) = super::mcp::direct_skill_reference(skill_name) {
            let Some(lease) = lease else {
                return Ok(Self::tool_error("Skill server is not connected."));
            };
            let manifest = match lease.send_skill_get(extension_name, uri).await {
                Ok(manifest) => manifest,
                Err(error) => return Ok(Self::tool_error(error)),
            };
            let published = super::mcp::qualified_skill_name(extension_name, &manifest.name);
            let skill = super::mcp::source_entry(&manifest, &published);
            return Ok(self
                .load_mcp_skill_with_manifest(&ctx.session_id, &skill, args, lease, manifest)
                .await);
        }
        let skills = self
            .discover_skills(&ctx.session_id, &working_dir, lease)
            .await;

        if let Some((skill, relative_path)) = super::mcp::requested_skill(&skills, skill_name) {
            if super::mcp::is_mcp_skill(skill) {
                let Some(lease) = lease else {
                    return Ok(Self::tool_error("Skill server is not connected."));
                };
                return Ok(match relative_path {
                    Some(path) => {
                        self.load_mcp_supporting_file(&ctx.session_id, skill, path, lease)
                            .await
                    }
                    None => {
                        self.load_mcp_skill(&ctx.session_id, skill, args, lease)
                            .await
                    }
                });
            }
            if let Some(relative_path) = relative_path {
                let listed_skill_dir = PathBuf::from(&skill.path);
                let load_skill_dir = match listed_skill_dir.canonicalize() {
                    Ok(path) => path,
                    Err(error) => {
                        return Ok(Self::tool_error(format!(
                            "Failed to resolve '{}': {error}",
                            skill.name
                        )));
                    }
                };
                for file_path in &skill.supporting_files {
                    let file_path = Path::new(file_path);
                    let Ok(relative) = file_path.strip_prefix(&listed_skill_dir) else {
                        continue;
                    };
                    if relative.to_string_lossy().replace('\\', "/") != relative_path {
                        continue;
                    }
                    return Ok(
                        match super::load_supporting_file(&load_skill_dir, relative, skill_name) {
                            Ok(content) => {
                                CallToolResult::success(vec![ContentBlock::text(content)])
                            }
                            Err(error) => {
                                Self::tool_error(format!("Failed to read '{skill_name}': {error}"))
                            }
                        },
                    );
                }
                let available: Vec<String> = skill
                    .supporting_files
                    .iter()
                    .filter_map(|file| {
                        Path::new(file)
                            .strip_prefix(&listed_skill_dir)
                            .ok()
                            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                    })
                    .take(10)
                    .collect();
                return Ok(if available.is_empty() {
                    Self::tool_error(format!("Skill '{}' has no supporting files.", skill.name))
                } else {
                    Self::tool_error(format!(
                        "File '{skill_name}' not found. Available: {}",
                        available.join(", ")
                    ))
                });
            }
            return Ok(match loaded_skill_context_with_args(skill, args) {
                Ok(rendered) => CallToolResult::success(vec![ContentBlock::text(rendered)]),
                Err(error) => Self::tool_error(format!("Failed to parse skill arguments: {error}")),
            });
        }

        let suggestions: Vec<&str> = skills
            .iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&skill_name.to_lowercase())
                    || skill_name.to_lowercase().contains(&s.name.to_lowercase())
            })
            .take(3)
            .map(|s| s.name.as_str())
            .collect();

        Ok(if suggestions.is_empty() {
            CallToolResult::error(vec![ContentBlock::text(format!(
                "Skill '{}' not found.",
                skill_name
            ))])
        } else {
            CallToolResult::error(vec![ContentBlock::text(format!(
                "Skill '{}' not found. Did you mean: {}?",
                skill_name,
                suggestions.join(", ")
            ))])
        })
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn get_instructions(&self, session_id: &str, working_dir: &Path) -> Option<String> {
        let lease = match self
            .extension_manager
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
        {
            Some(manager) => manager.current_lease(session_id).await.ok(),
            None => None,
        };
        let sources = self
            .discover_skills(session_id, working_dir, lease.as_ref())
            .await;
        let mut skills: Vec<&SourceEntry> = sources
            .iter()
            .filter(|skill| {
                matches!(
                    skill.source_type,
                    SourceType::Skill | SourceType::BuiltinSkill | SourceType::McpSkill
                )
            })
            .collect();
        skills.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));

        if skills.is_empty() {
            return None;
        }

        let mut instructions = String::from(
            "\n\nYou have these skills at your disposal, when it is clear they can help you solve a problem or you are asked to use them:",
        );
        for skill in &skills {
            if skill.source_type == SourceType::McpSkill {
                let Some(origin) = skill
                    .properties
                    .get("mcpExtension")
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let name = super::mcp::prompt_text(&skill.name);
                let origin = super::mcp::prompt_text(origin);
                let description = super::mcp::prompt_text(&skill.description);
                let collision = skill
                    .properties
                    .get("nameCollision")
                    .and_then(serde_json::Value::as_bool)
                    .is_some_and(|collision| collision);
                instructions.push_str(&format!(
                    "\n• {} [untrusted MCP skill from {}{}] - {}",
                    name,
                    origin,
                    if collision { ", name collision" } else { "" },
                    description
                ));
            } else {
                instructions.push_str(&format!("\n• {} - {}", skill.name, skill.description));
            }
        }
        Some(instructions)
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        let (_tx, rx) = mpsc::channel(1);
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    fn write_plugin_skill(
        project: &Path,
        plugin_name: &str,
        skill_name: &str,
        description: &str,
        body: &str,
    ) {
        let skill_dir = project
            .join(".agents/plugins")
            .join(plugin_name)
            .join("skills")
            .join(skill_name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {skill_name}\ndescription: {description}\n---\n{body}"),
        )
        .unwrap();
    }

    fn write_open_plugin_manifest(project: &Path, plugin_name: &str) {
        let plugin_dir = project.join(".agents/plugins").join(plugin_name);
        fs::write(
            plugin_dir.join("plugin.json"),
            format!(
                r#"{{"name":"{plugin_name}","skills":{{"paths":["./skills","./custom-skills"]}}}}"#
            ),
        )
        .unwrap();
    }

    fn test_client(project: &Path, plugin_name: &str, enabled: bool) -> SkillsClient {
        let config = Box::leak(Box::new(
            Config::new(project.join("test-config.yaml"), "goose-skills-test").unwrap(),
        ));
        let plugin_root = project.join(".agents/plugins").join(plugin_name);
        config
            .set_param(
                "plugins",
                HashMap::from([(
                    plugin_root.to_string_lossy().into_owned(),
                    HashMap::from([("enabled", enabled)]),
                )]),
            )
            .unwrap();
        SkillsClient::default()
            .with_builtin_skills(false)
            .with_config(config)
    }

    fn result_text(result: &CallToolResult) -> &str {
        match &result.content[0] {
            ContentBlock::Text(text) => &text.text,
            _ => panic!("expected text"),
        }
    }

    #[tokio::test]
    async fn disabled_plugin_skill_is_not_listed_or_loadable() {
        let _guard = env_lock::lock_env([("PLUGINS", None::<&str>)]);
        let project = TempDir::new().unwrap();
        write_plugin_skill(
            project.path(),
            "disabled-plugin",
            "disabled-plugin-skill",
            "Disabled plugin metadata",
            "disabled plugin full body",
        );
        let client = test_client(project.path(), "disabled-plugin", false);

        assert!(client
            .get_instructions("test", project.path())
            .await
            .is_none_or(|instructions| !instructions.contains("disabled-plugin-skill")));

        let ctx =
            ToolCallContext::new("test".to_string(), Some(project.path().to_path_buf()), None);
        let args = serde_json::from_value(serde_json::json!({
            "name": "disabled-plugin-skill"
        }))
        .unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(result.is_error.unwrap_or(false));
        assert!(!result_text(&result).contains("disabled plugin full body"));
    }

    #[tokio::test]
    async fn enabled_plugin_skill_is_listed_and_loadable() {
        let _guard = env_lock::lock_env([("PLUGINS", None::<&str>)]);
        let project = TempDir::new().unwrap();
        write_plugin_skill(
            project.path(),
            "enabled-plugin",
            "enabled-plugin-skill",
            "Enabled plugin metadata",
            "enabled plugin full body",
        );
        let custom_skill_dir = project
            .path()
            .join(".agents/plugins/enabled-plugin/custom-skills/custom-plugin-skill");
        fs::create_dir_all(&custom_skill_dir).unwrap();
        fs::write(
            custom_skill_dir.join("SKILL.md"),
            "---\nname: custom-plugin-skill\ndescription: Custom plugin metadata\n---\ncustom plugin full body",
        )
        .unwrap();
        write_open_plugin_manifest(project.path(), "enabled-plugin");
        let client = test_client(project.path(), "enabled-plugin", true);

        let instructions = client
            .get_instructions("test", project.path())
            .await
            .unwrap();
        assert!(instructions.contains("enabled-plugin-skill"));
        assert!(instructions.contains("Enabled plugin metadata"));
        assert!(instructions.contains("custom-plugin-skill"));
        assert!(instructions.contains("Custom plugin metadata"));

        let ctx =
            ToolCallContext::new("test".to_string(), Some(project.path().to_path_buf()), None);
        let args = serde_json::from_value(serde_json::json!({
            "name": "custom-plugin-skill"
        }))
        .unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        assert!(result_text(&result).contains("custom plugin full body"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinked_project_plugin_supporting_file_is_loadable() {
        let _guard = env_lock::lock_env([("PLUGINS", None::<&str>)]);
        let project = TempDir::new().unwrap();
        let external = TempDir::new().unwrap();
        write_plugin_skill(
            external.path(),
            "symlinked-plugin",
            "symlinked-skill",
            "Symlinked skill metadata",
            "symlinked skill body",
        );
        write_open_plugin_manifest(external.path(), "symlinked-plugin");
        let external_plugin = external.path().join(".agents/plugins/symlinked-plugin");
        let supporting_file = external_plugin.join("skills/symlinked-skill/guide.md");
        fs::write(&supporting_file, "Symlinked supporting guidance.").unwrap();

        let plugin_link = project.path().join(".agents/plugins/symlinked-plugin");
        fs::create_dir_all(plugin_link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&external_plugin, &plugin_link).unwrap();
        let client = test_client(project.path(), "symlinked-plugin", true);

        let ctx =
            ToolCallContext::new("test".to_string(), Some(project.path().to_path_buf()), None);
        let args = serde_json::from_value(serde_json::json!({
            "name": "symlinked-skill/guide.md"
        }))
        .unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        assert!(result_text(&result).contains("Symlinked supporting guidance."));
    }

    #[tokio::test]
    async fn test_load_filesystem_skill_without_builtin_skills() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join(".goose/skills/my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: A test skill\n---\nDo the thing.",
        )
        .unwrap();
        fs::create_dir(skill_dir.join("nested")).unwrap();
        fs::write(skill_dir.join("nested/guide.md"), "Nested guidance.").unwrap();

        let client = SkillsClient::default().with_builtin_skills(false);

        assert!(client
            .discover_skills("test", temp_dir.path(), None)
            .await
            .iter()
            .all(|skill| skill.source_type != SourceType::BuiltinSkill));

        let ctx = ToolCallContext::new(
            "test".to_string(),
            Some(temp_dir.path().to_path_buf()),
            None,
        );
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "my-skill"})).unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        let text = match &result.content[0] {
            rmcp::model::ContentBlock::Text(t) => &t.text,
            _ => panic!("expected text"),
        };
        assert!(text.contains("my-skill"));
        assert!(text.contains("Do the thing"));

        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "my-skill/nested/guide.md"}))
                .unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        let text = match &result.content[0] {
            rmcp::model::ContentBlock::Text(t) => &t.text,
            _ => panic!("expected text"),
        };
        assert!(text.contains("Nested guidance."));
    }

    #[tokio::test]
    async fn test_load_skill_not_found_returns_error() {
        let client = SkillsClient::default();

        let ctx = ToolCallContext::new("test".to_string(), None, None);
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "nonexistent"})).unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(result.is_error.unwrap_or(false));
    }
}
