//! Everything specific to skills: filesystem discovery (`SKILL.md` walking +
//! built-ins) and the runtime MCP client (`client` submodule). User-facing
//! CRUD lives in `crate::sources`, which generalizes across source types.

mod builtin;
pub mod client;

pub use client::{SkillsClient, EXTENSION_NAME};

use crate::config::paths::Paths;
use crate::sources::parse_frontmatter;
use goose_sdk::custom_requests::{SourceEntry, SourceType};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Shared YAML frontmatter shape for `SKILL.md` files. `name` is optional at
/// the parser level so callers can decide whether to require it (runtime
/// discovery does; source CRUD uses the directory name instead).
#[derive(Debug, Deserialize)]
pub struct SkillFrontmatter {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: String,
}

/// Canonical writable location for global user skills: `~/.agents/skills`.
pub fn global_skills_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".agents").join("skills"))
}

/// Canonical writable location for project-scoped skills:
/// `<project>/.goose/skills`.
pub fn project_skills_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".goose").join("skills")
}

/// Every directory the agent reads skills from, paired with whether each is a
/// global (home-rooted) location. Order matches discovery precedence: project
/// dirs first, then global dirs.
pub fn all_skill_dirs(working_dir: Option<&Path>) -> Vec<(PathBuf, bool)> {
    let mut dirs: Vec<(PathBuf, bool)> = Vec::new();

    if let Some(wd) = working_dir {
        dirs.push((wd.join(".goose").join("skills"), false));
        dirs.push((wd.join(".claude").join("skills"), false));
        dirs.push((wd.join(".agents").join("skills"), false));
    }

    let home = dirs::home_dir();
    if let Some(h) = home.as_ref() {
        dirs.push((h.join(".agents").join("skills"), true));
    }
    dirs.push((Paths::config_dir().join("skills"), true));
    if let Some(h) = home.as_ref() {
        dirs.push((h.join(".claude").join("skills"), true));
        dirs.push((h.join(".config").join("agents").join("skills"), true));
    }

    dirs
}

fn parse_skill_content(content: &str, path: &Path, global: bool) -> Option<SourceEntry> {
    let (metadata, body): (SkillFrontmatter, String) = match parse_frontmatter(content) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => return None,
        Err(e) => {
            warn!("Failed to parse skill frontmatter: {}", e);
            return None;
        }
    };

    let name = metadata.name.filter(|n| !n.is_empty())?;

    if name.contains('/') {
        warn!("Skill name '{}' contains '/', skipping", name);
        return None;
    }

    Some(SourceEntry {
        source_type: SourceType::Skill,
        name,
        description: metadata.description,
        content: body,
        directory: path.to_string_lossy().into_owned(),
        global,
        supporting_files: Vec::new(),
    })
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git") | Some(".hg") | Some(".svn")
    )
}

fn walk_files_recursively<F, G>(
    dir: &Path,
    visited_dirs: &mut HashSet<PathBuf>,
    should_descend: &mut G,
    visit_file: &mut F,
) where
    F: FnMut(&Path),
    G: FnMut(&Path) -> bool,
{
    let canonical_dir = match std::fs::canonicalize(dir) {
        Ok(path) => path,
        Err(_) => return,
    };

    if !visited_dirs.insert(canonical_dir) {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if should_descend(&path) {
                walk_files_recursively(&path, visited_dirs, should_descend, visit_file);
            }
        } else if path.is_file() {
            visit_file(&path);
        }
    }
}

fn scan_skills_from_dir(dir: &Path, global: bool, seen: &mut HashSet<String>) -> Vec<SourceEntry> {
    let mut skill_files = Vec::new();
    let mut visited_dirs = HashSet::new();

    walk_files_recursively(
        dir,
        &mut visited_dirs,
        &mut |path| !should_skip_dir(path),
        &mut |path| {
            if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
                skill_files.push(path.to_path_buf());
            }
        },
    );

    let mut sources = Vec::new();
    for skill_file in skill_files {
        let Some(skill_dir) = skill_file.parent() else {
            continue;
        };
        let content = match std::fs::read_to_string(&skill_file) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read skill file {}: {}", skill_file.display(), e);
                continue;
            }
        };

        if let Some(mut source) = parse_skill_content(&content, skill_dir, global) {
            if !seen.contains(&source.name) {
                let mut files = Vec::new();
                let mut visited_support_dirs = HashSet::new();
                walk_files_recursively(
                    skill_dir,
                    &mut visited_support_dirs,
                    &mut |path| !should_skip_dir(path) && !path.join("SKILL.md").is_file(),
                    &mut |path| {
                        if path.file_name().and_then(|n| n.to_str()) != Some("SKILL.md") {
                            files.push(path.to_string_lossy().into_owned());
                        }
                    },
                );
                source.supporting_files = files;

                seen.insert(source.name.clone());
                sources.push(source);
            }
        }
    }
    sources
}

/// Discover skills from all configured filesystem locations and built-ins.
/// Each returned entry has `global` set according to the directory it was
/// found in (or `true` for built-ins).
pub fn discover_skills(working_dir: Option<&Path>) -> Vec<SourceEntry> {
    let mut sources: Vec<SourceEntry> = Vec::new();
    let mut seen = HashSet::new();

    for (dir, is_global) in all_skill_dirs(working_dir) {
        for source in scan_skills_from_dir(&dir, is_global, &mut seen) {
            sources.push(source);
        }
    }

    for content in builtin::get_all() {
        if let Some(source) = parse_skill_content(content, &PathBuf::new(), true) {
            if !seen.contains(&source.name) {
                seen.insert(source.name.clone());
                sources.push(SourceEntry {
                    source_type: SourceType::BuiltinSkill,
                    ..source
                });
            }
        }
    }

    sources
}

pub fn list_installed_skills(working_dir: Option<&Path>) -> Vec<SourceEntry> {
    let fallback;
    let wd = match working_dir {
        Some(p) => Some(p),
        None => {
            fallback = std::env::current_dir().ok();
            fallback.as_deref()
        }
    };
    discover_skills(wd)
}
