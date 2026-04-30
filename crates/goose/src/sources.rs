//! Filesystem-backed CRUD for [`SourceEntry`] values exchanged over ACP custom
//! methods. Skills live in `~/.agents/skills/` (or per-project under
//! `<project>/.agents/skills/`). Projects live in `<dataDir>/projects/<slug>.md`.

use crate::config::paths::Paths;
use crate::skills::{
    build_skill_md, discover_skills, infer_skill_name, is_global_skill_dir,
    parse_skill_frontmatter, resolve_discoverable_skill_dir, resolve_skill_dir, skill_base_dir,
    validate_skill_name,
};
use fs_err as fs;
use goose_sdk::custom_requests::{SourceEntry, SourceType};
use sacp::Error;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
        SourceType::Skill | SourceType::Project => Ok(()),
        other => Err(Error::invalid_params().data(format!(
            "Source type '{other}' is not supported for mutation."
        ))),
    }
}

// --- Project helpers ---

#[derive(Deserialize)]
struct ProjectFront {
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

fn build_project_md(
    name: &str,
    description: &str,
    content: &str,
    properties: &HashMap<String, serde_json::Value>,
) -> String {
    let mut fm = serde_yaml::Mapping::new();
    fm.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(name.into()),
    );
    fm.insert(
        serde_yaml::Value::String("description".into()),
        serde_yaml::Value::String(description.into()),
    );
    for (k, v) in properties {
        if k == "name" || k == "description" {
            continue;
        }
        if let Ok(yv) = serde_yaml::to_value(v) {
            fm.insert(serde_yaml::Value::String(k.clone()), yv);
        }
    }
    let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
    let mut md = format!("---\n{yaml}---\n");
    if !content.is_empty() {
        md.push('\n');
        md.push_str(content);
        md.push('\n');
    }
    md
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
    match parse_frontmatter::<ProjectFront>(raw) {
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
        return Err(
            Error::invalid_params().data(format!("Project source \"{}\" not found", path))
        );
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
) -> SourceEntry {
    SourceEntry {
        source_type: SourceType::Skill,
        name: name.to_string(),
        description: description.to_string(),
        content: content.to_string(),
        path: dir.to_string_lossy().to_string(),
        global,
        supporting_files: Vec::new(),
        properties: HashMap::new(),
    }
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
            let md = build_skill_md(name, description, content);
            fs::write(&file_path, md).map_err(|e| {
                Error::internal_error().data(format!("Failed to write SKILL.md: {e}"))
            })?;

            Ok(skill_source_entry(name, description, content, &dir, global))
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
            let md = build_project_md(display_name, description, content, &properties);
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
    properties: HashMap<String, serde_json::Value>,
) -> Result<SourceEntry, Error> {
    require_mutable_type(source_type)?;

    match source_type {
        SourceType::Skill => {
            validate_skill_name(name)?;

            let dir = resolve_discoverable_skill_dir(path)?;
            let current_dir_name = dir.file_name().and_then(|value| value.to_str()).ok_or_else(
                || Error::internal_error().data("Failed to resolve source directory name"),
            )?;

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
                    Error::internal_error()
                        .data(format!("Failed to rename source directory: {e}"))
                })?;

                target_dir
            };

            let file_path = target_dir.join("SKILL.md");
            let md = build_skill_md(name, description, content);
            fs::write(&file_path, md).map_err(|e| {
                Error::internal_error().data(format!("Failed to write SKILL.md: {e}"))
            })?;

            // Skills don't carry user-defined properties yet; ignore the
            // incoming bag rather than silently dropping it elsewhere.
            let _ = properties;

            Ok(skill_source_entry(
                name,
                description,
                content,
                &target_dir,
                is_global_skill_dir(&target_dir),
            ))
        }
        SourceType::Project => {
            validate_project_slug(name)?;
            let file = resolve_project_path(path)?;

            // We don't currently support renaming a project (it would change
            // the slug used as the stable thread.project_id). Reject mismatches
            // to surface this clearly.
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

            let display_name = properties
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or(name);
            let md = build_project_md(display_name, description, content, &properties);
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
        _ => unreachable!("guarded by require_mutable_type"),
    }
    Ok(())
}

pub fn list_sources(
    source_type: Option<SourceType>,
    project_dir: Option<&str>,
    include_project_sources: bool,
) -> Result<Vec<SourceEntry>, Error> {
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
                        .filter(|s| s.source_type == SourceType::BuiltinSkill),
                );
            }
            SourceType::Project => {
                sources.extend(read_project_dir()?);
            }
            SourceType::Recipe | SourceType::Subrecipe | SourceType::Agent => {
                return Err(Error::invalid_params().data(format!(
                    "Source type '{}' listing is not supported.",
                    kind
                )));
            }
        }
    }

    sources.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sources)
}

pub fn export_source(source_type: SourceType, path: &str) -> Result<(String, String), Error> {
    match source_type {
        SourceType::Skill => {
            let dir = resolve_discoverable_skill_dir(path)?;

            let md = dir.join("SKILL.md");
            let raw = fs::read_to_string(&md).map_err(|e| {
                Error::internal_error().data(format!("Failed to read SKILL.md: {e}"))
            })?;
            let (description, content) = parse_skill_frontmatter(&raw);

            let name = infer_skill_name(&dir);

            let export = serde_json::json!({
                "version": 1,
                "type": "skill",
                "name": name,
                "description": description,
                "content": content,
            });
            let json = serde_json::to_string_pretty(&export).map_err(|e| {
                Error::internal_error().data(format!("Failed to serialize source: {e}"))
            })?;
            let filename = format!("{}.skill.json", name);
            Ok((json, filename))
        }
        SourceType::Project => {
            let file = resolve_project_path(path)?;
            let raw = fs::read_to_string(&file).map_err(|e| {
                Error::internal_error().data(format!("Failed to read project file: {e}"))
            })?;
            let (title, description, content, properties) = parse_project_frontmatter(&raw);
            let slug = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let display_name = if title.is_empty() {
                slug.clone()
            } else {
                title
            };

            let mut export = serde_json::json!({
                "version": 1,
                "type": "project",
                "name": slug,
                "title": display_name,
                "description": description,
                "content": content,
            });
            if !properties.is_empty() {
                export["properties"] = serde_json::to_value(&properties).unwrap_or_default();
            }
            let json = serde_json::to_string_pretty(&export).map_err(|e| {
                Error::internal_error().data(format!("Failed to serialize project: {e}"))
            })?;
            let filename = format!("{}.project.json", slug);
            Ok((json, filename))
        }
        _ => Err(Error::invalid_params().data(format!(
            "Source type '{}' export is not supported.",
            source_type
        ))),
    }
}

pub fn import_sources(
    data: &str,
    global: bool,
    project_dir: Option<&str>,
) -> Result<Vec<SourceEntry>, Error> {
    let value: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| Error::invalid_params().data(format!("Invalid JSON: {e}")))?;

    let version = value
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| Error::invalid_params().data("Missing or invalid \"version\" field"))?;
    if version != 1 {
        return Err(
            Error::invalid_params().data(format!("Unsupported source export version: {}", version))
        );
    }

    let type_str = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("skill");
    let source_type = match type_str {
        "skill" => SourceType::Skill,
        "project" => SourceType::Project,
        other => {
            return Err(Error::invalid_params().data(format!(
                "Source type '{}' import is not supported.",
                other
            )));
        }
    };

    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::invalid_params().data("Missing or invalid \"name\" field"))?
        .to_string();
    if name.is_empty() {
        return Err(Error::invalid_params().data("Source name must not be empty"));
    }

    // Skills require a description; projects can omit it.
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if source_type == SourceType::Skill && description.is_empty() {
        return Err(Error::invalid_params().data("Source description must not be empty"));
    }

    let content = value
        .get("content")
        .or_else(|| value.get("instructions"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut properties: HashMap<String, serde_json::Value> = value
        .get("properties")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    // The export's top-level "title" wins over a properties.title if both
    // exist.
    if source_type == SourceType::Project {
        if let Some(title) = value.get("title").and_then(|v| v.as_str()) {
            if !title.is_empty() {
                properties.insert("title".into(), serde_json::Value::String(title.into()));
            }
        }
    }

    match source_type {
        SourceType::Skill => {
            validate_skill_name(&name)?;
            let base = skill_base_dir(global, project_dir)?;
            let mut final_name = name.clone();
            if base.join(&final_name).exists() {
                final_name = format!("{}-imported", name);
                let mut counter = 2u32;
                while base.join(&final_name).exists() {
                    final_name = format!("{}-imported-{}", name, counter);
                    counter += 1;
                }
            }
            create_source(
                SourceType::Skill,
                &final_name,
                &description,
                &content,
                global,
                project_dir,
                HashMap::new(),
            )
            .map(|entry| vec![entry])
        }
        SourceType::Project => {
            validate_project_slug(&name)?;
            let mut final_name = name.clone();
            if project_file_path(&final_name).exists() {
                final_name = format!("{}-imported", name);
                let mut counter = 2u32;
                while project_file_path(&final_name).exists() {
                    final_name = format!("{}-imported-{}", name, counter);
                    counter += 1;
                }
            }
            create_source(
                SourceType::Project,
                &final_name,
                &description,
                &content,
                true, // projects are always global
                None,
                properties,
            )
            .map(|entry| vec![entry])
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Tests that set GOOSE_PATH_ROOT must run serially because it's a global
    // env var.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_root(f: impl FnOnce(&Path)) {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        unsafe { std::env::set_var("GOOSE_PATH_ROOT", tmp.path()) };
        f(tmp.path());
        unsafe { std::env::remove_var("GOOSE_PATH_ROOT") };
    }

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
            HashMap::new(),
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
        let (json, filename) =
            export_source(SourceType::Skill, portable_dir.to_str().unwrap()).unwrap();
        assert_eq!(filename, "portable.skill.json");

        let imported = import_sources(&json, false, Some(project_b.to_str().unwrap())).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "portable");
        assert_eq!(imported[0].description, "describes itself");
        assert_eq!(imported[0].content, "body goes here");
    }

    #[test]
    fn export_allows_discovered_read_only_skill() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path();
        let claude_skill_dir = project.join(".claude").join("skills").join("portable");
        std::fs::create_dir_all(&claude_skill_dir).unwrap();
        std::fs::write(
            claude_skill_dir.join("SKILL.md"),
            build_skill_md("portable", "describes itself", "body goes here"),
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

        let (json, filename) = export_source(SourceType::Skill, exported_skill.path.as_str()).unwrap();
        assert_eq!(filename, "portable.skill.json");
        assert!(json.contains("\"name\": \"portable\""));
    }

    #[test]
    fn update_allows_discovered_read_only_skill() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path();
        let claude_skill_dir = project.join(".claude").join("skills").join("portable");
        std::fs::create_dir_all(&claude_skill_dir).unwrap();
        std::fs::write(
            claude_skill_dir.join("SKILL.md"),
            build_skill_md("portable", "describes itself", "body goes here"),
        )
        .unwrap();

        let updated = update_source(
            SourceType::Skill,
            claude_skill_dir.to_str().unwrap(),
            "portable",
            "updated description",
            "updated body",
            HashMap::new(),
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
    fn import_collision_appends_suffix() {
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

        let payload = serde_json::json!({
            "version": 1,
            "type": "skill",
            "name": "busy",
            "description": "d",
            "content": "c",
        })
        .to_string();
        let imported = import_sources(&payload, false, Some(project)).unwrap();
        assert_eq!(imported[0].name, "busy-imported");
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
            HashMap::new(),
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

        let err =
            update_source(SourceType::Recipe, "x", "x", "d", "c", HashMap::new()).unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = delete_source(SourceType::Subrecipe, "x").unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = export_source(SourceType::Recipe, "x").unwrap_err();
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
            HashMap::new(),
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
            build_skill_md("test-skill", "from agents", "Body"),
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
            build_skill_md("shared-skill", "preferred", "Agents"),
        )
        .unwrap();
        std::fs::write(
            legacy_skill_dir.join("SKILL.md"),
            build_skill_md("shared-skill", "legacy", "Goose"),
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
        assert!(exported.0.contains("preferred"));
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
            HashMap::new(),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("not found"));
    }

    #[test]
    fn project_create_read_update_delete_roundtrip() {
        with_temp_root(|_| {
            let mut props = HashMap::new();
            props.insert(
                "title".into(),
                serde_json::Value::String("My Web App".into()),
            );
            props.insert(
                "icon".into(),
                serde_json::Value::String("\u{1F4C1}".into()),
            );
            props.insert(
                "workingDirs".into(),
                serde_json::json!(["/Users/me/code/web-app"]),
            );

            let created = create_source(
                SourceType::Project,
                "web-app",
                "frontend monorepo",
                "Use pnpm. Prefer Vitest.",
                true,
                None,
                props.clone(),
            )
            .unwrap();
            assert_eq!(created.name, "web-app");
            assert_eq!(created.source_type, SourceType::Project);
            assert!(created.global);
            assert_eq!(
                created.properties.get("title").and_then(|v| v.as_str()),
                Some("My Web App")
            );

            let read = read_project("web-app").unwrap();
            assert_eq!(read.description, "frontend monorepo");
            assert_eq!(read.content, "Use pnpm. Prefer Vitest.");

            let dirs = project_working_dirs("web-app");
            assert_eq!(dirs, vec!["/Users/me/code/web-app".to_string()]);

            let mut new_props = props.clone();
            new_props.insert("color".into(), serde_json::Value::String("#3b82f6".into()));
            let updated = update_source(
                SourceType::Project,
                created.path.as_str(),
                "web-app",
                "frontend monorepo",
                "Updated body",
                new_props,
            )
            .unwrap();
            assert_eq!(updated.content, "Updated body");
            assert_eq!(
                updated.properties.get("color").and_then(|v| v.as_str()),
                Some("#3b82f6")
            );

            delete_source(SourceType::Project, created.path.as_str()).unwrap();
            assert!(read_project("web-app").is_err());
        });
    }

    #[test]
    fn project_update_rejects_slug_change() {
        with_temp_root(|_| {
            let created = create_source(
                SourceType::Project,
                "old-slug",
                "d",
                "c",
                true,
                None,
                HashMap::new(),
            )
            .unwrap();
            let err = update_source(
                SourceType::Project,
                created.path.as_str(),
                "new-slug",
                "d",
                "c",
                HashMap::new(),
            )
            .unwrap_err();
            assert!(format!("{:?}", err).contains("slug cannot be changed"));
        });
    }
}
