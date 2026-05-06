//! Filesystem-backed CRUD for [`SourceEntry`] values exchanged over ACP custom

use crate::skills::{
    build_skill_md, discover_skills, infer_skill_name, is_global_skill_dir,
    parse_skill_frontmatter, resolve_discoverable_skill_dir, resolve_skill_dir, skill_base_dir,
    validate_skill_name,
};
use agent_client_protocol::Error;
use fs_err as fs;
use goose_sdk::custom_requests::{SourceEntry, SourceType};
use serde::Deserialize;
use std::path::PathBuf;

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

fn require_skill_type(source_type: SourceType) -> Result<(), Error> {
    if source_type != SourceType::Skill {
        return Err(Error::invalid_params().data(format!(
            "Source type '{}' is not supported. Only 'skill' is currently supported.",
            source_type
        )));
    }
    Ok(())
}

fn require_listable_type(source_type: Option<SourceType>) -> Result<SourceType, Error> {
    match source_type.unwrap_or(SourceType::Skill) {
        SourceType::Skill => Ok(SourceType::Skill),
        SourceType::BuiltinSkill => Ok(SourceType::BuiltinSkill),
        other => Err(Error::invalid_params().data(format!(
            "Source type '{}' is not supported. Only 'skill' and 'builtinSkill' are currently supported for listing.",
            other
        ))),
    }
}

fn source_entry(
    source_type: SourceType,
    name: &str,
    description: &str,
    content: &str,
    dir: &std::path::Path,
    global: bool,
) -> SourceEntry {
    SourceEntry {
        source_type,
        name: name.to_string(),
        description: description.to_string(),
        content: content.to_string(),
        directory: dir.to_string_lossy().to_string(),
        global,
        supporting_files: Vec::new(),
    }
}

fn builtin_skill_entry(mut source: SourceEntry) -> SourceEntry {
    source.source_type = SourceType::BuiltinSkill;
    source.directory = format!("builtin://skills/{}", source.name);
    source.global = true;
    source.supporting_files.clear();
    source
}

pub fn create_source(
    source_type: SourceType,
    name: &str,
    description: &str,
    content: &str,
    global: bool,
    project_dir: Option<&str>,
) -> Result<SourceEntry, Error> {
    require_skill_type(source_type)?;
    validate_skill_name(name)?;
    let dir = skill_base_dir(global, project_dir)?.join(name);

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
    path: &str,
    name: &str,
    description: &str,
    content: &str,
) -> Result<SourceEntry, Error> {
    require_skill_type(source_type)?;
    validate_skill_name(name)?;

    let dir = resolve_discoverable_skill_dir(path)?;
    let current_dir_name = dir
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| Error::internal_error().data("Failed to resolve source directory name"))?;

    let target_dir = if name == current_dir_name {
        dir.clone()
    } else {
        let base_dir = dir.parent().ok_or_else(|| {
            Error::internal_error().data("Failed to resolve source base directory")
        })?;
        let target_dir = base_dir.join(name);

        if target_dir.exists() {
            return Err(
                Error::invalid_params().data(format!("A source named \"{}\" already exists", name))
            );
        }

        fs::rename(&dir, &target_dir).map_err(|e| {
            Error::internal_error().data(format!("Failed to rename source directory: {e}"))
        })?;

        target_dir
    };

    let file_path = target_dir.join("SKILL.md");
    let md = build_skill_md(name, description, content);
    fs::write(&file_path, md)
        .map_err(|e| Error::internal_error().data(format!("Failed to write SKILL.md: {e}")))?;

    Ok(source_entry(
        source_type,
        name,
        description,
        content,
        &target_dir,
        is_global_skill_dir(&target_dir),
    ))
}

pub fn delete_source(source_type: SourceType, path: &str) -> Result<(), Error> {
    require_skill_type(source_type)?;
    let dir = resolve_skill_dir(path)?;
    fs::remove_dir_all(&dir)
        .map_err(|e| Error::internal_error().data(format!("Failed to delete source: {e}")))?;
    Ok(())
}

pub fn list_sources(
    source_type: Option<SourceType>,
    project_dir: Option<&str>,
) -> Result<Vec<SourceEntry>, Error> {
    let listed_type = require_listable_type(source_type)?;

    let working_dir = project_dir
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from);

    let mut sources: Vec<SourceEntry> = discover_skills(working_dir.as_deref())
        .into_iter()
        .filter(|s| s.source_type == listed_type)
        .map(|s| {
            if listed_type == SourceType::BuiltinSkill {
                builtin_skill_entry(s)
            } else {
                s
            }
        })
        .collect();

    sources.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sources)
}

pub fn export_source(source_type: SourceType, path: &str) -> Result<(String, String), Error> {
    require_skill_type(source_type)?;
    let dir = resolve_discoverable_skill_dir(path)?;

    let md = dir.join("SKILL.md");
    let raw = fs::read_to_string(&md)
        .map_err(|e| Error::internal_error().data(format!("Failed to read SKILL.md: {e}")))?;
    let (description, content) = parse_skill_frontmatter(&raw);

    let name = infer_skill_name(&dir);

    let export = serde_json::json!({
        "version": 1,
        "type": "skill",
        "name": name,
        "description": description,
        "content": content,
    });
    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| Error::internal_error().data(format!("Failed to serialize source: {e}")))?;
    let filename = format!("{}.skill.json", name);
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

    match value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("skill")
    {
        "skill" => {}
        other => {
            return Err(Error::invalid_params().data(format!(
                "Source type '{}' is not supported. Only 'skill' is currently supported.",
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

    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::invalid_params().data("Missing or invalid \"description\" field"))?
        .to_string();
    if description.is_empty() {
        return Err(Error::invalid_params().data("Source description must not be empty"));
    }

    let content = value
        .get("content")
        .or_else(|| value.get("instructions"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

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

    let dir = base.join(&final_name);
    fs::create_dir_all(&dir).map_err(|e| {
        Error::internal_error().data(format!("Failed to create source directory: {e}"))
    })?;
    let file_path = dir.join("SKILL.md");
    let md = build_skill_md(&final_name, &description, &content);
    fs::write(&file_path, md)
        .map_err(|e| Error::internal_error().data(format!("Failed to write SKILL.md: {e}")))?;

    Ok(vec![source_entry(
        SourceType::Skill,
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
        )
        .unwrap();
        assert_eq!(created.name, "my-skill");
        assert!(!created.global);
        let dir = PathBuf::from(&created.directory);
        assert!(dir.join("SKILL.md").exists());

        let listed = list_sources(Some(SourceType::Skill), Some(project)).unwrap();
        assert!(listed.iter().any(|s| s.name == "my-skill" && !s.global));

        let updated = update_source(
            SourceType::Skill,
            created.directory.as_str(),
            "my-skill",
            "now does a different thing",
            "step three",
        )
        .unwrap();
        assert_eq!(updated.description, "now does a different thing");
        assert_eq!(updated.name, "my-skill");

        delete_source(SourceType::Skill, created.directory.as_str()).unwrap();
        assert!(!dir.exists());
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

        let listed =
            list_sources(Some(SourceType::Skill), Some(project.to_str().unwrap())).unwrap();
        let exported_skill = listed
            .iter()
            .find(|skill| skill.name == "portable")
            .expect("expected listed skill");

        let (json, filename) =
            export_source(SourceType::Skill, exported_skill.directory.as_str()).unwrap();
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
        let listed = list_sources(Some(SourceType::BuiltinSkill), None).unwrap();
        let builtin = listed
            .iter()
            .find(|source| source.name == "goose-doc-guide")
            .expect("expected goose-doc-guide builtin skill");

        assert_eq!(builtin.source_type, SourceType::BuiltinSkill);
        assert!(builtin.global);
        assert_eq!(builtin.directory, "builtin://skills/goose-doc-guide");
        assert!(builtin.supporting_files.is_empty());
        assert!(!builtin.content.is_empty());
    }

    #[test]
    fn list_skill_excludes_builtin_skills() {
        let listed = list_sources(Some(SourceType::Skill), None).unwrap();
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
            build_skill_md("goose-doc-guide", "project override", "Use project docs"),
        )
        .unwrap();

        let builtins = list_sources(
            Some(SourceType::BuiltinSkill),
            Some(project.to_str().unwrap()),
        )
        .unwrap();
        assert!(!builtins
            .iter()
            .any(|source| source.name == "goose-doc-guide"));

        let skills =
            list_sources(Some(SourceType::Skill), Some(project.to_str().unwrap())).unwrap();
        let project_skill = skills
            .iter()
            .find(|source| source.name == "goose-doc-guide")
            .expect("expected project skill");
        assert_eq!(project_skill.source_type, SourceType::Skill);
        assert_eq!(project_skill.description, "project override");
    }

    #[test]
    fn mutations_reject_non_writable_source_types() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        let err = create_source(
            SourceType::BuiltinSkill,
            "x",
            "d",
            "c",
            false,
            Some(project),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = update_source(
            SourceType::BuiltinSkill,
            "builtin://skills/x",
            "x",
            "d",
            "c",
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = update_source(SourceType::Recipe, "x", "x", "d", "c").unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = delete_source(SourceType::BuiltinSkill, "builtin://skills/x").unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = delete_source(SourceType::Subrecipe, "x").unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let listed = list_sources(Some(SourceType::BuiltinSkill), Some(project)).unwrap();
        assert!(listed
            .iter()
            .any(|source| source.source_type == SourceType::BuiltinSkill));

        let err = list_sources(Some(SourceType::Recipe), Some(project)).unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = export_source(SourceType::BuiltinSkill, "builtin://skills/x").unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let err = export_source(SourceType::Recipe, "x").unwrap_err();
        assert!(format!("{:?}", err).contains("not supported"));

        let payload = serde_json::json!({
            "version": 1,
            "type": "builtinSkill",
            "name": "x",
            "description": "d",
            "content": "c",
        })
        .to_string();
        let err = import_sources(&payload, false, Some(project)).unwrap_err();
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
        )
        .unwrap();

        let skill_dir = tmp.path().join(".agents").join("skills").join("my-dir");
        let updated = update_source(
            SourceType::Skill,
            skill_dir.to_str().unwrap(),
            "my-dir",
            "new description",
            "new body",
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

        let listed =
            list_sources(Some(SourceType::Skill), Some(tmp.path().to_str().unwrap())).unwrap();
        let skill = listed
            .iter()
            .find(|source| source.name == "test-skill" && !source.global)
            .unwrap();
        assert!(skill.directory.contains(".agents/skills"));
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

        let listed =
            list_sources(Some(SourceType::Skill), Some(tmp.path().to_str().unwrap())).unwrap();
        let matching: Vec<_> = listed
            .iter()
            .filter(|source| source.name == "shared-skill" && !source.global)
            .collect();
        assert_eq!(matching.len(), 1);
        assert!(matching[0].directory.contains(".agents/skills"));
        assert_eq!(matching[0].description, "preferred");

        let exported = export_source(SourceType::Skill, matching[0].directory.as_str()).unwrap();
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
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("not found"));
    }
}
