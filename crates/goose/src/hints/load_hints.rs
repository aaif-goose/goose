use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::config::paths::Paths;
use crate::hints::import_files::{read_referenced_files_with_limit, MAX_HINT_OUTPUT_BYTES};

pub const GOOSE_HINTS_FILENAME: &str = ".goosehints";
pub const AGENTS_MD_FILENAME: &str = "AGENTS.md";
const GLOBAL_HINTS_HEADER: &str = "\n### Global Hints\nThese are my global goose hints.\n";
const PROJECT_HINTS_HEADER: &str =
    "### Project Hints\nThese are hints for working on the project in this directory.\n";

pub fn get_context_filenames() -> Vec<String> {
    use crate::config::Config;

    Config::global()
        .get_param::<Vec<String>>("CONTEXT_FILE_NAMES")
        .unwrap_or_else(|_| {
            vec![
                GOOSE_HINTS_FILENAME.to_string(),
                AGENTS_MD_FILENAME.to_string(),
            ]
        })
}

pub struct SubdirectoryHintTracker {
    loaded_dirs: HashSet<PathBuf>,
    pending_dirs: Vec<PathBuf>,
    hints_filenames: Vec<String>,
    remaining_output_bytes: usize,
}

impl Default for SubdirectoryHintTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SubdirectoryHintTracker {
    pub fn new() -> Self {
        Self {
            loaded_dirs: HashSet::new(),
            pending_dirs: Vec::new(),
            hints_filenames: get_context_filenames(),
            remaining_output_bytes: MAX_HINT_OUTPUT_BYTES,
        }
    }

    pub fn record_tool_arguments(
        &mut self,
        arguments: &Option<serde_json::Map<String, serde_json::Value>>,
        working_dir: &Path,
    ) {
        let args = match arguments.as_ref() {
            Some(a) => a,
            None => return,
        };

        if let Some(path_str) = args.get("path").and_then(|v| v.as_str()) {
            if let Some(dir) = resolve_to_parent_dir(path_str, working_dir) {
                self.pending_dirs.push(dir);
            }
        }

        if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
            for token in shell_words::split(cmd).unwrap_or_default() {
                if token.starts_with('-') {
                    continue;
                }
                if token.contains(std::path::MAIN_SEPARATOR) || token.contains('.') {
                    if let Some(dir) = resolve_to_parent_dir(&token, working_dir) {
                        self.pending_dirs.push(dir);
                    }
                }
            }
        }
    }

    pub fn load_new_hints(&mut self, working_dir: &Path) -> Vec<(String, String)> {
        let pending = std::mem::take(&mut self.pending_dirs);
        if pending.is_empty() {
            return Vec::new();
        }

        let Ok(working_dir) = working_dir.canonicalize() else {
            return Vec::new();
        };

        let mut results = Vec::new();
        for dir in pending {
            let Ok(dir) = dir.canonicalize() else {
                continue;
            };
            if !dir.starts_with(&working_dir) || dir == working_dir {
                continue;
            }
            if self.loaded_dirs.contains(&dir) {
                continue;
            }
            if let Some(content) = load_hints_from_directory(
                &dir,
                &working_dir,
                &self.hints_filenames,
                self.remaining_output_bytes,
            ) {
                self.remaining_output_bytes -= content.len();
                let key = format!("subdir_hints:{}", dir.display());
                results.push((key, content));
            }
            self.loaded_dirs.insert(dir);
        }
        results
    }
}

fn resolve_to_parent_dir(token: &str, working_dir: &Path) -> Option<PathBuf> {
    let path = Path::new(token);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_dir.join(path)
    };
    resolved.parent().map(|d| d.to_path_buf())
}

fn load_hints_from_directory(
    directory: &Path,
    working_dir: &Path,
    hints_filenames: &[String],
    output_limit: usize,
) -> Option<String> {
    if !directory.is_dir() || !directory.is_absolute() {
        return None;
    }

    if !directory.starts_with(working_dir) || directory == working_dir {
        return None;
    }

    let git_root = find_git_root(working_dir);
    let import_boundary = git_root.unwrap_or(working_dir);
    let gitignore = Gitignore::empty();

    let mut directories: Vec<PathBuf> = directory
        .ancestors()
        .take_while(|d| d.starts_with(working_dir) && *d != working_dir)
        .map(|d| d.to_path_buf())
        .collect();
    directories.reverse();

    let header = format!("### Subdirectory Hints ({})\n", directory.display());
    let mut output = String::new();
    let mut has_hints = false;
    for dir in &directories {
        for hints_filename in hints_filenames {
            let hints_path = dir.join(hints_filename);
            if hints_path.is_file() {
                let framing = if has_hints { "\n" } else { &header };
                if append_hint_file(
                    &mut output,
                    framing,
                    &hints_path,
                    import_boundary,
                    &gitignore,
                    output_limit,
                ) {
                    has_hints = true;
                }
            }
        }
    }

    has_hints.then_some(output)
}

fn append_hint_file(
    output: &mut String,
    framing: &str,
    hints_path: &Path,
    import_boundary: &Path,
    ignore_patterns: &Gitignore,
    output_limit: usize,
) -> bool {
    let Some(used_with_framing) = output.len().checked_add(framing.len()) else {
        return false;
    };
    let Some(available) = output_limit.checked_sub(used_with_framing) else {
        return false;
    };

    let mut visited = HashSet::new();
    let expanded = read_referenced_files_with_limit(
        hints_path,
        import_boundary,
        &mut visited,
        0,
        ignore_patterns,
        available,
    );
    if expanded.is_empty() {
        return false;
    }

    output.push_str(framing);
    output.push_str(&expanded);
    true
}

fn find_git_root(start_dir: &Path) -> Option<&Path> {
    let mut check_dir = start_dir;

    loop {
        if check_dir.join(".git").exists() {
            return Some(check_dir);
        }
        if let Some(parent) = check_dir.parent() {
            check_dir = parent;
        } else {
            break;
        }
    }

    None
}

fn get_local_directories(git_root: Option<&Path>, cwd: &Path) -> Vec<PathBuf> {
    match git_root {
        Some(git_root) => {
            let mut directories = Vec::new();
            let mut current_dir = cwd;

            loop {
                directories.push(current_dir.to_path_buf());
                if current_dir == git_root {
                    break;
                }
                if let Some(parent) = current_dir.parent() {
                    current_dir = parent;
                } else {
                    break;
                }
            }
            directories.reverse();
            directories
        }
        None => vec![cwd.to_path_buf()],
    }
}

/// Build a `Gitignore` that includes `.gitignore` files from the git root
/// down to `cwd`, matching git's hierarchical ignore semantics. When there
/// is no git root, only `cwd/.gitignore` is loaded.
pub fn build_gitignore(cwd: &Path) -> Gitignore {
    let git_root = find_git_root(cwd);
    let directories = get_local_directories(git_root, cwd);

    let mut builder = GitignoreBuilder::new(cwd);
    for dir in &directories {
        let gitignore_path = dir.join(".gitignore");
        if gitignore_path.is_file() {
            builder.add(&gitignore_path);
        }
    }
    builder.build().unwrap_or_else(|_| {
        GitignoreBuilder::new(cwd)
            .build()
            .expect("Failed to build default gitignore")
    })
}

pub fn load_hint_files(
    cwd: &Path,
    hints_filenames: &[String],
    ignore_patterns: &Gitignore,
) -> String {
    load_hint_files_with_limit(cwd, hints_filenames, ignore_patterns, MAX_HINT_OUTPUT_BYTES)
}

pub(crate) fn load_hint_files_with_limit(
    cwd: &Path,
    hints_filenames: &[String],
    ignore_patterns: &Gitignore,
    output_limit: usize,
) -> String {
    let mut global_hints_paths: Vec<PathBuf> = hints_filenames
        .iter()
        .map(|name| Paths::in_config_dir(name))
        .collect();
    if hints_filenames
        .iter()
        .any(|name| name == AGENTS_MD_FILENAME)
    {
        global_hints_paths.push(Paths::in_agents_home_dir(AGENTS_MD_FILENAME));
    }

    let mut hints = String::new();
    let mut has_global_hints = false;
    for global_hints_path in &global_hints_paths {
        if global_hints_path.is_file() {
            let hints_dir = global_hints_path.parent().unwrap();
            let global_ignore_patterns = GitignoreBuilder::new(hints_dir)
                .build()
                .unwrap_or_else(|_| Gitignore::empty());
            let framing = if has_global_hints {
                "\n"
            } else {
                GLOBAL_HINTS_HEADER
            };
            if append_hint_file(
                &mut hints,
                framing,
                global_hints_path,
                hints_dir,
                &global_ignore_patterns,
                output_limit,
            ) {
                has_global_hints = true;
            }
        }
    }
    let git_root = find_git_root(cwd);
    let local_directories = get_local_directories(git_root, cwd);

    let import_boundary = git_root.unwrap_or(cwd);

    let mut has_local_hints = false;
    for directory in &local_directories {
        for hints_filename in hints_filenames {
            let hints_path = directory.join(hints_filename);
            if hints_path.is_file() {
                let framing = if has_local_hints {
                    "\n"
                } else if has_global_hints {
                    "\n\n### Project Hints\nThese are hints for working on the project in this directory.\n"
                } else {
                    PROJECT_HINTS_HEADER
                };
                if append_hint_file(
                    &mut hints,
                    framing,
                    &hints_path,
                    import_boundary,
                    ignore_patterns,
                    output_limit,
                ) {
                    has_local_hints = true;
                }
            }
        }
    }

    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use ignore::gitignore::GitignoreBuilder;
    use std::fs;
    use tempfile::TempDir;

    fn create_dummy_gitignore() -> Gitignore {
        let temp_dir = tempfile::tempdir().expect("failed to create tempdir");
        let builder = GitignoreBuilder::new(temp_dir.path());
        builder.build().expect("failed to build gitignore")
    }

    #[test]
    fn test_goosehints_when_present() {
        let dir = TempDir::new().unwrap();

        fs::write(dir.path().join(GOOSE_HINTS_FILENAME), "Test hint content").unwrap();
        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(dir.path(), &[GOOSE_HINTS_FILENAME.to_string()], &gitignore);

        assert!(hints.contains("Test hint content"));
    }

    #[test]
    fn oversized_top_level_hint_is_rejected_without_partial_instructions() {
        let dir = TempDir::new().unwrap();
        let filename = "LOUPE_894_OVERSIZED.md";
        fs::write(
            dir.path().join(filename),
            "x".repeat(MAX_HINT_OUTPUT_BYTES + 1),
        )
        .unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(dir.path(), &[filename.to_string()], &gitignore);

        assert!(hints.is_empty());
    }

    #[test]
    fn top_level_hint_and_framing_fit_exact_output_limit() {
        let dir = TempDir::new().unwrap();
        let filename = "LOUPE_894_EXACT.md";
        let content = "x".repeat(MAX_HINT_OUTPUT_BYTES - PROJECT_HINTS_HEADER.len());
        fs::write(dir.path().join(filename), &content).unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(dir.path(), &[filename.to_string()], &gitignore);

        assert_eq!(hints.len(), MAX_HINT_OUTPUT_BYTES);
        assert!(hints.starts_with(PROJECT_HINTS_HEADER));
        assert!(hints.ends_with(&content));
    }

    #[test]
    fn reference_expansion_cannot_exceed_aggregate_output_limit() {
        let dir = TempDir::new().unwrap();
        let filename = "LOUPE_894_REFERENCE.md";
        let included = "expanded policy".repeat(16);
        fs::write(dir.path().join("policy.md"), &included).unwrap();
        fs::write(
            dir.path().join(filename),
            "keep this\n@policy.md\nkeep that",
        )
        .unwrap();

        let gitignore = create_dummy_gitignore();
        let mut output = String::new();
        let appended = append_hint_file(
            &mut output,
            "header\n",
            &dir.path().join(filename),
            dir.path(),
            &gitignore,
            64,
        );

        assert!(appended);
        assert!(output.len() <= 64);
        assert!(output.contains("@policy.md"));
        assert!(!output.contains("expanded policy"));
        assert!(output.ends_with("keep that"));
    }

    #[test]
    fn multiple_top_level_hints_share_one_output_limit() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("first.md"), "A1".repeat(300 * 1024)).unwrap();
        fs::write(dir.path().join("second.md"), "B2".repeat(300 * 1024)).unwrap();
        fs::write(dir.path().join("third.md"), "third hint").unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(
            dir.path(),
            &[
                "first.md".to_string(),
                "second.md".to_string(),
                "third.md".to_string(),
            ],
            &gitignore,
        );

        assert!(hints.len() <= MAX_HINT_OUTPUT_BYTES);
        assert_eq!(hints.matches("A1").count(), 300 * 1024);
        assert!(!hints.contains("B2"));
        assert!(hints.ends_with("third hint"));
    }

    #[test]
    #[serial_test::serial]
    fn test_global_agents_md_in_agents_home() {
        let root = TempDir::new().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", root.path());

        let agents_home = root.path().join(".agents");
        fs::create_dir_all(&agents_home).unwrap();
        fs::write(
            agents_home.join(AGENTS_MD_FILENAME),
            "Global agents home instructions",
        )
        .unwrap();

        let project = TempDir::new().unwrap();
        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(
            project.path(),
            &[
                GOOSE_HINTS_FILENAME.to_string(),
                AGENTS_MD_FILENAME.to_string(),
            ],
            &gitignore,
        );

        std::env::remove_var("GOOSE_PATH_ROOT");

        assert!(hints.contains("Global Hints"));
        assert!(hints.contains("Global agents home instructions"));
    }

    #[test]
    #[serial_test::serial]
    fn global_and_local_hints_share_one_output_limit() {
        let root = TempDir::new().unwrap();
        let filename = "LOUPE_894_GLOBAL_LOCAL.md";
        std::env::set_var("GOOSE_PATH_ROOT", root.path());
        fs::create_dir(root.path().join("config")).unwrap();
        fs::write(
            root.path().join("config").join(filename),
            "G1".repeat(300 * 1024),
        )
        .unwrap();

        let project = TempDir::new().unwrap();
        fs::write(project.path().join(filename), "L2".repeat(300 * 1024)).unwrap();
        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(project.path(), &[filename.to_string()], &gitignore);

        std::env::remove_var("GOOSE_PATH_ROOT");

        assert!(hints.len() <= MAX_HINT_OUTPUT_BYTES);
        assert!(hints.contains("Global Hints"));
        assert!(hints.contains("G1"));
        assert!(!hints.contains("Project Hints"));
        assert!(!hints.contains("L2"));
    }

    #[test]
    #[serial_test::serial]
    fn test_global_agents_md_imports_not_filtered_by_project_gitignore() {
        let root = TempDir::new().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", root.path());

        let agents_home = root.path().join(".agents");
        fs::create_dir_all(&agents_home).unwrap();
        fs::write(agents_home.join("policy.md"), "Imported policy content").unwrap();
        fs::write(
            agents_home.join(AGENTS_MD_FILENAME),
            "Global header\n@policy.md\n",
        )
        .unwrap();

        let project = TempDir::new().unwrap();
        let mut builder = GitignoreBuilder::new(project.path());
        builder.add_line(None, "*.md").unwrap();
        let gitignore = builder.build().unwrap();

        let hints = load_hint_files(
            project.path(),
            &[
                GOOSE_HINTS_FILENAME.to_string(),
                AGENTS_MD_FILENAME.to_string(),
            ],
            &gitignore,
        );

        std::env::remove_var("GOOSE_PATH_ROOT");

        assert!(hints.contains("Imported policy content"));
    }

    #[test]
    #[serial_test::serial]
    fn test_global_agents_md_skipped_when_not_in_context_file_names() {
        let root = TempDir::new().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", root.path());

        let agents_home = root.path().join(".agents");
        fs::create_dir_all(&agents_home).unwrap();
        fs::write(
            agents_home.join(AGENTS_MD_FILENAME),
            "Global agents home instructions",
        )
        .unwrap();

        let project = TempDir::new().unwrap();
        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(
            project.path(),
            &[GOOSE_HINTS_FILENAME.to_string()],
            &gitignore,
        );

        std::env::remove_var("GOOSE_PATH_ROOT");

        assert!(!hints.contains("Global agents home instructions"));
    }

    #[test]
    fn test_goosehints_when_missing() {
        let dir = TempDir::new().unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(dir.path(), &[GOOSE_HINTS_FILENAME.to_string()], &gitignore);

        assert!(!hints.contains("Project Hints"));
    }

    #[test]
    fn test_goosehints_multiple_filenames() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("CLAUDE.md"),
            "Custom hints file content from CLAUDE.md",
        )
        .unwrap();
        fs::write(
            dir.path().join(GOOSE_HINTS_FILENAME),
            "Custom hints file content from .goosehints",
        )
        .unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(
            dir.path(),
            &["CLAUDE.md".to_string(), GOOSE_HINTS_FILENAME.to_string()],
            &gitignore,
        );

        assert!(hints.contains("Custom hints file content from CLAUDE.md"));
        assert!(hints.contains("Custom hints file content from .goosehints"));
    }

    #[test]
    fn test_goosehints_configurable_filename() {
        let dir = TempDir::new().unwrap();

        fs::write(dir.path().join("CLAUDE.md"), "Custom hints file content").unwrap();
        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(dir.path(), &["CLAUDE.md".to_string()], &gitignore);

        assert!(hints.contains("Custom hints file content"));
        assert!(!hints.contains(".goosehints")); // Make sure it's not loading the default
    }

    #[test]
    fn test_nested_goosehints_with_git_root() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        fs::create_dir(project_root.join(".git")).unwrap();
        fs::write(
            project_root.join(GOOSE_HINTS_FILENAME),
            "Root hints content",
        )
        .unwrap();

        let subdir = project_root.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join(GOOSE_HINTS_FILENAME), "Subdir hints content").unwrap();
        let current_dir = subdir.join("current_dir");
        fs::create_dir(&current_dir).unwrap();
        fs::write(
            current_dir.join(GOOSE_HINTS_FILENAME),
            "current_dir hints content",
        )
        .unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(
            &current_dir,
            &[GOOSE_HINTS_FILENAME.to_string()],
            &gitignore,
        );

        assert!(
            hints.contains("Root hints content\nSubdir hints content\ncurrent_dir hints content")
        );
    }

    #[test]
    fn test_nested_goosehints_without_git_root() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path();

        fs::write(base_dir.join(GOOSE_HINTS_FILENAME), "Base hints content").unwrap();

        let subdir = base_dir.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join(GOOSE_HINTS_FILENAME), "Subdir hints content").unwrap();

        let current_dir = subdir.join("current_dir");
        fs::create_dir(&current_dir).unwrap();
        fs::write(
            current_dir.join(GOOSE_HINTS_FILENAME),
            "Current dir hints content",
        )
        .unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(
            &current_dir,
            &[GOOSE_HINTS_FILENAME.to_string()],
            &gitignore,
        );

        // Without .git, should only find hints in current directory
        assert!(hints.contains("Current dir hints content"));
        assert!(!hints.contains("Base hints content"));
        assert!(!hints.contains("Subdir hints content"));
    }

    #[test]
    fn test_nested_goosehints_mixed_filenames() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        fs::create_dir(project_root.join(".git")).unwrap();
        fs::write(project_root.join("CLAUDE.md"), "Root CLAUDE.md content").unwrap();

        let subdir = project_root.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(
            subdir.join(GOOSE_HINTS_FILENAME),
            "Subdir .goosehints content",
        )
        .unwrap();

        let current_dir = subdir.join("current_dir");
        fs::create_dir(&current_dir).unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(
            &current_dir,
            &["CLAUDE.md".to_string(), GOOSE_HINTS_FILENAME.to_string()],
            &gitignore,
        );

        assert!(hints.contains("Root CLAUDE.md content"));
        assert!(hints.contains("Subdir .goosehints content"));
    }

    #[test]
    fn test_hints_with_basic_imports() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        fs::create_dir(project_root.join(".git")).unwrap();

        fs::write(project_root.join("README.md"), "# Project README").unwrap();
        fs::write(project_root.join("config.md"), "Configuration details").unwrap();

        let hints_content = r#"Project hints content
@README.md
@config.md
Additional instructions here."#;
        fs::write(project_root.join(GOOSE_HINTS_FILENAME), hints_content).unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(
            project_root,
            &[GOOSE_HINTS_FILENAME.to_string()],
            &gitignore,
        );

        assert!(hints.contains("Project hints content"));
        assert!(hints.contains("Additional instructions here"));

        assert!(hints.contains("--- Content from README.md ---"));
        assert!(hints.contains("# Project README"));
        assert!(hints.contains("--- End of README.md ---"));

        assert!(hints.contains("--- Content from config.md ---"));
        assert!(hints.contains("Configuration details"));
        assert!(hints.contains("--- End of config.md ---"));
    }

    #[test]
    fn test_hints_with_git_import_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        fs::create_dir(project_root.join(".git")).unwrap();

        fs::write(project_root.join("root_file.md"), "Root file content").unwrap();
        fs::write(
            project_root.join("shared_docs.md"),
            "Shared documentation content",
        )
        .unwrap();

        let docs_dir = project_root.join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("api.md"), "API documentation content").unwrap();

        let utils_dir = project_root.join("src").join("utils");
        fs::create_dir_all(&utils_dir).unwrap();
        fs::write(
            utils_dir.join("helpers.md"),
            "Helper utilities content @../../shared_docs.md",
        )
        .unwrap();

        let components_dir = project_root.join("src").join("components");
        fs::create_dir_all(&components_dir).unwrap();
        fs::write(components_dir.join("local_file.md"), "Local file content").unwrap();

        let outside_dir = temp_dir.path().parent().unwrap();
        fs::write(outside_dir.join("forbidden.md"), "Forbidden content").unwrap();

        let root_hints_content = r#"Project root hints
@docs/api.md
Root level instructions"#;
        fs::write(project_root.join(GOOSE_HINTS_FILENAME), root_hints_content).unwrap();

        let nested_hints_content = r#"Nested directory hints
@local_file.md
@../utils/helpers.md
@../../docs/api.md
@../../root_file.md
@../../../forbidden.md
End of nested hints"#;
        fs::write(
            components_dir.join(GOOSE_HINTS_FILENAME),
            nested_hints_content,
        )
        .unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(
            &components_dir,
            &[GOOSE_HINTS_FILENAME.to_string()],
            &gitignore,
        );
        println!("======{}", hints);
        assert!(hints.contains("Project root hints"));
        assert!(hints.contains("Root level instructions"));

        assert!(hints.contains("API documentation content"));
        assert!(hints.contains("--- Content from docs/api.md ---"));

        assert!(hints.contains("Nested directory hints"));
        assert!(hints.contains("End of nested hints"));

        assert!(hints.contains("Local file content"));
        assert!(hints.contains("--- Content from local_file.md ---"));

        assert!(hints.contains("Helper utilities content"));
        assert!(hints.contains("--- Content from ../utils/helpers.md ---"));
        assert!(hints.contains("Shared documentation content"));
        assert!(hints.contains("--- Content from ../../shared_docs.md ---"));

        let api_content_count = hints.matches("API documentation content").count();
        assert_eq!(
            api_content_count, 2,
            "API content should appear twice - from root and nested hints"
        );

        assert!(hints.contains("Root file content"));
        assert!(hints.contains("--- Content from ../../root_file.md ---"));

        assert!(!hints.contains("Forbidden content"));
        assert!(hints.contains("@../../../forbidden.md"));
    }

    #[test]
    fn test_hints_without_git_import_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path();

        let current_dir = base_dir.join("current");
        fs::create_dir(&current_dir).unwrap();
        fs::write(current_dir.join("local.md"), "Local content").unwrap();

        fs::write(base_dir.join("parent.md"), "Parent content").unwrap();

        let hints_content = r#"Current directory hints
@local.md
@../parent.md
End of hints"#;
        fs::write(current_dir.join(GOOSE_HINTS_FILENAME), hints_content).unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(
            &current_dir,
            &[GOOSE_HINTS_FILENAME.to_string()],
            &gitignore,
        );

        assert!(hints.contains("Local content"));
        assert!(hints.contains("--- Content from local.md ---"));

        assert!(!hints.contains("Parent content"));
        assert!(hints.contains("@../parent.md"));
    }

    #[test]
    fn test_import_boundary_respects_nested_setting() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();
        fs::create_dir(project_root.join(".git")).unwrap();
        fs::write(project_root.join("root_file.md"), "Root file content").unwrap();
        let subdir = project_root.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("local_file.md"), "Local file content").unwrap();
        let hints_content = r#"Subdir hints
@local_file.md
@../root_file.md
End of hints"#;
        fs::write(subdir.join(GOOSE_HINTS_FILENAME), hints_content).unwrap();
        let gitignore = create_dummy_gitignore();

        let hints = load_hint_files(&subdir, &[GOOSE_HINTS_FILENAME.to_string()], &gitignore);

        assert!(hints.contains("Local file content"));
        assert!(hints.contains("--- Content from local_file.md ---"));

        assert!(hints.contains("Root file content"));
        assert!(hints.contains("--- Content from ../root_file.md ---"));
    }

    #[test]
    fn resolve_to_parent_dir_relative() {
        let wd = Path::new("/home/user/project");
        assert_eq!(
            resolve_to_parent_dir("src/main.rs", wd),
            Some(PathBuf::from("/home/user/project/src"))
        );
    }

    #[test]
    fn resolve_to_parent_dir_absolute() {
        let wd = Path::new("/home/user/project");
        assert_eq!(
            resolve_to_parent_dir("/tmp/foo.rs", wd),
            Some(PathBuf::from("/tmp"))
        );
    }

    #[test]
    fn tracker_records_path_argument() {
        let temp_dir = TempDir::new().unwrap();
        let wd = temp_dir.path().join("project");
        let src = wd.join("src");
        fs::create_dir_all(&src).unwrap();
        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "src/main.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &wd);
        let hints = tracker.load_new_hints(&wd);
        assert!(hints.is_empty());
        assert!(tracker.loaded_dirs.contains(&src.canonicalize().unwrap()));
    }

    #[test]
    fn tracker_records_command_argument() {
        let temp_dir = TempDir::new().unwrap();
        let wd = temp_dir.path().join("project");
        let nested = wd.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"command": "cat nested/doc.md"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &wd);
        let hints = tracker.load_new_hints(&wd);
        assert!(hints.is_empty());
        assert!(tracker
            .loaded_dirs
            .contains(&nested.canonicalize().unwrap()));
    }

    #[test]
    fn tracker_skips_flags_in_command() {
        let temp_dir = TempDir::new().unwrap();
        let wd = temp_dir.path().join("project");
        let src = wd.join("src");
        fs::create_dir_all(&src).unwrap();
        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"command": "grep -rn pattern src/lib.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &wd);
        let _ = tracker.load_new_hints(&wd);
        assert!(tracker.loaded_dirs.contains(&src.canonicalize().unwrap()));
        assert_eq!(tracker.loaded_dirs.len(), 1);
    }

    #[test]
    fn tracker_loads_subdirectory_hints() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().to_path_buf();
        let subdir = project_root.join("nested");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(
            subdir.join(GOOSE_HINTS_FILENAME),
            "nested subdirectory hints",
        )
        .unwrap();

        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "nested/foo.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &project_root);
        let hints = tracker.load_new_hints(&project_root);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].0.contains("nested"));
        assert!(hints[0].1.contains("nested subdirectory hints"));
    }

    #[test]
    fn tracker_aggregates_output_limit_across_subdirectories() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().to_path_buf();
        for directory in ["first", "second", "third"] {
            fs::create_dir(project_root.join(directory)).unwrap();
        }
        fs::write(
            project_root.join("first").join(GOOSE_HINTS_FILENAME),
            "A1".repeat(300 * 1024),
        )
        .unwrap();
        fs::write(
            project_root.join("second").join(GOOSE_HINTS_FILENAME),
            "B2".repeat(300 * 1024),
        )
        .unwrap();
        fs::write(
            project_root.join("third").join(GOOSE_HINTS_FILENAME),
            "third hint",
        )
        .unwrap();

        let mut tracker = SubdirectoryHintTracker::new();
        for directory in ["first", "second", "third"] {
            let args: serde_json::Map<String, serde_json::Value> = serde_json::from_value(
                serde_json::json!({ "path": format!("{directory}/file.rs") }),
            )
            .unwrap();
            tracker.record_tool_arguments(&Some(args), &project_root);
        }

        let hints = tracker.load_new_hints(&project_root);
        let output_bytes: usize = hints.iter().map(|(_, content)| content.len()).sum();
        let combined = hints
            .iter()
            .map(|(_, content)| content.as_str())
            .collect::<String>();

        assert!(output_bytes <= MAX_HINT_OUTPUT_BYTES);
        assert!(combined.contains("A1"));
        assert!(!combined.contains("B2"));
        assert!(combined.contains("third hint"));
    }

    #[test]
    fn tracker_deduplicates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().to_path_buf();
        let subdir = project_root.join("nested");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join(GOOSE_HINTS_FILENAME), "nested hints").unwrap();

        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "nested/foo.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args.clone()), &project_root);
        let hints = tracker.load_new_hints(&project_root);
        assert_eq!(hints.len(), 1);

        tracker.record_tool_arguments(&Some(args), &project_root);
        let hints = tracker.load_new_hints(&project_root);
        assert!(hints.is_empty());
    }

    #[test]
    fn tracker_rejects_parent_traversal_outside_working_directory() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().join("project");
        let nested = project_root.join("nested");
        let outside = temp_dir.path().join("outside");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join(GOOSE_HINTS_FILENAME), "outside hints").unwrap();

        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "nested/../../outside/file.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &project_root);

        assert!(tracker.load_new_hints(&project_root).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn tracker_rejects_directory_symlink_outside_working_directory() {
        use std::os::unix::fs::symlink;

        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().join("project");
        let outside = temp_dir.path().join("outside");
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join(GOOSE_HINTS_FILENAME), "outside hints").unwrap();
        symlink(&outside, project_root.join("alias")).unwrap();

        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "alias/file.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &project_root);

        assert!(tracker.load_new_hints(&project_root).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn tracker_preserves_in_boundary_symlinks_and_deduplicates_aliases() {
        use std::os::unix::fs::symlink;

        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().join("project");
        let real = project_root.join("real");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join(GOOSE_HINTS_FILENAME), "real hints").unwrap();
        symlink(&real, project_root.join("alias")).unwrap();

        let mut tracker = SubdirectoryHintTracker::new();
        let alias_args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "alias/file.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(alias_args), &project_root);
        let hints = tracker.load_new_hints(&project_root);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].1.contains("real hints"));

        let real_args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "real/file.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(real_args), &project_root);
        assert!(tracker.load_new_hints(&project_root).is_empty());
        assert_eq!(tracker.loaded_dirs.len(), 1);
    }

    #[test]
    fn tracker_retries_directory_after_parent_is_created() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().join("project");
        fs::create_dir_all(&project_root).unwrap();

        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "future/file.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args.clone()), &project_root);
        assert!(tracker.load_new_hints(&project_root).is_empty());
        assert!(tracker.loaded_dirs.is_empty());

        let future = project_root.join("future");
        fs::create_dir_all(&future).unwrap();
        fs::write(future.join(GOOSE_HINTS_FILENAME), "future hints").unwrap();
        tracker.record_tool_arguments(&Some(args), &project_root);

        let hints = tracker.load_new_hints(&project_root);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].1.contains("future hints"));
    }
}

#[cfg(test)]
mod gitignore_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_hints_with_gitignore_filters_referenced_files() {
        let dir = TempDir::new().unwrap();
        let project_root = dir.path();

        fs::create_dir(project_root.join(".git")).unwrap();
        fs::write(project_root.join("allowed.md"), "Allowed content").unwrap();
        fs::write(project_root.join("secret.env"), "SECRET_KEY=abc123").unwrap();
        fs::write(project_root.join(".gitignore"), "*.env\n").unwrap();

        let hints_content = "Project hints\n@allowed.md\n@secret.env\nEnd of hints";
        fs::write(project_root.join(GOOSE_HINTS_FILENAME), hints_content).unwrap();

        let gitignore = build_gitignore(project_root);

        let hints = load_hint_files(
            project_root,
            &[GOOSE_HINTS_FILENAME.to_string()],
            &gitignore,
        );

        assert!(hints.contains("Allowed content"));
        assert!(!hints.contains("SECRET_KEY=abc123"));
        assert!(hints.contains("@secret.env"));
    }

    #[test]
    fn test_build_gitignore_loads_from_git_root_in_subdirectory() {
        let dir = TempDir::new().unwrap();
        let project_root = dir.path();

        fs::create_dir(project_root.join(".git")).unwrap();
        // Root .gitignore ignores .env files
        fs::write(project_root.join(".gitignore"), "*.env\n").unwrap();
        fs::write(project_root.join("secret.env"), "SECRET_KEY=abc123").unwrap();
        fs::write(project_root.join("allowed.md"), "Allowed content").unwrap();

        let subdir = project_root.join("subdir");
        fs::create_dir(&subdir).unwrap();

        let hints_content = "Subdir hints\n@../allowed.md\n@../secret.env\nEnd of hints";
        fs::write(subdir.join(GOOSE_HINTS_FILENAME), hints_content).unwrap();

        // Build gitignore from the subdirectory — should still pick up root .gitignore
        let gitignore = build_gitignore(&subdir);

        let hints = load_hint_files(&subdir, &[GOOSE_HINTS_FILENAME.to_string()], &gitignore);

        assert!(hints.contains("Allowed content"));
        assert!(!hints.contains("SECRET_KEY=abc123"));
        assert!(hints.contains("@../secret.env"));
    }

    #[test]
    fn test_build_gitignore_merges_nested_gitignores() {
        let dir = TempDir::new().unwrap();
        let project_root = dir.path();

        fs::create_dir(project_root.join(".git")).unwrap();
        // Root ignores *.log
        fs::write(project_root.join(".gitignore"), "*.log\n").unwrap();

        let subdir = project_root.join("subdir");
        fs::create_dir(&subdir).unwrap();
        // Subdir ignores *.tmp
        fs::write(subdir.join(".gitignore"), "*.tmp\n").unwrap();

        fs::write(project_root.join("debug.log"), "debug log").unwrap();
        fs::write(subdir.join("cache.tmp"), "temp data").unwrap();
        fs::write(subdir.join("readme.md"), "Readme content").unwrap();

        let hints_content = "Hints\n@../debug.log\n@cache.tmp\n@readme.md\nEnd";
        fs::write(subdir.join(GOOSE_HINTS_FILENAME), hints_content).unwrap();

        let gitignore = build_gitignore(&subdir);
        let hints = load_hint_files(&subdir, &[GOOSE_HINTS_FILENAME.to_string()], &gitignore);

        assert!(hints.contains("Readme content"));
        assert!(!hints.contains("debug log"));
        assert!(!hints.contains("temp data"));
        assert!(hints.contains("@../debug.log"));
        assert!(hints.contains("@cache.tmp"));
    }
}
