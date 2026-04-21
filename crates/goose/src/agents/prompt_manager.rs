#[cfg(test)]
use chrono::DateTime;
use chrono::Utc;
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::agents::extension::ExtensionInfo;
use crate::hints::load_hints::build_gitignore;
use crate::hints::{get_context_filenames, load_hint_files, SubdirectoryHintTracker};
use crate::{
    config::{Config, GooseMode},
    prompt_template,
    utils::sanitize_unicode_tags,
};
use std::path::Path;

const MAX_EXTENSIONS: usize = 5;
const MAX_TOOLS: usize = 50;

pub struct PromptManager {
    system_prompt_override: Option<String>,
    system_prompt_extras: IndexMap<String, String>,
    current_date_timestamp: String,
    subdirectory_hint_tracker: SubdirectoryHintTracker,
}

impl Default for PromptManager {
    fn default() -> Self {
        PromptManager::new()
    }
}

#[derive(Serialize)]
struct SystemPromptContext {
    extensions: Vec<ExtensionInfo>,
    current_date_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    extension_tool_limits: Option<(usize, usize)>,
    goose_mode: GooseMode,
    is_autonomous: bool,
    enable_subagents: bool,
    max_extensions: usize,
    max_tools: usize,
    code_execution_mode: bool,
}

pub struct SystemPromptBuilder<'a, M> {
    manager: &'a M,

    extensions_info: Vec<ExtensionInfo>,
    frontend_instructions: Option<String>,
    extension_tool_count: Option<(usize, usize)>,
    subagents_enabled: bool,
    hints: Option<String>,
    code_execution_mode: bool,
    goose_mode: Option<GooseMode>,
}

impl<'a> SystemPromptBuilder<'a, PromptManager> {
    pub fn with_extension(mut self, extension: ExtensionInfo) -> Self {
        self.extensions_info.push(extension);
        self
    }

    pub fn with_extensions(mut self, extensions: impl Iterator<Item = ExtensionInfo>) -> Self {
        for extension in extensions {
            self.extensions_info.push(extension);
        }
        self
    }

    pub fn with_frontend_instructions(mut self, frontend_instructions: Option<String>) -> Self {
        self.frontend_instructions = frontend_instructions;
        self
    }

    pub fn with_extension_and_tool_counts(
        mut self,
        extension_count: usize,
        tool_count: usize,
    ) -> Self {
        self.extension_tool_count = Some((extension_count, tool_count));
        self
    }

    pub fn with_code_execution_mode(mut self, enabled: bool) -> Self {
        self.code_execution_mode = enabled;
        self
    }

    pub fn with_hints(mut self, working_dir: &Path) -> Self {
        let hints_filenames = get_context_filenames();
        let ignore_patterns = build_gitignore(working_dir);

        let hints = load_hint_files(working_dir, &hints_filenames, &ignore_patterns);

        if !hints.is_empty() {
            self.hints = Some(hints);
        }
        self
    }

    pub fn with_enable_subagents(mut self, subagents_enabled: bool) -> Self {
        self.subagents_enabled = subagents_enabled;
        self
    }

    pub fn with_goose_mode(mut self, mode: GooseMode) -> Self {
        self.goose_mode = Some(mode);
        self
    }

    pub fn build(self) -> String {
        let mut extensions_info = self.extensions_info;

        // Add frontend instructions to extensions_info to simplify json rendering
        if let Some(frontend_instructions) = self.frontend_instructions {
            extensions_info.push(ExtensionInfo::new(
                "frontend",
                &frontend_instructions,
                false,
            ));
        }
        // Stable tool ordering is important for multi session prompt caching.
        extensions_info.sort_by(|a, b| a.name.cmp(&b.name));

        let sanitized_extensions_info: Vec<ExtensionInfo> = extensions_info
            .into_iter()
            .map(|mut ext_info| {
                ext_info.instructions = sanitize_unicode_tags(&ext_info.instructions);
                ext_info
            })
            .collect();

        let goose_mode = self
            .goose_mode
            .unwrap_or_else(|| Config::global().get_goose_mode().unwrap_or_default());

        let extension_tool_limits = self
            .extension_tool_count
            .filter(|(extensions, tools)| *extensions > MAX_EXTENSIONS || *tools > MAX_TOOLS);

        let context = SystemPromptContext {
            extensions: sanitized_extensions_info,
            current_date_time: self.manager.current_date_timestamp.clone(),
            extension_tool_limits,
            goose_mode,
            is_autonomous: goose_mode == GooseMode::Auto,
            enable_subagents: self.subagents_enabled,
            max_extensions: MAX_EXTENSIONS,
            max_tools: MAX_TOOLS,
            code_execution_mode: self.code_execution_mode,
        };

        let base_prompt = if let Some(override_prompt) = &self.manager.system_prompt_override {
            let sanitized_override_prompt = sanitize_unicode_tags(override_prompt);
            prompt_template::render_string(&sanitized_override_prompt, &context)
        } else {
            prompt_template::render_template("system.md", &context)
        }
        .unwrap_or_else(|_| {
            "You are a general-purpose AI agent called goose, created by Block".to_string()
        });

        let mut system_prompt_extras = self.manager.system_prompt_extras.clone();

        // Add hints if provided
        if let Some(hints) = self.hints {
            system_prompt_extras.insert("hints".to_string(), hints);
        }

        if goose_mode == GooseMode::Chat {
            system_prompt_extras.insert(
                "chat_mode".to_string(),
                "Right now you are in the chat only mode, no access to any tool use and system."
                    .to_string(),
            );
        }

        if system_prompt_extras.is_empty() {
            base_prompt
        } else {
            let sanitized_system_prompt_extras: Vec<String> = system_prompt_extras
                .into_values()
                .map(|extra| sanitize_unicode_tags(&extra))
                .collect();

            format!(
                "{}\n\n# Additional Instructions:\n\n{}",
                base_prompt,
                sanitized_system_prompt_extras.join("\n\n")
            )
        }
    }
}

impl PromptManager {
    pub fn new() -> Self {
        PromptManager {
            system_prompt_override: None,
            system_prompt_extras: IndexMap::new(),
            // Use the fixed current date time so that prompt cache can be used.
            // Filtering to an hour to balance user time accuracy and multi session prompt cache hits.
            current_date_timestamp: Utc::now().format("%Y-%m-%d %H:00").to_string(),
            subdirectory_hint_tracker: SubdirectoryHintTracker::new(),
        }
    }

    #[cfg(test)]
    pub fn with_timestamp(dt: DateTime<Utc>) -> Self {
        PromptManager {
            system_prompt_override: None,
            system_prompt_extras: IndexMap::new(),
            current_date_timestamp: dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            subdirectory_hint_tracker: SubdirectoryHintTracker::new(),
        }
    }

    /// Add an additional instruction to the system prompt with a key
    /// Using the same key will replace the previous instruction
    pub fn add_system_prompt_extra(&mut self, key: String, instruction: String) {
        self.system_prompt_extras.insert(key, instruction);
    }

    pub fn record_tool_arguments(
        &mut self,
        arguments: &Option<serde_json::Map<String, serde_json::Value>>,
        working_dir: &Path,
    ) {
        self.subdirectory_hint_tracker
            .record_tool_arguments(arguments, working_dir);
    }

    pub fn load_subdirectory_hints(&mut self, working_dir: &Path) -> bool {
        let new_hints = self.subdirectory_hint_tracker.load_new_hints(working_dir);
        let has_new = !new_hints.is_empty();
        for (key, content) in new_hints {
            self.system_prompt_extras.insert(key, content);
        }
        has_new
    }

    /// Override the system prompt with custom text
    pub fn set_system_prompt_override(&mut self, template: String) {
        self.system_prompt_override = Some(template);
    }

    pub fn builder<'a>(&'a self) -> SystemPromptBuilder<'a, Self> {
        SystemPromptBuilder {
            manager: self,

            extensions_info: vec![],
            frontend_instructions: None,
            extension_tool_count: None,
            subagents_enabled: false,
            hints: None,
            code_execution_mode: false,
            goose_mode: None,
        }
    }

    pub async fn get_recipe_prompt(&self) -> String {
        let context: HashMap<&str, Value> = HashMap::new();
        prompt_template::render_template("recipe.md", &context)
            .unwrap_or_else(|_| "The recipe prompt is busted. Tell the user.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;

    #[test]
    fn test_build_system_prompt_sanitizes_override() {
        let mut manager = PromptManager::new();
        let malicious_override = "System prompt\u{E0041}\u{E0042}\u{E0043}with hidden text";
        manager.set_system_prompt_override(malicious_override.to_string());

        let result = manager.builder().build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("System prompt"));
        assert!(result.contains("with hidden text"));
    }

    #[test]
    fn test_build_system_prompt_sanitizes_extras() {
        let mut manager = PromptManager::new();
        let malicious_extra = "Extra instruction\u{E0041}\u{E0042}\u{E0043}hidden";
        manager.add_system_prompt_extra("test".to_string(), malicious_extra.to_string());

        let result = manager.builder().build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Extra instruction"));
        assert!(result.contains("hidden"));
    }

    #[test]
    fn test_build_system_prompt_sanitizes_multiple_extras() {
        let mut manager = PromptManager::new();
        manager
            .add_system_prompt_extra("test1".to_string(), "First\u{E0041}instruction".to_string());
        manager.add_system_prompt_extra(
            "test2".to_string(),
            "Second\u{E0042}instruction".to_string(),
        );
        manager
            .add_system_prompt_extra("test3".to_string(), "Third\u{E0043}instruction".to_string());

        let result = manager.builder().build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Firstinstruction"));
        assert!(result.contains("Secondinstruction"));
        assert!(result.contains("Thirdinstruction"));
    }

    #[test]
    fn test_build_system_prompt_preserves_legitimate_unicode_in_extras() {
        let mut manager = PromptManager::new();
        let legitimate_unicode = "Instruction with 世界 and 🌍 emojis";
        manager.add_system_prompt_extra("test".to_string(), legitimate_unicode.to_string());

        let result = manager.builder().build();

        assert!(result.contains("世界"));
        assert!(result.contains("🌍"));
        assert!(result.contains("Instruction with"));
        assert!(result.contains("emojis"));
    }

    #[test]
    fn test_build_system_prompt_sanitizes_extension_instructions() {
        let manager = PromptManager::new();
        let malicious_extension_info = ExtensionInfo::new(
            "test_extension",
            "Extension help\u{E0041}\u{E0042}\u{E0043}hidden instructions",
            false,
        );

        let result = manager
            .builder()
            .with_extension(malicious_extension_info)
            .build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Extension help"));
        assert!(result.contains("hidden instructions"));
    }

    #[test]
    fn test_basic() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let system_prompt = manager.builder().build();

        assert_snapshot!(system_prompt)
    }

    #[test]
    fn test_one_extension() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let system_prompt = manager
            .builder()
            .with_extension(ExtensionInfo::new(
                "test",
                "how to use this extension",
                true,
            ))
            .build();

        assert_snapshot!(system_prompt)
    }

    #[test]
    fn test_typical_setup() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let system_prompt = manager
            .builder()
            .with_extension(ExtensionInfo::new(
                "extension_A",
                "<instructions on how to use extension A>",
                true,
            ))
            .with_extension(ExtensionInfo::new(
                "extension_B",
                "<instructions on how to use extension B (no resources)>",
                false,
            ))
            .with_extension_and_tool_counts(MAX_EXTENSIONS + 1, MAX_TOOLS + 1)
            .build();

        assert_snapshot!(system_prompt)
    }

    #[tokio::test]
    async fn test_all_platform_extensions() {
        use crate::agents::platform_extensions::{PlatformExtensionContext, PLATFORM_EXTENSIONS};
        use crate::config::GooseMode;
        use crate::session::SessionManager;
        use std::sync::Arc;

        let tmp_dir = tempfile::tempdir().unwrap();
        let temp_root = tmp_dir.path().display().to_string();
        let _guard = env_lock::lock_env([
            ("HOME", Some(temp_root.as_str())),
            ("GOOSE_PATH_ROOT", Some(temp_root.as_str())),
        ]);
        let session_manager = Arc::new(SessionManager::new(tmp_dir.path().to_path_buf()));
        let session = session_manager
            .create_session(
                tmp_dir.path().to_path_buf(),
                "test session".to_owned(),
                crate::session::SessionType::Hidden,
                GooseMode::default(),
            )
            .await
            .unwrap();
        let context = PlatformExtensionContext {
            extension_manager: None,
            session_manager,
            session: Some(Arc::new(session)),
        };

        let mut extensions: Vec<ExtensionInfo> = PLATFORM_EXTENSIONS
            .values()
            .map(|def| {
                let client = (def.client_factory)(context.clone());
                let info = client.get_info();
                let instructions = info
                    .and_then(|i| i.instructions.clone())
                    .unwrap_or_default();
                let has_resources = info
                    .and_then(|i| i.capabilities.resources.as_ref())
                    .is_some();
                ExtensionInfo::new(def.name, &instructions, has_resources)
            })
            .collect();

        extensions.sort_by(|a, b| a.name.cmp(&b.name));

        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());
        let system_prompt = manager
            .builder()
            .with_extensions(extensions.into_iter())
            .build();

        assert_snapshot!(system_prompt);
    }
}
