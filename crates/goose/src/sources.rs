//! Filesystem-backed CRUD for [`SourceEntry`] values exchanged over ACP custom
//! methods. A source is a user-editable entity stored under a per-scope root
//! directory — `~/.agents/skills` for global sources and `<project>/.goose/skills`
//! for project-specific sources.

use crate::agents::platform_extensions::parse_frontmatter;
use fs_err as fs;
use goose_sdk::custom_requests::{SourceEntry, SourceType};
use sacp::Error;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct SkillFront {
    #[serde(default)]
    description: String,
}

const GLOBAL_SKILLS_SUBPATH: &[&str] = &[".agents", "skills"];
const PROJECT_SKILLS_SUBPATH: &[&str] = &[".goose", "skills"];

fn home_dir() -> Result<PathBuf, Error> {
    dirs::home_dir()
        .ok_or_else(|| Error::internal_error().data("Could not determine home directory"))
}

fn skills_dir_global() -> Result<PathBuf, Error> {
    let mut dir = home_dir()?;
    for part in GLOBAL_SKILLS_SUBPATH {
        dir = dir.join(part);
    }
    Ok(dir)
}

fn skills_dir_project(project_dir: &str) -> Result<PathBuf, Error> {
    if project_dir.trim().is_empty() {
        return Err(
            Error::invalid_params().data("projectDir must not be empty when global is false")
        );
    }
    let mut dir = PathBuf::from(project_dir);
    for part in PROJECT_SKILLS_SUBPATH {
        dir = dir.join(part);
    }
    Ok(dir)
}

fn source_base_dir(
    source_type: SourceType,
    global: bool,
    project_dir: Option<&str>,
) -> Result<PathBuf, Error> {
    match source_type {
        SourceType::Skill => {
            if global {
                skills_dir_global()
            } else {
                let pd = project_dir.ok_or_else(|| {
                    Error::invalid_params().data("projectDir is required when global is false")
                })?;
                skills_dir_project(pd)
            }
        }
    }
}

/// Kebab-case validation: `^[a-z0-9]+(-[a-z0-9]+)*$`. Prevents path traversal
/// via names like `../../.ssh/authorized_keys`.
fn validate_source_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::invalid_params().data("Source name must not be empty"));
    }
    let mut expect_alnum = true;
    for ch in name.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            expect_alnum = false;
        } else if ch == '-' && !expect_alnum {
            expect_alnum = true;
        } else {
            return Err(Error::invalid_params().data(format!(
                "Invalid source name \"{}\". Names must be kebab-case (lowercase letters, digits, and hyphens; \
                 must not start or end with a hyphen or contain consecutive hyphens).",
                name
            )));
        }
    }
    if expect_alnum {
        return Err(Error::invalid_params().data(format!(
            "Invalid source name \"{}\". Names must not end with a hyphen.",
            name
        )));
    }
    Ok(())
}

fn build_skill_md(name: &str, description: &str, content: &str) -> String {
    // YAML single-quoted strings escape a literal single quote by doubling it.
    let safe_desc = description.replace('\'', "''");
    let mut md = format!("---\nname: {}\ndescription: '{}'\n---\n", name, safe_desc);
    if !content.is_empty() {
        md.push('\n');
        md.push_str(content);
        md.push('\n');
    }
    md
}

fn parse_skill_frontmatter(raw: &str) -> (String, String) {
    if !raw.trim_start().starts_with("---") {
        return (String::new(), raw.to_string());
    }
    match parse_frontmatter::<SkillFront>(raw) {
        Ok(Some((meta, body))) => (meta.description, body),
        _ => (String::new(), raw.to_string()),
    }
}

fn source_entry(
    source_type: SourceType,
    name: &str,
    description: &str,
    content: &str,
    dir: &Path,
    global: bool,
) -> SourceEntry {
    SourceEntry {
        source_type,
        name: name.to_string(),
        description: description.to_string(),
        content: content.to_string(),
        directory: dir.to_string_lossy().to_string(),
        global,
    }
}

pub fn create_source(
    source_type: SourceType,
    name: &str,
    description: &str,
    content: &str,
    global: bool,
    project_dir: Option<&str>,
) -> Result<SourceEntry, Error> {
    validate_source_name(name)?;
    let dir = source_base_dir(source_type, global, project_dir)?.join(name);

    if dir.exists() {
        return Err(
            Error::invalid_params().data(format!("A source named \"{}\" already exists", name))
        );
    }

    fs::create_dir_all(&dir).map_err(|e| {
        Error::internal_error().data(format!("Failed to create source directory: {e}"))
    })?;
    let file_path = dir.join("SKILL.md");
    let md = build_skill_md(name, description, content);
    fs::write(&file_path, md)
        .map_err(|e| Error::internal_error().data(format!("Failed to write SKILL.md: {e}")))?;

    Ok(source_entry(
        source_type,
        name,
        description,
        content,
        &dir,
        global,
    ))
}

pub fn update_source(
    source_type: SourceType,
    name: &str,
    description: &str,
    content: &str,
    global: bool,
    project_dir: Option<&str>,
) -> Result<SourceEntry, Error> {
    validate_source_name(name)?;
    let dir = source_base_dir(source_type, global, project_dir)?.join(name);

    if !dir.exists() {
        return Err(Error::invalid_params().data(format!("Source \"{}\" not found", name)));
    }

    let file_path = dir.join("SKILL.md");
    let md = build_skill_md(name, description, content);
    fs::write(&file_path, md)
        .map_err(|e| Error::internal_error().data(format!("Failed to write SKILL.md: {e}")))?;

    Ok(source_entry(
        source_type,
        name,
        description,
        content,
        &dir,
        global,
    ))
}

pub fn delete_source(
    source_type: SourceType,
    name: &str,
    global: bool,
    project_dir: Option<&str>,
) -> Result<(), Error> {
    validate_source_name(name)?;
    let dir = source_base_dir(source_type, global, project_dir)?.join(name);

    if !dir.exists() {
        return Err(Error::invalid_params().data(format!("Source \"{}\" not found", name)));
    }
    fs::remove_dir_all(&dir)
        .map_err(|e| Error::internal_error().data(format!("Failed to delete source: {e}")))?;
    Ok(())
}

pub fn list_sources(
    source_type: Option<SourceType>,
    project_dir: Option<&str>,
) -> Result<Vec<SourceEntry>, Error> {
    let kinds: Vec<SourceType> = match source_type {
        Some(k) => vec![k],
        None => vec![SourceType::Skill],
    };

    let mut sources = Vec::new();
    for kind in kinds {
        match kind {
            SourceType::Skill => {
                if let Some(pd) = project_dir {
                    if !pd.trim().is_empty() {
                        let dir = skills_dir_project(pd)?;
                        sources.extend(read_skill_dir(&dir, false)?);
                    }
                }
                let dir = skills_dir_global()?;
                sources.extend(read_skill_dir(&dir, true)?);
            }
        }
    }
    sources.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sources)
}

fn read_skill_dir(dir: &Path, global: bool) -> Result<Vec<SourceEntry>, Error> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(dir)
        .map_err(|e| Error::internal_error().data(format!("Failed to read skills dir: {e}")))?;

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let raw = fs::read_to_string(&skill_md).unwrap_or_default();
        let (description, content) = parse_skill_frontmatter(&raw);
        out.push(source_entry(
            SourceType::Skill,
            &name,
            &description,
            &content,
            &path,
            global,
        ));
    }
    Ok(out)
}

pub fn export_source(
    source_type: SourceType,
    name: &str,
    global: bool,
    project_dir: Option<&str>,
) -> Result<(String, String), Error> {
    validate_source_name(name)?;
    let dir = source_base_dir(source_type, global, project_dir)?.join(name);

    if !dir.exists() {
        return Err(Error::invalid_params().data(format!("Source \"{}\" not found", name)));
    }

    let md = dir.join("SKILL.md");
    let raw = fs::read_to_string(&md)
        .map_err(|e| Error::internal_error().data(format!("Failed to read SKILL.md: {e}")))?;
    let (description, content) = parse_skill_frontmatter(&raw);

    let type_slug = match source_type {
        SourceType::Skill => "skill",
    };
    let export = serde_json::json!({
        "version": 1,
        "type": type_slug,
        "name": name,
        "description": description,
        "content": content,
    });
    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| Error::internal_error().data(format!("Failed to serialize source: {e}")))?;
    let filename = format!("{}.{}.json", name, type_slug);
    Ok((json, filename))
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

    // Default to `skill` to preserve compatibility with pre-sources skill exports.
    let source_type = match value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("skill")
    {
        "skill" => SourceType::Skill,
        other => {
            return Err(Error::invalid_params().data(format!("Unsupported source type: {}", other)));
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

    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::invalid_params().data("Missing or invalid \"description\" field"))?
        .to_string();
    if description.is_empty() {
        return Err(Error::invalid_params().data("Source description must not be empty"));
    }

    // Accept both the new `content` key and the legacy skills `instructions` key.
    let content = value
        .get("content")
        .or_else(|| value.get("instructions"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    validate_source_name(&name)?;

    let base = source_base_dir(source_type, global, project_dir)?;
    let mut final_name = name.clone();
    if base.join(&final_name).exists() {
        final_name = format!("{}-imported", name);
        let mut counter = 2u32;
        while base.join(&final_name).exists() {
            final_name = format!("{}-imported-{}", name, counter);
            counter += 1;
        }
    }

    let dir = base.join(&final_name);
    fs::create_dir_all(&dir).map_err(|e| {
        Error::internal_error().data(format!("Failed to create source directory: {e}"))
    })?;
    let file_path = dir.join("SKILL.md");
    let md = build_skill_md(&final_name, &description, &content);
    fs::write(&file_path, md)
        .map_err(|e| Error::internal_error().data(format!("Failed to write SKILL.md: {e}")))?;

    Ok(vec![source_entry(
        source_type,
        &final_name,
        &description,
        &content,
        &dir,
        global,
    )])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn kebab_case_validation() {
        assert!(validate_source_name("my-skill").is_ok());
        assert!(validate_source_name("abc123").is_ok());
        assert!(validate_source_name("").is_err());
        assert!(validate_source_name("-leading").is_err());
        assert!(validate_source_name("trailing-").is_err());
        assert!(validate_source_name("double--hyphen").is_err());
        assert!(validate_source_name("CAPS").is_err());
        assert!(validate_source_name("../escape").is_err());
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
        )
        .unwrap();
        assert_eq!(created.name, "my-skill");
        assert!(!created.global);
        assert!(PathBuf::from(&created.directory).join("SKILL.md").exists());

        let listed = list_sources(Some(SourceType::Skill), Some(project)).unwrap();
        assert!(listed.iter().any(|s| s.name == "my-skill" && !s.global));

        let updated = update_source(
            SourceType::Skill,
            "my-skill",
            "now does a different thing",
            "step three",
            false,
            Some(project),
        )
        .unwrap();
        assert_eq!(updated.description, "now does a different thing");

        delete_source(SourceType::Skill, "my-skill", false, Some(project)).unwrap();
        assert!(!PathBuf::from(&created.directory).exists());
    }

    #[test]
    fn create_rejects_duplicate_name() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        create_source(SourceType::Skill, "dup", "d", "c", false, Some(project)).unwrap();
        let err =
            create_source(SourceType::Skill, "dup", "d", "c", false, Some(project)).unwrap_err();
        assert!(format!("{:?}", err).contains("already exists"));
    }

    #[test]
    fn project_scope_requires_project_dir() {
        let err = create_source(SourceType::Skill, "x", "d", "c", false, None).unwrap_err();
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
        )
        .unwrap();

        let (json, filename) = export_source(
            SourceType::Skill,
            "portable",
            false,
            Some(project_a.to_str().unwrap()),
        )
        .unwrap();
        assert_eq!(filename, "portable.skill.json");

        let imported = import_sources(&json, false, Some(project_b.to_str().unwrap())).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "portable");
        assert_eq!(imported[0].description, "describes itself");
        assert_eq!(imported[0].content, "body goes here");
    }

    #[test]
    fn import_collision_appends_suffix() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        create_source(SourceType::Skill, "busy", "d", "c", false, Some(project)).unwrap();

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
}
