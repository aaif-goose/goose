#[cfg(test)]
use chrono::DateTime;
use chrono::Utc;
use indexmap::IndexMap;
use serde::Serialize;

use crate::agents::{extension::ExtensionInfo, moim};
use crate::hints::load_hints::build_gitignore;
use crate::hints::{get_context_filenames, load_hint_files};
use crate::session::Session;
use crate::{
    config::{Config, GooseMode},
    prompt_template,
    utils::sanitize_unicode_tags,
};
use std::path::Path;

pub struct PromptManager {
    current_date_timestamp: String,
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
    goose_mode: GooseMode,
    is_autonomous: bool,
    enable_subagents: bool,
    code_execution_mode: bool,
    include_extensions: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    moim_system_prompt_block: Option<String>,
}

pub struct SystemPromptBuilder<'a, M> {
    manager: &'a M,

    system_prompt_override: Option<String>,
    extensions_info: Vec<ExtensionInfo>,
    prompt_extras: IndexMap<String, String>,
    subagents_enabled: bool,
    hints: Option<String>,
    code_execution_mode: bool,
    include_extensions: bool,
    goose_mode: Option<GooseMode>,
}

impl<'a> SystemPromptBuilder<'a, PromptManager> {
    pub fn with_session(mut self, session: &Session) -> Self {
        self.system_prompt_override = session.system_prompt_override.clone();
        self.prompt_extras
            .extend(session.system_prompt_extras.clone());
        self
    }

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

    pub fn with_prompt_extras(
        mut self,
        extras: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        self.prompt_extras.extend(extras);
        self
    }

    pub fn with_code_execution_mode(mut self, enabled: bool) -> Self {
        self.code_execution_mode = enabled;
        self
    }

    pub fn without_extensions(mut self) -> Self {
        self.include_extensions = false;
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

        let context = SystemPromptContext {
            extensions: sanitized_extensions_info,
            current_date_time: self.manager.current_date_timestamp.clone(),
            goose_mode,
            is_autonomous: goose_mode == GooseMode::Auto,
            enable_subagents: self.subagents_enabled,
            code_execution_mode: self.code_execution_mode,
            include_extensions: self.include_extensions,
            moim_system_prompt_block: moim::system_prompt_block(),
        };

        let base_prompt = if let Some(override_prompt) = &self.system_prompt_override {
            let sanitized_override_prompt = sanitize_unicode_tags(override_prompt);
            prompt_template::render_string(&sanitized_override_prompt, &context)
        } else {
            prompt_template::render_template("system.md", &context)
        }
        .unwrap_or_else(|_| {
            "You are a general-purpose AI agent called goose, created by Block".to_string()
        });

        let mut system_prompt_extras = self.prompt_extras;

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
            // Use the fixed current date time so that prompt cache can be used.
            // Filtering to an hour to balance user time accuracy and multi session prompt cache hits.
            current_date_timestamp: Utc::now().format("%Y-%m-%d %H:00 %:z").to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_timestamp(dt: DateTime<Utc>) -> Self {
        PromptManager {
            current_date_timestamp: dt.format("%Y-%m-%d %H:%M:%S %:z").to_string(),
        }
    }

    pub fn build_system_prompt(
        &self,
        session: &Session,
        prompt_parts: Vec<(String, String)>,
        goose_mode: GooseMode,
    ) -> String {
        self.builder()
            .with_session(session)
            .with_prompt_extras(prompt_parts)
            .with_hints(&session.working_dir)
            .with_goose_mode(goose_mode)
            .without_extensions()
            .build()
    }

    pub fn builder<'a>(&'a self) -> SystemPromptBuilder<'a, Self> {
        SystemPromptBuilder {
            manager: self,

            system_prompt_override: None,
            extensions_info: vec![],
            prompt_extras: IndexMap::new(),
            subagents_enabled: false,
            hints: None,
            code_execution_mode: false,
            include_extensions: true,
            goose_mode: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;

    fn session_with_override(system_prompt_override: &str) -> Session {
        Session {
            system_prompt_override: Some(system_prompt_override.to_string()),
            ..Session::default()
        }
    }

    #[test]
    fn test_build_system_prompt_sanitizes_override() {
        let manager = PromptManager::new();
        let session =
            session_with_override("System prompt\u{E0041}\u{E0042}\u{E0043}with hidden text");

        let result = manager.builder().with_session(&session).build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("System prompt"));
        assert!(result.contains("with hidden text"));
    }

    #[test]
    fn test_current_date_time_includes_timezone() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());
        let session = session_with_override("It is currently {{current_date_time}}");

        let result = manager.builder().with_session(&session).build();

        assert_eq!(result, "It is currently 1970-01-01 00:00:00 +00:00");
    }

    #[test]
    fn test_build_system_prompt_sanitizes_extras() {
        let malicious_extra = "Extra instruction\u{E0041}\u{E0042}\u{E0043}hidden";

        let result = PromptManager::new()
            .builder()
            .with_prompt_extras([("test".to_string(), malicious_extra.to_string())])
            .build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Extra instruction"));
        assert!(result.contains("hidden"));
    }

    #[test]
    fn prompt_contributions_are_not_retained() {
        let manager = PromptManager::new();

        let with_contribution = manager
            .builder()
            .with_prompt_extras([("operation".to_string(), "temporary instruction".to_string())])
            .build();
        let without_contribution = manager.builder().build();

        assert!(with_contribution.contains("temporary instruction"));
        assert!(!without_contribution.contains("temporary instruction"));
    }

    #[test]
    fn composed_prompt_uses_contributions_instead_of_the_extension_catalog() {
        let manager = PromptManager::new();
        let working_dir = tempfile::tempdir().unwrap();

        let session = Session {
            working_dir: working_dir.path().to_path_buf(),
            ..Session::default()
        };

        let prompt = manager.build_system_prompt(
            &session,
            vec![(
                "extensions".to_string(),
                "# Extensions\n\n## developer".to_string(),
            )],
            GooseMode::Auto,
        );

        assert!(prompt.contains("## developer"));
        assert!(!prompt.contains("No extensions are defined"));
    }

    #[test]
    fn project_git_metadata_does_not_reach_system_prompt() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join(".git")).unwrap();
        std::fs::create_dir(project.path().join("docs")).unwrap();
        std::fs::write(
            project.path().join(".git/config"),
            "url = https://oauth2:PROMPT_SECRET@example.invalid/repo.git",
        )
        .unwrap();
        std::fs::write(
            project.path().join("docs/config.md"),
            "legitimate project configuration",
        )
        .unwrap();
        std::fs::write(
            project.path().join(crate::hints::AGENTS_MD_FILENAME),
            "project instructions\n@.git/config\n@docs/config.md",
        )
        .unwrap();
        let ignore_patterns = build_gitignore(project.path());
        let hints = load_hint_files(
            project.path(),
            &[crate::hints::AGENTS_MD_FILENAME.to_string()],
            &ignore_patterns,
        );

        let prompt = PromptManager::new()
            .builder()
            .with_prompt_extras([("hints".to_string(), hints)])
            .build();

        assert!(prompt.contains("project instructions"));
        assert!(prompt.contains("legitimate project configuration"));
        assert!(!prompt.contains("PROMPT_SECRET"));
    }

    #[test]
    fn test_build_system_prompt_sanitizes_multiple_extras() {
        let result = PromptManager::new()
            .builder()
            .with_prompt_extras([
                ("test1".to_string(), "First\u{E0041}instruction".to_string()),
                (
                    "test2".to_string(),
                    "Second\u{E0042}instruction".to_string(),
                ),
                ("test3".to_string(), "Third\u{E0043}instruction".to_string()),
            ])
            .build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Firstinstruction"));
        assert!(result.contains("Secondinstruction"));
        assert!(result.contains("Thirdinstruction"));
    }

    #[test]
    fn test_build_system_prompt_preserves_legitimate_unicode_in_extras() {
        let legitimate_unicode = "Instruction with 世界 and 🌍 emojis";

        let result = PromptManager::new()
            .builder()
            .with_prompt_extras([("test".to_string(), legitimate_unicode.to_string())])
            .build();

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
            .build();

        assert_snapshot!(system_prompt)
    }

    // The snapshot lists every platform extension, so it needs every feature that adds one.
    #[cfg(all(
        feature = "chat-recall",
        feature = "code-mode",
        feature = "platform-apps",
        feature = "scheduler",
        feature = "tree-sitter"
    ))]
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
        let scheduler = crate::scheduler::Scheduler::new(
            tmp_dir.path().join("schedules"),
            session_manager.clone(),
        )
        .await
        .unwrap();
        let context = PlatformExtensionContext {
            extension_manager: None,
            providers: Default::default(),
            session_manager,
            scheduler: Some(scheduler),
            use_login_shell_path: false,
        };

        let mut extensions = Vec::new();
        for def in PLATFORM_EXTENSIONS.values() {
            let Some(client) = (def.client_factory)(context.clone()) else {
                continue;
            };
            let instructions = client
                .get_instructions(&session.id, &session.working_dir)
                .await
                .unwrap_or_default();
            let has_resources = client
                .get_info()
                .and_then(|i| i.capabilities.resources.as_ref())
                .is_some();
            extensions.push(ExtensionInfo::new(def.name, &instructions, has_resources));
        }

        extensions.sort_by(|a, b| a.name.cmp(&b.name));

        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());
        let system_prompt = manager
            .builder()
            .with_extensions(extensions.into_iter())
            .build();

        assert_snapshot!(system_prompt);
    }
}
