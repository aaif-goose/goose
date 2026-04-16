use std::fs;
use std::path::PathBuf;

fn skills_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(home.join(".agents").join("skills"))
}

/// Validates that a skill name is kebab-case only: `^[a-z0-9]+(-[a-z0-9]+)*$`.
/// This prevents path traversal attacks (e.g. `../../.ssh/authorized_keys`).
fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Skill name must not be empty".to_string());
    }
    let mut expect_alnum = true; // true = next char must be [a-z0-9], false = can also be '-'
    for ch in name.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            expect_alnum = false;
        } else if ch == '-' && !expect_alnum {
            expect_alnum = true; // char after '-' must be [a-z0-9]
        } else {
            return Err(format!(
                "Invalid skill name \"{}\". Names must be kebab-case (lowercase letters, digits, and hyphens; \
                 must not start or end with a hyphen or contain consecutive hyphens).",
                name
            ));
        }
    }
    if expect_alnum {
        // name ended with '-'
        return Err(format!(
            "Invalid skill name \"{}\". Names must not end with a hyphen.",
            name
        ));
    }
    Ok(())
}

fn build_skill_md(name: &str, description: &str, instructions: &str) -> String {
    // Escape embedded single quotes by doubling them, then wrap in single quotes
    // to prevent YAML injection in the description field.
    let safe_desc = description.replace('\'', "''");
    let mut md = format!("---\nname: {}\ndescription: '{}'\n---\n", name, safe_desc);
    if !instructions.is_empty() {
        md.push('\n');
        md.push_str(instructions);
        md.push('\n');
    }
    md
}

#[tauri::command]
pub fn create_skill(name: String, description: String, instructions: String) -> Result<(), String> {
    validate_skill_name(&name)?;
    let dir = skills_dir()?.join(&name);

    if dir.exists() {
        return Err(format!("A skill named \"{}\" already exists", name));
    }

    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create skill directory: {}", e))?;

    let skill_path = dir.join("SKILL.md");
    let content = build_skill_md(&name, &description, &instructions);

    fs::write(&skill_path, content).map_err(|e| format!("Failed to write SKILL.md: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn list_skills() -> Result<Vec<SkillInfo>, String> {
    let dir = skills_dir()?;

    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut skills = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("Failed to read skills dir: {}", e))?;

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
        let (description, instructions) = parse_frontmatter(&raw);

        skills.push(SkillInfo {
            name,
            description,
            instructions,
            path: skill_md.to_string_lossy().to_string(),
        });
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

#[tauri::command]
pub fn delete_skill(name: String) -> Result<(), String> {
    validate_skill_name(&name)?;
    let dir = skills_dir()?.join(&name);
    if !dir.exists() {
        return Err(format!("Skill \"{}\" not found", name));
    }
    fs::remove_dir_all(&dir).map_err(|e| format!("Failed to delete skill: {}", e))?;
    Ok(())
}

fn parse_frontmatter(raw: &str) -> (String, String) {
    let trimmed = raw.trim();
    if !trimmed.starts_with("---") {
        return (String::new(), raw.to_string());
    }

    if let Some(end) = trimmed[3..].find("\n---") {
        let front = &trimmed[3..3 + end].trim();
        let body = trimmed[3 + end + 4..].trim().to_string();

        let mut description = String::new();
        for line in front.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("description:") {
                let val = rest.trim();
                // Strip surrounding quotes (single or double)
                let unquoted = val
                    .trim_start_matches(['\'', '"'])
                    .trim_end_matches(['\'', '"']);
                description = if val.starts_with('\'') {
                    // Un-escape doubled single quotes
                    unquoted.replace("''", "'")
                } else {
                    // Legacy double-quote format
                    unquoted.replace("\\\"", "\"")
                }
                .to_string();
            }
        }

        (description, body)
    } else {
        (String::new(), raw.to_string())
    }
}

#[derive(serde::Serialize, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub path: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillExportV1 {
    version: u32,
    name: String,
    description: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    instructions: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSkillResult {
    json: String,
    filename: String,
}

#[tauri::command]
pub fn update_skill(
    name: String,
    description: String,
    instructions: String,
) -> Result<SkillInfo, String> {
    validate_skill_name(&name)?;
    let dir = skills_dir()?.join(&name);

    if !dir.exists() {
        return Err(format!("Skill \"{}\" not found", name));
    }

    let skill_path = dir.join("SKILL.md");
    let content = build_skill_md(&name, &description, &instructions);

    fs::write(&skill_path, content).map_err(|e| format!("Failed to write SKILL.md: {}", e))?;

    Ok(SkillInfo {
        name: name.clone(),
        description,
        instructions,
        path: skill_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn export_skill(name: String) -> Result<ExportSkillResult, String> {
    validate_skill_name(&name)?;
    let dir = skills_dir()?.join(&name);

    if !dir.exists() {
        return Err(format!("Skill \"{}\" not found", name));
    }

    let skill_md = dir.join("SKILL.md");
    let raw =
        fs::read_to_string(&skill_md).map_err(|e| format!("Failed to read SKILL.md: {}", e))?;
    let (description, instructions) = parse_frontmatter(&raw);

    let export = SkillExportV1 {
        version: 1,
        name: name.clone(),
        description,
        instructions,
    };

    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| format!("Failed to serialize skill: {}", e))?;

    let filename = format!("{}.skill.json", name);

    Ok(ExportSkillResult { json, filename })
}

#[tauri::command]
pub fn import_skills(file_bytes: Vec<u8>, file_name: String) -> Result<Vec<SkillInfo>, String> {
    // Validate file extension
    if !file_name.ends_with(".skill.json") && !file_name.ends_with(".json") {
        return Err("File must have a .skill.json or .json extension".to_string());
    }

    // Parse bytes as UTF-8
    let text =
        String::from_utf8(file_bytes).map_err(|e| format!("File is not valid UTF-8: {}", e))?;

    // Parse as JSON
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {}", e))?;

    // Validate version
    let version = value
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or("Missing or invalid \"version\" field")?;
    if version != 1 {
        return Err(format!("Unsupported skill export version: {}", version));
    }

    // Extract fields
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing or invalid \"name\" field")?
        .to_string();
    if name.is_empty() {
        return Err("Skill name must not be empty".to_string());
    }

    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or("Missing or invalid \"description\" field")?
        .to_string();
    if description.is_empty() {
        return Err("Skill description must not be empty".to_string());
    }

    let instructions = value
        .get("instructions")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Validate the name
    validate_skill_name(&name)?;

    // Determine final name, avoiding collisions
    let base_dir = skills_dir()?;
    let mut final_name = name.clone();
    if base_dir.join(&final_name).exists() {
        final_name = format!("{}-imported", name);
        // If that also exists, append a number
        let mut counter = 2u32;
        while base_dir.join(&final_name).exists() {
            final_name = format!("{}-imported-{}", name, counter);
            counter += 1;
        }
    }

    // Create the skill on disk
    let dir = base_dir.join(&final_name);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create skill directory: {}", e))?;

    let skill_path = dir.join("SKILL.md");
    let content = build_skill_md(&final_name, &description, &instructions);
    fs::write(&skill_path, content).map_err(|e| format!("Failed to write SKILL.md: {}", e))?;

    Ok(vec![SkillInfo {
        name: final_name,
        description,
        instructions,
        path: skill_path.to_string_lossy().to_string(),
    }])
}
