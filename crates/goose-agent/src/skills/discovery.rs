//! Native filesystem skill discovery under caller-supplied roots.

use goose_sdk_types::custom_requests::{SourceEntry, SourceType};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::warn;

/// A host-selected search root; earlier roots take precedence.
#[derive(Debug, Clone)]
pub struct SkillRoot {
    /// Directory searched recursively for `SKILL.md`. Returned entry paths identify
    /// the containing skill directory, not the Markdown file.
    /// Relative roots are resolved against the process working directory.
    pub path: PathBuf,
    /// Scope metadata copied to each entry, not inferred from its location.
    pub global: bool,
    /// Writability metadata copied to each entry. Discovery does not check
    /// permissions or enforce whether a host may modify the skill.
    pub writable: bool,
    /// Retain lexical skill-directory and supporting-file paths (e.g. plugin links)
    /// instead of canonicalizing the skill directory. Relative roots then produce
    /// relative result paths; supply absolute roots when absolute paths are needed.
    /// This does not change symlink traversal or confine reads to the root.
    pub preserve_path: bool,
}

#[derive(Debug, Deserialize)]
struct Frontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    metadata: HashMap<String, Value>,
}

fn parse_frontmatter<T: for<'de> Deserialize<'de>>(
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

fn parse_skill_content(
    content: &str,
    path: &Path,
    global: bool,
    writable: bool,
) -> Option<SourceEntry> {
    let (metadata, body): (Frontmatter, String) = match parse_frontmatter(content) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => return None,
        Err(e) => {
            warn!("Failed to parse skill frontmatter: {}", e);
            return None;
        }
    };

    let name = match metadata.name.filter(|n| !n.is_empty()) {
        Some(n) => n,
        None => {
            warn!(
                "Skill at '{}' is missing a required 'name' in frontmatter, skipping",
                path.display()
            );
            return None;
        }
    };

    if name.contains('/') {
        warn!("Skill name '{}' contains '/', skipping", name);
        return None;
    }

    Some(SourceEntry {
        source_type: SourceType::Skill,
        name,
        description: metadata.description,
        content: body,
        path: path.to_string_lossy().into_owned(),
        global,
        writable,
        supporting_files: Vec::new(),
        properties: metadata.metadata,
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

fn scan_skills_from_dir(
    dir: &Path,
    global: bool,
    writable: bool,
    preserve_path: bool,
    seen: &mut HashSet<String>,
) -> Vec<SourceEntry> {
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
        let registered_skill_dir = if preserve_path {
            skill_dir.to_path_buf()
        } else {
            let Ok(canonical_dir) = skill_dir.canonicalize() else {
                continue;
            };
            canonical_dir
        };
        let content = match std::fs::read_to_string(&skill_file) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read skill file {}: {}", skill_file.display(), e);
                continue;
            }
        };

        if let Some(mut source) =
            parse_skill_content(&content, &registered_skill_dir, global, writable)
        {
            if !seen.contains(&source.name) {
                let mut files = Vec::new();
                let mut visited_support_dirs = HashSet::new();
                walk_files_recursively(
                    skill_dir,
                    &mut visited_support_dirs,
                    &mut |path| !should_skip_dir(path) && !path.join("SKILL.md").is_file(),
                    &mut |path| {
                        if path.file_name().and_then(|n| n.to_str()) != Some("SKILL.md") {
                            if let Ok(relative) = path.strip_prefix(skill_dir) {
                                files.push(
                                    registered_skill_dir
                                        .join(relative)
                                        .to_string_lossy()
                                        .into_owned(),
                                );
                            }
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

/// Recursively discovers skills from explicit roots, without consulting host configuration.
///
/// Roots are visited in input order. The first successfully parsed entry with a
/// given name wins; names are compared exactly and case-sensitively. Traversal
/// within each root is unsorted, so result order, duplicate-name winners within a
/// root, and supporting-file order are unspecified.
///
/// Discovery is best-effort: missing or unreadable directories and invalid or
/// unreadable skills are skipped. Some failures emit tracing warnings; no errors
/// are returned. An empty result does not distinguish no skills from a failed scan.
///
/// Traversal follows file and directory symlinks, including targets outside a
/// root. Canonical directory identities prevent cycles. `preserve_path` only
/// affects returned path spelling; discovery is not a confined file-open API.
/// The caller must enforce access policy when opening returned paths.
pub fn discover_skills(roots: impl IntoIterator<Item = SkillRoot>) -> Vec<SourceEntry> {
    let mut sources: Vec<SourceEntry> = Vec::new();
    let mut seen = HashSet::new();

    for dir in roots {
        for source in scan_skills_from_dir(
            &dir.path,
            dir.global,
            dir.writable,
            dir.preserve_path,
            &mut seen,
        ) {
            sources.push(source);
        }
    }

    sources
}
