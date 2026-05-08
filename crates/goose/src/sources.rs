//! Filesystem-backed CRUD for [`SourceEntry`] values exchanged over ACP custom
//! methods. Skills live in `~/.agents/skills/` (or per-project under
//! `<project>/.agents/skills/`). Projects live in `<dataDir>/projects/<slug>.md`.

use crate::config::paths::Paths;
use crate::skills::{
    build_skill_md, discover_skills, infer_skill_name, is_global_skill_dir,
    resolve_discoverable_skill_dir, resolve_skill_dir, skill_base_dir, validate_skill_name,
};
use crate::source_roots::SourceRoot;
use agent_client_protocol::Error;
use fs_err as fs;
use goose_sdk::custom_requests::{SourceEntry, SourceType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use tracing::warn;
use zip::write::SimpleFileOptions;

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

fn require_mutable_type(source_type: SourceType) -> Result<(), Error> {
    match source_type {
        SourceType::Skill | SourceType::Project | SourceType::Agent => Ok(()),
        other => Err(Error::invalid_params().data(format!(
            "Source type '{other}' is not supported for mutation."
        ))),
    }
}

fn require_listable_type(source_type: Option<SourceType>) -> Result<SourceType, Error> {
    match source_type.unwrap_or(SourceType::Skill) {
        SourceType::Skill => Ok(SourceType::Skill),
        SourceType::BuiltinSkill => Ok(SourceType::BuiltinSkill),
        SourceType::Project => Ok(SourceType::Project),
        SourceType::Agent => Ok(SourceType::Agent),
        other => Err(Error::invalid_params().data(format!(
            "Source type '{}' is not supported for listing.",
            other
        ))),
    }
}

// --- Project helpers ---

#[derive(Deserialize)]
struct MarkdownSourceFrontmatter {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default, flatten)]
    properties: HashMap<String, serde_json::Value>,
}

fn projects_dir() -> PathBuf {
    Paths::data_dir().join("projects")
}

fn project_file_path(slug: &str) -> PathBuf {
    projects_dir().join(format!("{slug}.md"))
}

fn build_source_md<T: Serialize>(frontmatter: &T, content: &str) -> Result<String, Error> {
    let yaml = serde_yaml::to_string(frontmatter)
        .map_err(|e| Error::internal_error().data(format!("Failed to serialize source: {e}")))?;
    let mut md = format!("---\n{yaml}---\n");
    if !content.is_empty() {
        md.push('\n');
        md.push_str(content);
        md.push('\n');
    }
    Ok(md)
}

fn build_source_markdown(
    name: &str,
    description: &str,
    content: &str,
    properties: &HashMap<String, serde_json::Value>,
) -> Result<String, Error> {
    let mut frontmatter = serde_yaml::Mapping::new();
    frontmatter.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(name.into()),
    );
    frontmatter.insert(
        serde_yaml::Value::String("description".into()),
        serde_yaml::Value::String(description.into()),
    );
    for (key, value) in properties {
        if key == "name" || key == "description" {
            continue;
        }
        let value = serde_yaml::to_value(value).map_err(|e| {
            Error::internal_error().data(format!("Failed to serialize source property: {e}"))
        })?;
        frontmatter.insert(serde_yaml::Value::String(key.clone()), value);
    }
    build_source_md(&frontmatter, content)
}

/// Returns (display_name, description, body, properties).
fn parse_project_frontmatter(
    raw: &str,
) -> (String, String, String, HashMap<String, serde_json::Value>) {
    if !raw.trim_start().starts_with("---") {
        return (
            String::new(),
            String::new(),
            raw.to_string(),
            HashMap::new(),
        );
    }
    match parse_frontmatter::<MarkdownSourceFrontmatter>(raw) {
        Ok(Some((meta, body))) => (meta.name, meta.description, body, meta.properties),
        _ => (
            String::new(),
            String::new(),
            raw.to_string(),
            HashMap::new(),
        ),
    }
}

/// Validate a project slug. Same shape as a skill name (kebab-case, ASCII).
fn validate_project_slug(slug: &str) -> Result<(), Error> {
    validate_skill_name(slug)
}

/// Read the `metadata:` field out of an existing SKILL.md, returning an
/// empty map if the file is missing, malformed, or carries no metadata.
fn read_existing_skill_properties(skill_dir: &Path) -> HashMap<String, serde_json::Value> {
    let raw = match fs::read_to_string(skill_dir.join("SKILL.md")) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    match parse_frontmatter::<crate::skills::SkillFrontmatter>(&raw) {
        Ok(Some((meta, _))) => meta.metadata,
        _ => HashMap::new(),
    }
}

/// Read the properties bag out of an existing project file.
fn read_existing_project_properties(file: &Path) -> HashMap<String, serde_json::Value> {
    let raw = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    let (_, _, _, properties) = parse_project_frontmatter(&raw);
    properties
}

/// Read the properties bag out of an existing agent file.
fn read_existing_agent_properties(file: &Path) -> HashMap<String, serde_json::Value> {
    let raw = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    match parse_agent_frontmatter(&raw) {
        Ok((frontmatter, _)) => frontmatter.properties,
        Err(_) => HashMap::new(),
    }
}

fn project_entry_from_file(file: &Path) -> Option<SourceEntry> {
    let slug = file.file_stem().and_then(|s| s.to_str())?.to_string();
    if slug.is_empty() {
        return None;
    }
    let raw = fs::read_to_string(file).ok()?;
    let (title, description, content, mut properties) = parse_project_frontmatter(&raw);
    let display_name = if title.is_empty() {
        slug.clone()
    } else {
        title
    };
    if display_name != slug {
        // Preserve the user-facing display name so the frontend doesn't have
        // to special-case slug vs title.
        properties.insert(
            "title".into(),
            serde_json::Value::String(display_name.clone()),
        );
    }
    Some(SourceEntry {
        source_type: SourceType::Project,
        name: slug,
        description,
        content,
        path: file.to_string_lossy().into_owned(),
        global: true,
        writable: true,
        supporting_files: Vec::new(),
        properties,
    })
}

/// Read all projects from `<dataDir>/projects/`.
fn read_project_dir() -> Result<Vec<SourceEntry>, Error> {
    let dir = projects_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(&dir)
        .map_err(|e| Error::internal_error().data(format!("Failed to read projects dir: {e}")))?;

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(entry) = project_entry_from_file(&path) {
            out.push(entry);
        }
    }
    Ok(out)
}

/// Read a single project source by slug.
pub fn read_project(slug: &str) -> Result<SourceEntry, Error> {
    validate_project_slug(slug)?;
    let file = project_file_path(slug);
    if !file.exists() {
        return Err(Error::invalid_params().data(format!("Project \"{}\" not found", slug)));
    }
    project_entry_from_file(&file)
        .ok_or_else(|| Error::internal_error().data("Failed to read project file"))
}

/// Get the working directories configured for a project, if any.
/// Returns an empty Vec when the project doesn't exist or has none configured.
pub fn project_working_dirs(slug: &str) -> Vec<String> {
    let entry = match read_project(slug) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    entry
        .properties
        .get("workingDirs")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or_default()
}

/// Validate that the given path is a project file we manage and the file
/// exists. Returns the canonical path on success.
fn resolve_project_path(path: &str) -> Result<PathBuf, Error> {
    let canonical_path = Path::new(path).canonicalize().map_err(|_| {
        Error::invalid_params().data(format!("Project source \"{}\" not found", path))
    })?;
    let canonical_root = projects_dir()
        .canonicalize()
        .unwrap_or_else(|_| projects_dir());
    if !canonical_path.starts_with(&canonical_root) {
        return Err(Error::invalid_params().data(format!(
            "Path \"{}\" is not a project source",
            canonical_path.display()
        )));
    }
    if canonical_path.extension().and_then(|e| e.to_str()) != Some("md") {
        return Err(
            Error::invalid_params().data(format!("Path \"{}\" is not a markdown file", path))
        );
    }
    if !canonical_path.is_file() {
        return Err(Error::invalid_params().data(format!("Project source \"{}\" not found", path)));
    }
    Ok(canonical_path)
}

// --- SourceEntry construction ---

fn skill_source_entry(
    name: &str,
    description: &str,
    content: &str,
    dir: &Path,
    global: bool,
    properties: HashMap<String, serde_json::Value>,
) -> SourceEntry {
    SourceEntry {
        source_type: SourceType::Skill,
        name: name.to_string(),
        description: description.to_string(),
        content: content.to_string(),
        path: dir.to_string_lossy().to_string(),
        global,
        writable: true,
        supporting_files: Vec::new(),
        properties,
    }
}

fn builtin_skill_entry(mut source: SourceEntry) -> SourceEntry {
    source.source_type = SourceType::BuiltinSkill;
    source.path = format!("builtin://skills/{}", source.name);
    source.global = true;
    source.supporting_files.clear();
    source
}

fn agent_base_dir(global: bool, project_dir: Option<&str>) -> Result<PathBuf, Error> {
    if global {
        Ok(Paths::agents_dir())
    } else {
        let project_dir = project_dir.ok_or_else(|| {
            Error::invalid_params().data("projectDir is required when global is false")
        })?;
        if project_dir.trim().is_empty() {
            return Err(
                Error::invalid_params().data("projectDir must not be empty when global is false")
            );
        }
        Ok(Path::new(project_dir).join(".agents").join("agents"))
    }
}

fn validate_agent_name(name: &str) -> Result<(), Error> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Error::invalid_params().data("Agent name must not be empty"));
    }
    if trimmed.len() > 80 {
        return Err(Error::invalid_params().data(format!(
            "Invalid agent name \"{}\". Names must be at most 80 characters.",
            name
        )));
    }
    if trimmed.chars().any(|ch| matches!(ch, '/' | '\\')) {
        return Err(Error::invalid_params().data(format!(
            "Invalid agent name \"{}\". Names must not contain path separators.",
            name
        )));
    }
    Ok(())
}

fn slugify_agent_name(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let mut collapsed = String::with_capacity(slug.len());
    let mut previous_hyphen = false;
    for ch in slug.chars() {
        if ch == '-' {
            if !previous_hyphen {
                collapsed.push('-');
            }
            previous_hyphen = true;
        } else {
            collapsed.push(ch);
            previous_hyphen = false;
        }
    }
    let trimmed = collapsed.trim_matches('-');
    if trimmed.is_empty() {
        "agent".to_string()
    } else {
        trimmed
            .chars()
            .take(64)
            .collect::<String>()
            .trim_end_matches('-')
            .to_string()
    }
}

fn parse_agent_frontmatter(raw: &str) -> Result<(MarkdownSourceFrontmatter, String), Error> {
    parse_frontmatter::<MarkdownSourceFrontmatter>(raw)
        .map_err(|e| Error::invalid_params().data(format!("Invalid agent frontmatter: {e}")))?
        .ok_or_else(|| Error::invalid_params().data("Agent file is missing frontmatter"))
}

fn agent_source_entry(path: &Path, global: bool, writable: bool) -> Result<SourceEntry, Error> {
    let raw = fs::read_to_string(path)
        .map_err(|e| Error::internal_error().data(format!("Failed to read agent file: {e}")))?;
    let (frontmatter, content) = parse_agent_frontmatter(&raw)?;
    Ok({
        SourceEntry {
            source_type: SourceType::Agent,
            name: frontmatter.name,
            description: frontmatter.description,
            content,
            path: path.to_string_lossy().to_string(),
            global,
            writable,
            supporting_files: Vec::new(),
            properties: frontmatter.properties,
        }
    })
}

fn canonicalize_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn is_under_root(path: &Path, root: &Path) -> bool {
    canonicalize_or_original(path).starts_with(canonicalize_or_original(root))
}

fn is_read_only_agent_file(path: &Path, additional_roots: &[SourceRoot]) -> bool {
    additional_roots
        .iter()
        .filter(|root| !root.writable)
        .any(|root| is_under_root(path, &root.path))
}

fn reject_read_only_agent_file(path: &Path, additional_roots: &[SourceRoot]) -> Result<(), Error> {
    if is_read_only_agent_file(path, additional_roots) {
        return Err(Error::invalid_params().data("Source is read-only"));
    }
    Ok(())
}

fn is_global_agent_file(path: &Path) -> bool {
    let canonical_path = canonicalize_or_original(path);
    let mut global_roots = Vec::new();
    global_roots.push(Paths::agents_dir());
    if let Some(home) = dirs::home_dir() {
        global_roots.push(home.join(".agents").join("agents"));
        global_roots.push(home.join(".goose").join("agents"));
        global_roots.push(home.join(".claude").join("agents"));
    }
    global_roots.push(Paths::config_dir().join("agents"));

    global_roots
        .into_iter()
        .any(|root| canonical_path.starts_with(canonicalize_or_original(&root)))
}

fn resolve_agent_file_with_roots(
    path: &str,
    additional_roots: &[SourceRoot],
) -> Result<PathBuf, Error> {
    if path.is_empty() {
        return Err(Error::invalid_params().data("Source path must not be empty"));
    }

    let canonical_file = Path::new(path)
        .canonicalize()
        .map_err(|_| Error::invalid_params().data(format!("Source \"{}\" not found", path)))?;

    let parent_name = canonical_file
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str());
    let grandparent_name = canonical_file
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str());
    let in_agent_dir = parent_name == Some("agents")
        && matches!(
            grandparent_name,
            Some(".goose") | Some(".claude") | Some(".agents")
        );
    let in_additional_root = additional_roots
        .iter()
        .any(|root| is_under_root(&canonical_file, &root.path));

    if !canonical_file.is_file()
        || canonical_file.extension().and_then(|ext| ext.to_str()) != Some("md")
        || (!in_agent_dir && !is_global_agent_file(&canonical_file) && !in_additional_root)
    {
        return Err(Error::invalid_params().data(format!("Source \"{}\" not found", path)));
    }

    Ok(canonical_file)
}

fn list_agent_dirs(working_dir: Option<&Path>, additional_roots: &[SourceRoot]) -> Vec<SourceRoot> {
    let mut dirs = Vec::new();
    if let Some(working_dir) = working_dir {
        dirs.push(SourceRoot {
            path: working_dir.join(".agents").join("agents"),
            writable: true,
        });
        dirs.push(SourceRoot {
            path: working_dir.join(".goose").join("agents"),
            writable: true,
        });
        dirs.push(SourceRoot {
            path: working_dir.join(".claude").join("agents"),
            writable: true,
        });
    }

    dirs.push(SourceRoot {
        path: Paths::agents_dir(),
        writable: true,
    });
    if let Some(home) = dirs::home_dir() {
        dirs.push(SourceRoot {
            path: home.join(".agents").join("agents"),
            writable: true,
        });
        dirs.push(SourceRoot {
            path: home.join(".goose").join("agents"),
            writable: true,
        });
        dirs.push(SourceRoot {
            path: home.join(".claude").join("agents"),
            writable: true,
        });
    }
    dirs.push(SourceRoot {
        path: Paths::config_dir().join("agents"),
        writable: true,
    });
    dirs.extend(additional_roots.iter().cloned());
    dirs
}

fn is_project_agent_file(path: &Path, working_dir: &Path) -> bool {
    [".agents", ".goose", ".claude"]
        .into_iter()
        .map(|dir| working_dir.join(dir).join("agents"))
        .any(|root| is_under_root(path, &root))
}

fn list_agent_sources(
    project_dir: Option<&str>,
    additional_roots: &[SourceRoot],
) -> Vec<SourceEntry> {
    let working_dir = project_dir
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let mut seen = std::collections::HashSet::new();
    let mut sources = Vec::new();

    for root in list_agent_dirs(working_dir.as_deref(), additional_roots) {
        let entries = match fs::read_dir(&root.path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let global = working_dir
                .as_deref()
                .is_none_or(|working_dir| !is_project_agent_file(&path, working_dir));
            match agent_source_entry(&path, global, root.writable) {
                Ok(source) => {
                    let key = source.name.to_lowercase();
                    if seen.insert(key) {
                        sources.push(source);
                    }
                }
                Err(err) => warn!("Skipping agent source {}: {:?}", path.display(), err),
            }
        }
    }

    sources
}

fn create_agent_source(
    name: &str,
    description: &str,
    content: &str,
    properties: HashMap<String, serde_json::Value>,
    global: bool,
    project_dir: Option<&str>,
) -> Result<SourceEntry, Error> {
    validate_agent_name(name)?;
    let base = agent_base_dir(global, project_dir)?;
    let slug = slugify_agent_name(name);
    let mut file_path = base.join(format!("{slug}.md"));
    if file_path.exists() {
        let mut counter = 2u32;
        loop {
            file_path = base.join(format!("{slug}-{counter}.md"));
            if !file_path.exists() {
                break;
            }
            counter += 1;
        }
    }

    fs::create_dir_all(&base).map_err(|e| {
        Error::internal_error().data(format!("Failed to create source directory: {e}"))
    })?;
    let md = build_source_markdown(name, description, content, &properties)?;
    fs::write(&file_path, md)
        .map_err(|e| Error::internal_error().data(format!("Failed to write agent file: {e}")))?;

    agent_source_entry(&file_path, global, true)
}

fn update_agent_source(
    path: &str,
    name: &str,
    description: &str,
    content: &str,
    properties: Option<HashMap<String, serde_json::Value>>,
    additional_roots: &[SourceRoot],
) -> Result<SourceEntry, Error> {
    validate_agent_name(name)?;
    let file_path = resolve_agent_file_with_roots(path, additional_roots)?;
    reject_read_only_agent_file(&file_path, additional_roots)?;
    let global = is_global_agent_file(&file_path);
    let resolved_properties = match properties {
        Some(p) => p,
        None => read_existing_agent_properties(&file_path),
    };
    let md = build_source_markdown(name, description, content, &resolved_properties)?;
    fs::write(&file_path, md)
        .map_err(|e| Error::internal_error().data(format!("Failed to write agent file: {e}")))?;

    agent_source_entry(&file_path, global, true)
}

// --- Import/export helpers ---

const MARKDOWN_MIME: &str = "text/markdown";
const ZIP_MIME: &str = "application/zip";

#[derive(Debug)]
pub struct SourceExport {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub mime_type: &'static str,
}

fn archive_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn should_skip_source_export_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git") | Some(".hg") | Some(".svn")
    )
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<Cursor<&mut Vec<u8>>>,
    source_dir: &Path,
    archive_root: &str,
) -> Result<(), Error> {
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut stack = vec![source_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| {
            Error::internal_error().data(format!("Failed to read source tree: {e}"))
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if !should_skip_source_export_dir(&path) {
                    stack.push(path);
                }
                continue;
            }
            if !path.is_file() {
                continue;
            }

            let relative = path.strip_prefix(source_dir).map_err(|e| {
                Error::internal_error().data(format!("Failed to build archive path: {e}"))
            })?;
            zip.start_file(
                format!("{archive_root}/{}", archive_path(relative)),
                options,
            )
            .map_err(|e| {
                Error::internal_error().data(format!("Failed to write source archive: {e}"))
            })?;
            zip.write_all(&fs::read(&path).map_err(|e| {
                Error::internal_error().data(format!("Failed to read source file: {e}"))
            })?)
            .map_err(|e| {
                Error::internal_error().data(format!("Failed to write source archive: {e}"))
            })?;
        }
    }

    Ok(())
}

fn zip_source_tree(source_dir: &Path, archive_root: &str) -> Result<Vec<u8>, Error> {
    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut zip = zip::ZipWriter::new(cursor);
        add_dir_to_zip(&mut zip, source_dir, archive_root)?;
        zip.finish().map_err(|e| {
            Error::internal_error().data(format!("Failed to finish source archive: {e}"))
        })?;
    }
    Ok(buffer)
}

fn safe_import_filename(filename: &str) -> Result<&str, Error> {
    Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| Error::invalid_params().data("Import filename must not be empty"))
}

fn path_component_string(path: &Path, fallback: &str) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn infer_import_source_type(
    filename: &str,
    requested: Option<SourceType>,
) -> Result<SourceType, Error> {
    let lower = filename.to_lowercase();
    let source_type = match requested {
        Some(SourceType::Skill) => SourceType::Skill,
        Some(SourceType::Project) => SourceType::Project,
        Some(SourceType::Agent) => SourceType::Agent,
        Some(other) => {
            return Err(Error::invalid_params()
                .data(format!("Source type '{}' import is not supported.", other)))
        }
        None if lower.ends_with(".zip") => SourceType::Skill,
        None if lower.ends_with(".project.md") => SourceType::Project,
        None if lower.ends_with(".agent.md") || lower.ends_with(".persona.md") => SourceType::Agent,
        None if lower.ends_with(".md") => SourceType::Agent,
        None => {
            return Err(Error::invalid_params().data(
                "Unsupported source file type. Import a .md source file or .zip source tree.",
            ))
        }
    };

    if lower.ends_with(".zip") && source_type != SourceType::Skill {
        return Err(Error::invalid_params().data("Only skill imports support .zip archives"));
    }

    Ok(source_type)
}

fn next_available_name(name: &str, mut exists: impl FnMut(&str) -> bool) -> String {
    if !exists(name) {
        return name.to_string();
    }

    (2..)
        .map(|counter| format!("{name}-{counter}"))
        .find(|candidate| !exists(candidate))
        .expect("infinite iterator")
}

fn source_entry_from_skill_dir(dir: &Path, global: bool) -> Result<SourceEntry, Error> {
    let raw = fs::read_to_string(dir.join("SKILL.md"))
        .map_err(|e| Error::internal_error().data(format!("Failed to read SKILL.md: {e}")))?;
    let (metadata, content): (crate::skills::SkillFrontmatter, String) = parse_frontmatter(&raw)
        .map_err(|e| Error::invalid_params().data(format!("Invalid skill frontmatter: {e}")))?
        .ok_or_else(|| Error::invalid_params().data("Skill file is missing frontmatter"))?;
    let name = metadata.name.unwrap_or_else(|| infer_skill_name(dir));
    Ok(skill_source_entry(
        &name,
        &metadata.description,
        &content,
        dir,
        global,
        metadata.metadata,
    ))
}

fn enclosed_archive_path(file: &zip::read::ZipFile<'_, Cursor<&[u8]>>) -> Result<PathBuf, Error> {
    file.enclosed_name()
        .ok_or_else(|| Error::invalid_params().data("Invalid path in source archive"))
}

fn find_imported_skill_root(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
) -> Result<PathBuf, Error> {
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|e| {
            Error::invalid_params().data(format!("Invalid source archive entry: {e}"))
        })?;
        let path = enclosed_archive_path(&file)?;
        if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
            return Ok(path.parent().unwrap_or_else(|| Path::new("")).to_path_buf());
        }
    }
    Err(Error::invalid_params().data("Source archive must contain a SKILL.md file"))
}

fn read_archive_text(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    path: &Path,
) -> Result<String, Error> {
    let mut file = archive
        .by_name(&archive_path(path))
        .map_err(|e| Error::invalid_params().data(format!("Invalid source archive entry: {e}")))?;
    let mut content = String::new();
    std::io::Read::read_to_string(&mut file, &mut content)
        .map_err(|e| Error::invalid_params().data(format!("Source file must be UTF-8: {e}")))?;
    Ok(content)
}

fn extract_archive_root(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    root: &Path,
    target: &Path,
) -> Result<(), Error> {
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|e| {
            Error::invalid_params().data(format!("Invalid source archive entry: {e}"))
        })?;
        let path = enclosed_archive_path(&file)?;
        let relative = if root.as_os_str().is_empty() {
            path.as_path()
        } else {
            match path.strip_prefix(root) {
                Ok(path) => path,
                Err(_) => continue,
            }
        };
        if relative.as_os_str().is_empty() {
            continue;
        }

        let target_path = target.join(relative);
        if file.is_dir() {
            fs::create_dir_all(&target_path).map_err(|e| {
                Error::internal_error().data(format!("Failed to extract source archive: {e}"))
            })?;
        } else {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    Error::internal_error().data(format!("Failed to extract source archive: {e}"))
                })?;
            }
            let mut out = fs::File::create(&target_path).map_err(|e| {
                Error::internal_error().data(format!("Failed to extract source archive: {e}"))
            })?;
            std::io::copy(&mut file, &mut out).map_err(|e| {
                Error::internal_error().data(format!("Failed to extract source archive: {e}"))
            })?;
        }
    }
    Ok(())
}

fn import_skill_tree(
    bytes: &[u8],
    global: bool,
    project_dir: Option<&str>,
) -> Result<Vec<SourceEntry>, Error> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| Error::invalid_params().data(format!("Invalid source archive: {e}")))?;
    let root = find_imported_skill_root(&mut archive)?;
    let raw = read_archive_text(&mut archive, &root.join("SKILL.md"))?;
    let (metadata, content): (crate::skills::SkillFrontmatter, String) = parse_frontmatter(&raw)
        .map_err(|e| Error::invalid_params().data(format!("Invalid skill frontmatter: {e}")))?
        .ok_or_else(|| Error::invalid_params().data("Skill file is missing frontmatter"))?;
    let name = metadata
        .name
        .ok_or_else(|| Error::invalid_params().data("Skill file is missing a name"))?;
    validate_skill_name(&name)?;

    let base = skill_base_dir(global, project_dir)?;
    let final_name = next_available_name(&name, |candidate| base.join(candidate).exists());
    let target = base.join(&final_name);
    extract_archive_root(&mut archive, &root, &target)?;

    if final_name != name {
        fs::write(
            target.join("SKILL.md"),
            build_skill_md(
                &final_name,
                &metadata.description,
                &content,
                &metadata.metadata,
            ),
        )
        .map_err(|e| Error::internal_error().data(format!("Failed to write SKILL.md: {e}")))?;
    }

    source_entry_from_skill_dir(&target, global).map(|source| vec![source])
}

fn import_skill_file(
    bytes: &[u8],
    global: bool,
    project_dir: Option<&str>,
) -> Result<Vec<SourceEntry>, Error> {
    let raw = String::from_utf8(bytes.to_vec())
        .map_err(|e| Error::invalid_params().data(format!("Source file must be UTF-8: {e}")))?;
    let (metadata, content): (crate::skills::SkillFrontmatter, String) = parse_frontmatter(&raw)
        .map_err(|e| Error::invalid_params().data(format!("Invalid skill frontmatter: {e}")))?
        .ok_or_else(|| Error::invalid_params().data("Skill file is missing frontmatter"))?;
    let name = metadata
        .name
        .ok_or_else(|| Error::invalid_params().data("Skill file is missing a name"))?;
    validate_skill_name(&name)?;
    let base = skill_base_dir(global, project_dir)?;
    let final_name = next_available_name(&name, |candidate| base.join(candidate).exists());
    create_source(
        SourceType::Skill,
        &final_name,
        &metadata.description,
        &content,
        global,
        project_dir,
        metadata.metadata,
    )
    .map(|entry| vec![entry])
}

fn import_project_file(bytes: &[u8], filename: &str) -> Result<Vec<SourceEntry>, Error> {
    let raw = String::from_utf8(bytes.to_vec())
        .map_err(|e| Error::invalid_params().data(format!("Source file must be UTF-8: {e}")))?;
    let lower = filename.to_lowercase();
    let filename_stem = if lower.ends_with(".project.md") {
        &filename[..filename.len() - ".project.md".len()]
    } else if lower.ends_with(".md") {
        &filename[..filename.len() - ".md".len()]
    } else {
        filename
    };
    let (title, description, content, mut properties) = parse_project_frontmatter(&raw);
    let mut slug = filename_stem.to_string();
    if validate_project_slug(&slug).is_err() && !title.is_empty() {
        slug = slugify_agent_name(&title);
    }
    validate_project_slug(&slug)?;

    let final_slug = next_available_name(&slug, |candidate| project_file_path(candidate).exists());
    if !title.is_empty() {
        properties.insert("title".into(), serde_json::Value::String(title));
    }
    create_source(
        SourceType::Project,
        &final_slug,
        &description,
        &content,
        true,
        None,
        properties,
    )
    .map(|entry| vec![entry])
}

fn import_agent_file(
    bytes: &[u8],
    global: bool,
    project_dir: Option<&str>,
) -> Result<Vec<SourceEntry>, Error> {
    let raw = String::from_utf8(bytes.to_vec())
        .map_err(|e| Error::invalid_params().data(format!("Source file must be UTF-8: {e}")))?;
    let (frontmatter, content) = parse_agent_frontmatter(&raw)?;
    create_agent_source(
        &frontmatter.name,
        &frontmatter.description,
        &content,
        frontmatter.properties,
        global,
        project_dir,
    )
    .map(|entry| vec![entry])
}

// --- Public CRUD ---

pub fn create_source(
    source_type: SourceType,
    name: &str,
    description: &str,
    content: &str,
    global: bool,
    project_dir: Option<&str>,
    properties: HashMap<String, serde_json::Value>,
) -> Result<SourceEntry, Error> {
    require_mutable_type(source_type)?;
    if source_type == SourceType::Agent {
        return create_agent_source(name, description, content, properties, global, project_dir);
    }

    match source_type {
        SourceType::Skill => {
            validate_skill_name(name)?;
            let dir = skill_base_dir(global, project_dir)?.join(name);

            if dir.exists() {
                return Err(Error::invalid_params()
                    .data(format!("A source named \"{}\" already exists", name)));
            }

            fs::create_dir_all(&dir).map_err(|e| {
                Error::internal_error().data(format!("Failed to create source directory: {e}"))
            })?;
            let file_path = dir.join("SKILL.md");
            let md = build_skill_md(name, description, content, &properties);
            fs::write(&file_path, md).map_err(|e| {
                Error::internal_error().data(format!("Failed to write SKILL.md: {e}"))
            })?;

            Ok(skill_source_entry(
                name,
                description,
                content,
                &dir,
                global,
                properties,
            ))
        }
        SourceType::Project => {
            validate_project_slug(name)?;
            let base = projects_dir();
            fs::create_dir_all(&base).map_err(|e| {
                Error::internal_error().data(format!("Failed to create projects dir: {e}"))
            })?;
            let file = project_file_path(name);
            if file.exists() {
                return Err(Error::invalid_params()
                    .data(format!("A source named \"{}\" already exists", name)));
            }
            // The display name comes from `properties.title`; if absent, the
            // file's frontmatter `name:` is the slug itself.
            let display_name = properties
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or(name);
            let md = build_source_markdown(display_name, description, content, &properties)?;
            fs::write(&file, md).map_err(|e| {
                Error::internal_error().data(format!("Failed to write project file: {e}"))
            })?;
            project_entry_from_file(&file)
                .ok_or_else(|| Error::internal_error().data("Failed to read newly created project"))
        }
        _ => unreachable!("guarded by require_mutable_type"),
    }
}

pub fn update_source(
    source_type: SourceType,
    path: &str,
    name: &str,
    description: &str,
    content: &str,
    properties: Option<HashMap<String, serde_json::Value>>,
) -> Result<SourceEntry, Error> {
    update_source_with_roots(
        source_type,
        path,
        name,
        description,
        content,
        UpdateSourceOptions {
            properties,
            additional_roots: &[],
        },
    )
}

pub struct UpdateSourceOptions<'a> {
    pub properties: Option<HashMap<String, serde_json::Value>>,
    pub additional_roots: &'a [SourceRoot],
}

pub fn update_source_with_roots(
    source_type: SourceType,
    path: &str,
    name: &str,
    description: &str,
    content: &str,
    options: UpdateSourceOptions<'_>,
) -> Result<SourceEntry, Error> {
    require_mutable_type(source_type)?;
    if source_type == SourceType::Agent {
        return update_agent_source(
            path,
            name,
            description,
            content,
            options.properties,
            options.additional_roots,
        );
    }

    match source_type {
        SourceType::Skill => {
            validate_skill_name(name)?;

            let dir = resolve_discoverable_skill_dir(path)?;
            let current_dir_name = dir
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    Error::internal_error().data("Failed to resolve source directory name")
                })?;

            let resolved_properties = match options.properties {
                Some(p) => p,
                None => read_existing_skill_properties(&dir),
            };

            let target_dir = if name == current_dir_name {
                dir.clone()
            } else {
                let base_dir = dir.parent().ok_or_else(|| {
                    Error::internal_error().data("Failed to resolve source base directory")
                })?;
                let target_dir = base_dir.join(name);

                if target_dir.exists() {
                    return Err(Error::invalid_params()
                        .data(format!("A source named \"{}\" already exists", name)));
                }

                fs::rename(&dir, &target_dir).map_err(|e| {
                    Error::internal_error().data(format!("Failed to rename source directory: {e}"))
                })?;

                target_dir
            };

            let file_path = target_dir.join("SKILL.md");
            let md = build_skill_md(name, description, content, &resolved_properties);
            fs::write(&file_path, md).map_err(|e| {
                Error::internal_error().data(format!("Failed to write SKILL.md: {e}"))
            })?;

            Ok(skill_source_entry(
                name,
                description,
                content,
                &target_dir,
                is_global_skill_dir(&target_dir),
                resolved_properties,
            ))
        }
        SourceType::Project => {
            validate_project_slug(name)?;
            let file = resolve_project_path(path)?;

            let current_slug = file
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| Error::internal_error().data("Bad project filename"))?;
            if current_slug != name {
                return Err(Error::invalid_params().data(format!(
                    "Project slug cannot be changed (current: \"{}\", requested: \"{}\")",
                    current_slug, name
                )));
            }

            let resolved_properties = match options.properties {
                Some(p) => p,
                None => read_existing_project_properties(&file),
            };

            let display_name = resolved_properties
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or(name);
            let md =
                build_source_markdown(display_name, description, content, &resolved_properties)?;
            fs::write(&file, md).map_err(|e| {
                Error::internal_error().data(format!("Failed to write project file: {e}"))
            })?;
            project_entry_from_file(&file)
                .ok_or_else(|| Error::internal_error().data("Failed to read updated project"))
        }
        _ => unreachable!("guarded by require_mutable_type"),
    }
}

pub fn delete_source(source_type: SourceType, path: &str) -> Result<(), Error> {
    delete_source_with_roots(source_type, path, &[])
}

pub fn delete_source_with_roots(
    source_type: SourceType,
    path: &str,
    additional_roots: &[SourceRoot],
) -> Result<(), Error> {
    require_mutable_type(source_type)?;

    match source_type {
        SourceType::Skill => {
            let dir = resolve_skill_dir(path)?;
            fs::remove_dir_all(&dir).map_err(|e| {
                Error::internal_error().data(format!("Failed to delete source: {e}"))
            })?;
        }
        SourceType::Project => {
            let file = resolve_project_path(path)?;
            fs::remove_file(&file).map_err(|e| {
                Error::internal_error().data(format!("Failed to delete project: {e}"))
            })?;
        }
        SourceType::Agent => {
            let file_path = resolve_agent_file_with_roots(path, additional_roots)?;
            reject_read_only_agent_file(&file_path, additional_roots)?;
            fs::remove_file(&file_path).map_err(|e| {
                Error::internal_error().data(format!("Failed to delete source: {e}"))
            })?;
        }
        _ => unreachable!("guarded by require_mutable_type"),
    }
    Ok(())
}

pub fn list_sources(
    source_type: Option<SourceType>,
    project_dir: Option<&str>,
    include_project_sources: bool,
) -> Result<Vec<SourceEntry>, Error> {
    list_sources_with_roots(source_type, project_dir, include_project_sources, &[])
}

pub fn list_sources_with_roots(
    source_type: Option<SourceType>,
    project_dir: Option<&str>,
    include_project_sources: bool,
    additional_roots: &[SourceRoot],
) -> Result<Vec<SourceEntry>, Error> {
    if let Some(t) = source_type {
        require_listable_type(Some(t))?;
    }
    let kinds: Vec<SourceType> = match source_type {
        Some(t) => vec![t],
        None => vec![SourceType::Skill, SourceType::Project],
    };

    let mut sources = Vec::new();
    for kind in kinds {
        match kind {
            SourceType::Skill => {
                let working_dir = project_dir
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from);
                sources.extend(
                    discover_skills(working_dir.as_deref())
                        .into_iter()
                        .filter(|s| s.source_type == SourceType::Skill),
                );

                if include_project_sources {
                    let projects = read_project_dir()?;
                    let already_scanned = working_dir.as_deref();
                    for proj in &projects {
                        let dirs = proj
                            .properties
                            .get("workingDirs")
                            .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                            .unwrap_or_default();
                        let project_name = proj
                            .properties
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&proj.name);
                        for wd in &dirs {
                            let wd_path = PathBuf::from(wd);
                            if Some(wd_path.as_path()) == already_scanned {
                                continue;
                            }
                            for skill in discover_skills(Some(&wd_path)) {
                                if skill.source_type != SourceType::Skill || skill.global {
                                    continue;
                                }
                                let mut tagged = skill;
                                tagged.properties.insert(
                                    "projectName".into(),
                                    serde_json::Value::String(project_name.to_string()),
                                );
                                tagged.properties.insert(
                                    "projectDir".into(),
                                    serde_json::Value::String(wd.clone()),
                                );
                                sources.push(tagged);
                            }
                        }
                    }
                }
            }
            SourceType::BuiltinSkill => {
                let working_dir = project_dir
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from);
                sources.extend(
                    discover_skills(working_dir.as_deref())
                        .into_iter()
                        .filter(|s| s.source_type == SourceType::BuiltinSkill)
                        .map(builtin_skill_entry),
                );
            }
            SourceType::Project => {
                sources.extend(read_project_dir()?);
            }
            SourceType::Agent => {
                sources.extend(list_agent_sources(project_dir, additional_roots));
            }
            SourceType::Recipe | SourceType::Subrecipe => {
                return Err(Error::invalid_params()
                    .data(format!("Source type '{}' listing is not supported.", kind)));
            }
        }
    }

    sources.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sources)
}

pub fn export_source(source_type: SourceType, path: &str) -> Result<SourceExport, Error> {
    export_source_with_roots(source_type, path, &[])
}

pub fn export_source_with_roots(
    source_type: SourceType,
    path: &str,
    additional_roots: &[SourceRoot],
) -> Result<SourceExport, Error> {
    match source_type {
        SourceType::Skill => {
            let dir = resolve_discoverable_skill_dir(path)?;
            let name = path_component_string(&dir, "skill");
            let bytes = zip_source_tree(&dir, &name)?;
            Ok(SourceExport {
                bytes,
                filename: format!("{name}.skill.zip"),
                mime_type: ZIP_MIME,
            })
        }
        SourceType::Agent => {
            let file_path = resolve_agent_file_with_roots(path, additional_roots)?;
            let read_only = is_read_only_agent_file(&file_path, additional_roots);
            let source = agent_source_entry(
                &file_path,
                is_global_agent_file(&file_path) || read_only,
                !read_only,
            )?;
            let bytes = fs::read(&file_path).map_err(|e| {
                Error::internal_error().data(format!("Failed to read agent file: {e}"))
            })?;
            Ok(SourceExport {
                bytes,
                filename: format!("{}.agent.md", slugify_agent_name(&source.name)),
                mime_type: MARKDOWN_MIME,
            })
        }
        SourceType::Project => {
            let file = resolve_project_path(path)?;
            let filename = path_component_string(&file, "project.md");
            let bytes = fs::read(&file).map_err(|e| {
                Error::internal_error().data(format!("Failed to read project file: {e}"))
            })?;
            Ok(SourceExport {
                bytes,
                filename,
                mime_type: MARKDOWN_MIME,
            })
        }
        _ => Err(Error::invalid_params().data(format!(
            "Source type '{}' export is not supported.",
            source_type
        ))),
    }
}

pub fn import_sources(
    bytes: &[u8],
    filename: &str,
    source_type: Option<SourceType>,
    global: bool,
    project_dir: Option<&str>,
) -> Result<Vec<SourceEntry>, Error> {
    let filename = safe_import_filename(filename)?;
    let source_type = infer_import_source_type(filename, source_type)?;
    let lower = filename.to_lowercase();

    match source_type {
        SourceType::Skill if lower.ends_with(".zip") => {
            import_skill_tree(bytes, global, project_dir)
        }
        SourceType::Skill => import_skill_file(bytes, global, project_dir),
        SourceType::Project => import_project_file(bytes, filename),
        SourceType::Agent => import_agent_file(bytes, global, project_dir),
        _ => Err(Error::invalid_params().data(format!(
            "Source type '{}' import is not supported.",
            source_type
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn skill_name_validation() {
        assert!(validate_skill_name("my-skill").is_ok());
        assert!(validate_skill_name("abc123").is_ok());
        assert!(validate_skill_name("double--hyphen").is_ok());
        assert!(validate_skill_name("").is_err());
        assert!(validate_skill_name("-leading").is_err());
        assert!(validate_skill_name("trailing-").is_err());
        assert!(validate_skill_name("CAPS").is_err());
        assert!(validate_skill_name("../escape").is_err());
        assert!(validate_skill_name(&"a".repeat(64)).is_ok());
        assert!(validate_skill_name(&"a".repeat(65)).is_err());
    }

    #[test]
    fn lists_additional_read_only_agent_roots() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("builtin").join("agents");
        std::fs::create_dir_all(&root).unwrap();
        let agent_path = root.join("solo.md");
        std::fs::write(
            &agent_path,
            "---\nname: Solo\ndescription: Built in\n---\n\nYou are Solo.",
        )
        .unwrap();

        let sources = list_sources_with_roots(
            Some(SourceType::Agent),
            None,
            false,
            &[SourceRoot::read_only(root.clone())],
        )
        .unwrap();

        let solo = sources.iter().find(|source| source.name == "Solo").unwrap();
        assert!(!solo.writable);
        assert!(solo.global);
        assert_eq!(solo.path, agent_path.to_string_lossy());

        let err = update_source_with_roots(
            SourceType::Agent,
            &solo.path,
            "Solo",
            "Built in",
            "Updated",
            UpdateSourceOptions {
                properties: None,
                additional_roots: &[SourceRoot::read_only(root.canonicalize().unwrap())],
            },
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("read-only"));

        let err = delete_source_with_roots(
            SourceType::Agent,
            &solo.path,
            &[SourceRoot::read_only(root.canonicalize().unwrap())],
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("read-only"));
    }

    #[test]
    fn create_list_update_delete_project_skill() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        let created = create_source(
            SourceType::Skill,
            "my-skill",
            "does the thing",
            "step one\nstep two",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap();
        assert_eq!(created.name, "my-skill");
        assert!(!created.global);
        let dir = PathBuf::from(&created.path);
        assert!(dir.join("SKILL.md").exists());

        let listed = list_sources(Some(SourceType::Skill), Some(project), false).unwrap();
        assert!(listed.iter().any(|s| s.name == "my-skill" && !s.global));

        let updated = update_source(
            SourceType::Skill,
            created.path.as_str(),
            "my-skill",
            "now does a different thing",
            "step three",
            Some(HashMap::new()),
        )
        .unwrap();
        assert_eq!(updated.description, "now does a different thing");
        assert_eq!(updated.name, "my-skill");

        delete_source(SourceType::Skill, created.path.as_str()).unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn create_rejects_duplicate_name() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        create_source(
            SourceType::Skill,
            "dup",
            "d",
            "c",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap();
        let err = create_source(
            SourceType::Skill,
            "dup",
            "d",
            "c",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("already exists"));
    }

    #[test]
    fn project_scope_requires_project_dir() {
        let err = create_source(
            SourceType::Skill,
            "x",
            "d",
            "c",
            false,
            None,
            HashMap::new(),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("projectDir"));
    }

    #[test]
    fn export_then_import_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let project_a = tmp.path().join("a");
        let project_b = tmp.path().join("b");
        std::fs::create_dir_all(&project_a).unwrap();
        std::fs::create_dir_all(&project_b).unwrap();

        create_source(
            SourceType::Skill,
            "portable",
            "describes itself",
            "body goes here",
            false,
            Some(project_a.to_str().unwrap()),
            HashMap::new(),
        )
        .unwrap();

        let portable_dir = project_a.join(".agents").join("skills").join("portable");
        std::fs::write(portable_dir.join("notes.txt"), "supporting context").unwrap();

        let export = export_source(SourceType::Skill, portable_dir.to_str().unwrap()).unwrap();
        assert_eq!(export.filename, "portable.skill.zip");
        assert_eq!(export.mime_type, "application/zip");

        let imported = import_sources(
            &export.bytes,
            &export.filename,
            None,
            false,
            Some(project_b.to_str().unwrap()),
        )
        .unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "portable");
        assert_eq!(imported[0].description, "describes itself");
        assert_eq!(imported[0].content, "body goes here");
        assert!(project_b
            .join(".agents")
            .join("skills")
            .join("portable")
            .join("notes.txt")
            .exists());
    }

    #[test]
    fn export_allows_discovered_read_only_skill() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path();
        let claude_skill_dir = project.join(".claude").join("skills").join("portable");
        std::fs::create_dir_all(&claude_skill_dir).unwrap();
        std::fs::write(
            claude_skill_dir.join("SKILL.md"),
            build_skill_md(
                "portable",
                "describes itself",
                "body goes here",
                &HashMap::new(),
            ),
        )
        .unwrap();

        let listed = list_sources(
            Some(SourceType::Skill),
            Some(project.to_str().unwrap()),
            false,
        )
        .unwrap();
        let exported_skill = listed
            .iter()
            .find(|skill| skill.name == "portable")
            .expect("expected listed skill");

        let export = export_source(SourceType::Skill, exported_skill.path.as_str()).unwrap();
        assert_eq!(export.filename, "portable.skill.zip");
        assert_eq!(export.mime_type, "application/zip");
    }

    #[test]
    fn update_allows_discovered_read_only_skill() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path();
        let claude_skill_dir = project.join(".claude").join("skills").join("portable");
        std::fs::create_dir_all(&claude_skill_dir).unwrap();
        std::fs::write(
            claude_skill_dir.join("SKILL.md"),
            build_skill_md(
                "portable",
                "describes itself",
                "body goes here",
                &HashMap::new(),
            ),
        )
        .unwrap();

        let updated = update_source(
            SourceType::Skill,
            claude_skill_dir.to_str().unwrap(),
            "portable",
            "updated description",
            "updated body",
            Some(HashMap::new()),
        )
        .unwrap();

        assert_eq!(updated.name, "portable");
        assert_eq!(updated.description, "updated description");
        assert_eq!(updated.content, "updated body");

        let raw = std::fs::read_to_string(claude_skill_dir.join("SKILL.md")).unwrap();
        assert!(raw.contains("description: 'updated description'"));
        assert!(raw.contains("updated body"));
    }

    #[test]
    fn import_collision_uses_shared_suffix_logic() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        create_source(
            SourceType::Skill,
            "busy",
            "d",
            "c",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap();

        let busy_dir = tmp.path().join(".agents").join("skills").join("busy");
        let exported = export_source(SourceType::Skill, busy_dir.to_str().unwrap()).unwrap();
        let imported = import_sources(
            &exported.bytes,
            &exported.filename,
            None,
            false,
            Some(project),
        )
        .unwrap();
        assert_eq!(imported[0].name, "busy-2");
    }

    #[test]
    fn update_rejects_nonexistent_source() {
        let tmp = TempDir::new().unwrap();
        let missing_dir = tmp
            .path()
            .join(".goose")
            .join("skills")
            .join("no-such-skill");
        let err = update_source(
            SourceType::Skill,
            missing_dir.to_str().unwrap(),
            "no-such-skill",
            "d",
            "c",
            Some(HashMap::new()),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("not found"));
    }

    #[test]
    fn delete_rejects_nonexistent_source() {
        let tmp = TempDir::new().unwrap();
        let missing_dir = tmp
            .path()
            .join(".goose")
            .join("skills")
            .join("no-such-skill");
        let err = delete_source(SourceType::Skill, missing_dir.to_str().unwrap()).unwrap_err();
        assert!(format!("{:?}", err).contains("not found"));
    }

    #[test]
    fn list_sources_lists_builtin_skills() {
        let listed = list_sources(Some(SourceType::BuiltinSkill), None, false).unwrap();
        let builtin = listed
            .iter()
            .find(|source| source.name == "goose-doc-guide")
            .expect("expected goose-doc-guide builtin skill");

        assert_eq!(builtin.source_type, SourceType::BuiltinSkill);
        assert!(builtin.global);
        assert_eq!(builtin.path, "builtin://skills/goose-doc-guide");
        assert!(builtin.supporting_files.is_empty());
        assert!(!builtin.content.is_empty());
    }

    #[test]
    fn list_skill_excludes_builtin_skills() {
        let listed = list_sources(Some(SourceType::Skill), None, false).unwrap();
        assert!(!listed
            .iter()
            .any(|source| source.source_type == SourceType::BuiltinSkill));
    }

    #[test]
    fn filesystem_skill_suppresses_same_named_builtin() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path();
        let skill_dir = project
            .join(".agents")
            .join("skills")
            .join("goose-doc-guide");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            build_skill_md(
                "goose-doc-guide",
                "project override",
                "Use project docs",
                &HashMap::new(),
            ),
        )
        .unwrap();

        let builtins = list_sources(
            Some(SourceType::BuiltinSkill),
            Some(project.to_str().unwrap()),
            false,
        )
        .unwrap();
        assert!(!builtins
            .iter()
            .any(|source| source.name == "goose-doc-guide"));

        let skills = list_sources(
            Some(SourceType::Skill),
            Some(project.to_str().unwrap()),
            false,
        )
        .unwrap();
        let project_skill = skills
            .iter()
            .find(|source| source.name == "goose-doc-guide")
            .expect("expected project skill");
        assert_eq!(project_skill.source_type, SourceType::Skill);
        assert_eq!(project_skill.description, "project override");
    }

    #[test]
    fn rejects_unsupported_source_type_for_mutation() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        let err = create_source(
            SourceType::BuiltinSkill,
            "x",
            "d",
            "c",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = update_source(
            SourceType::BuiltinSkill,
            "builtin://skills/x",
            "x",
            "d",
            "c",
            Some(HashMap::new()),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = update_source(SourceType::Recipe, "x", "x", "d", "c", Some(HashMap::new()))
            .unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = delete_source(SourceType::BuiltinSkill, "builtin://skills/x").unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = delete_source(SourceType::Subrecipe, "x").unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let listed = list_sources(Some(SourceType::BuiltinSkill), Some(project), false).unwrap();
        assert!(listed
            .iter()
            .any(|source| source.source_type == SourceType::BuiltinSkill));

        let err = list_sources(Some(SourceType::Recipe), Some(project), false).unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = export_source(SourceType::BuiltinSkill, "builtin://skills/x").unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = export_source(SourceType::Recipe, "x").unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let payload = build_skill_md("x", "d", "c", &HashMap::new()).into_bytes();
        let err = import_sources(
            &payload,
            "x.skill.md",
            Some(SourceType::BuiltinSkill),
            false,
            Some(project),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));
    }

    #[test]
    fn update_derives_name_from_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        create_source(
            SourceType::Skill,
            "my-dir",
            "orig",
            "body",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap();

        let skill_dir = tmp.path().join(".agents").join("skills").join("my-dir");
        let updated = update_source(
            SourceType::Skill,
            skill_dir.to_str().unwrap(),
            "my-dir",
            "new description",
            "new body",
            Some(HashMap::new()),
        )
        .unwrap();
        // Name is derived from the frontmatter written by create_source
        assert_eq!(updated.name, "my-dir");
    }

    #[test]
    fn list_sources_reads_project_agents_skills() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join(".agents").join("skills").join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            build_skill_md("test-skill", "from agents", "Body", &HashMap::new()),
        )
        .unwrap();

        let listed = list_sources(
            Some(SourceType::Skill),
            Some(tmp.path().to_str().unwrap()),
            false,
        )
        .unwrap();
        let skill = listed
            .iter()
            .find(|source| source.name == "test-skill" && !source.global)
            .unwrap();
        assert!(skill.path.contains(".agents/skills"));
        assert_eq!(skill.description, "from agents");
    }

    #[test]
    fn project_sources_prefer_agents_directory_over_legacy_goose() {
        let tmp = TempDir::new().unwrap();
        let agents_skill_dir = tmp
            .path()
            .join(".agents")
            .join("skills")
            .join("shared-skill");
        let legacy_skill_dir = tmp
            .path()
            .join(".goose")
            .join("skills")
            .join("shared-skill");
        std::fs::create_dir_all(&agents_skill_dir).unwrap();
        std::fs::create_dir_all(&legacy_skill_dir).unwrap();
        std::fs::write(
            agents_skill_dir.join("SKILL.md"),
            build_skill_md("shared-skill", "preferred", "Agents", &HashMap::new()),
        )
        .unwrap();
        std::fs::write(
            legacy_skill_dir.join("SKILL.md"),
            build_skill_md("shared-skill", "legacy", "Goose", &HashMap::new()),
        )
        .unwrap();

        let listed = list_sources(
            Some(SourceType::Skill),
            Some(tmp.path().to_str().unwrap()),
            false,
        )
        .unwrap();
        let matching: Vec<_> = listed
            .iter()
            .filter(|source| source.name == "shared-skill" && !source.global)
            .collect();
        assert_eq!(matching.len(), 1);
        assert!(matching[0].path.contains(".agents/skills"));
        assert_eq!(matching[0].description, "preferred");

        let exported = export_source(SourceType::Skill, matching[0].path.as_str()).unwrap();
        assert_eq!(exported.filename, "shared-skill.skill.zip");
    }

    #[test]
    fn update_rejects_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path();
        let escaped_dir = project.join(".goose").join("escaped");
        std::fs::create_dir_all(&escaped_dir).unwrap();
        std::fs::write(
            escaped_dir.join("SKILL.md"),
            "---\nname: escaped\ndescription: escaped\n---\ncontent",
        )
        .unwrap();

        let attempted_escape = project.join(".goose").join("escaped");
        let err = update_source(
            SourceType::Skill,
            attempted_escape.to_str().unwrap(),
            "escaped",
            "new description",
            "new content",
            Some(HashMap::new()),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("not found"));
    }
}
