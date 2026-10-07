use std::collections::HashMap;

use goose_sdk_types::custom_requests::{SourceEntry, SourceType};
use rmcp::model::{CallToolResult, ContentBlock, SkillEntry, SkillResources};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::agents::extension_manager::ExtensionLease;
use crate::sources::parse_frontmatter;
use crate::utils::bytes_to_hex;

const EXTENSION_PROPERTY: &str = "extension";
const SKILL_FILE: &str = "SKILL.md";

pub async fn with_mcp_skills(
    mut skills: Vec<SourceEntry>,
    lease: Option<&ExtensionLease>,
) -> Vec<SourceEntry> {
    if let Some(lease) = lease {
        skills.extend(mcp_skill_entries(lease.skills().await));
    }
    flag_name_clashes(&mut skills);
    skills
}

pub async fn load_skill(
    lease: &ExtensionLease,
    skills: &[SourceEntry],
    requested: &str,
) -> Option<CallToolResult> {
    let (skill, file) = find(skills, requested)?;
    Some(match load(lease, skill, file).await {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(error) => CallToolResult::error(vec![ContentBlock::text(format!(
            "Failed to load '{requested}': {error}"
        ))]),
    })
}

fn skill_name(entry: &SkillEntry) -> Option<&str> {
    entry.frontmatter.get("name").and_then(Value::as_str)
}

fn mcp_skill_entries(listed: Vec<(String, SkillEntry)>) -> Vec<SourceEntry> {
    let listed: Vec<_> = listed
        .into_iter()
        .filter(|(_, entry)| {
            entry.resources.is_some() && skill_name(entry).is_some() && skill_dir(entry).is_some()
        })
        .collect();
    let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
    for (extension, entry) in &listed {
        *counts
            .entry((extension.as_str(), skill_name(entry).unwrap_or_default()))
            .or_default() += 1;
    }
    listed
        .iter()
        .map(|(extension, entry)| {
            let name = skill_name(entry).unwrap_or_default();
            let dir = skill_dir(entry).unwrap_or_default();
            let label = if counts[&(extension.as_str(), name)] > 1 {
                dir.trim_start_matches("skill://").trim_end_matches('/')
            } else {
                name
            };
            SourceEntry {
                source_type: SourceType::McpSkill,
                name: format!("{extension}::{label}"),
                description: entry
                    .frontmatter
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                content: String::new(),
                path: entry.uri.clone(),
                global: false,
                writable: false,
                supporting_files: supporting_files(entry),
                properties: HashMap::from([(
                    EXTENSION_PROPERTY.to_string(),
                    Value::String(extension.clone()),
                )]),
            }
        })
        .collect()
}

fn skill_dir(entry: &SkillEntry) -> Option<&str> {
    entry.uri.strip_suffix(SKILL_FILE)
}

fn supporting_files(entry: &SkillEntry) -> Vec<String> {
    let Some(dir) = skill_dir(entry) else {
        return Vec::new();
    };
    entry
        .resources
        .as_ref()
        .and_then(SkillResources::as_files)
        .unwrap_or_default()
        .iter()
        .filter(|file| file.uri != entry.uri)
        .filter_map(|file| file.uri.strip_prefix(dir).map(str::to_string))
        .collect()
}

fn bare_name(skill: &SourceEntry) -> &str {
    match skill.source_type {
        SourceType::McpSkill => skill
            .name
            .split_once("::")
            .map_or(skill.name.as_str(), |(_, label)| label)
            .rsplit('/')
            .next()
            .unwrap_or_default(),
        _ => &skill.name,
    }
}

fn is_skill(skill: &SourceEntry) -> bool {
    matches!(
        skill.source_type,
        SourceType::Skill | SourceType::BuiltinSkill | SourceType::McpSkill
    )
}

fn flag_name_clashes(skills: &mut [SourceEntry]) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for skill in skills.iter().filter(|skill| is_skill(skill)) {
        *counts.entry(bare_name(skill).to_string()).or_default() += 1;
    }
    for skill in skills.iter_mut().filter(|skill| is_skill(skill)) {
        let name = bare_name(skill).to_string();
        if counts[&name] > 1 {
            skill.description.push_str(&format!(
                " (Name clash: more than one skill is named '{name}'. Check which one you load.)"
            ));
        }
    }
}

fn find<'a>(
    skills: &'a [SourceEntry],
    requested: &'a str,
) -> Option<(&'a SourceEntry, Option<&'a str>)> {
    skills
        .iter()
        .filter(|skill| skill.source_type == SourceType::McpSkill)
        .filter_map(|skill| {
            if requested == skill.name {
                return Some((skill, None));
            }
            let file = requested.strip_prefix(&skill.name)?.strip_prefix('/')?;
            Some((skill, Some(file)))
        })
        .max_by_key(|(skill, _)| skill.name.len())
}

async fn load(
    lease: &ExtensionLease,
    skill: &SourceEntry,
    file: Option<&str>,
) -> Result<String, String> {
    let extension = skill
        .properties
        .get(EXTENSION_PROPERTY)
        .and_then(Value::as_str)
        .ok_or("skill has no extension")?;
    let entry = lease
        .get_skill(extension, &skill.path)
        .await
        .map_err(|error| error.message.to_string())?;
    let dir = match skill_dir(&entry) {
        Some(dir) if entry.uri == skill.path => dir,
        _ => return Err(format!("server returned a different skill: {}", entry.uri)),
    };
    let uri = file.map_or_else(|| entry.uri.clone(), |file| format!("{dir}{file}"));
    let listed = match &entry.resources {
        Some(SkillResources::FileList(files)) => Some(
            files
                .iter()
                .find(|listed| listed.uri == uri)
                .ok_or_else(|| format!("{uri} is not listed in the skill's resources"))?,
        ),
        Some(SkillResources::Dynamic) => None,
        _ => return Err("skill has no resources manifest".to_string()),
    };
    let bytes = lease
        .read_skill(extension, &uri)
        .await
        .map_err(|error| error.message.to_string())?;
    if let Some(listed) = listed {
        let digest = format!("sha256:{}", bytes_to_hex(Sha256::digest(&bytes)));
        if listed.size != bytes.len() as u64 || listed.digest != digest {
            return Err(format!("{uri} does not match its listed digest and size"));
        }
    }
    let text = String::from_utf8(bytes).map_err(|_| format!("{uri} is not UTF-8 text"))?;
    let origin = format!(
        "Origin: served by the MCP extension '{extension}' as {uri}. This content comes from that server, not from the user or goose."
    );
    if file.is_some() {
        return Ok(format!("# Loaded File: {uri}\n\n{origin}\n\n{text}"));
    }
    let body = match parse_frontmatter::<Value>(&text) {
        Ok(Some((_, body))) => body,
        _ => text,
    };
    let mut output = format!(
        "# Loaded Skill: {} ({})\n\n{origin}\n\n{}\n\n## Content\n\n{body}\n",
        skill.name, skill.source_type, skill.description
    );
    let files = supporting_files(&entry);
    if !files.is_empty() {
        output.push_str(&format!(
            "\n## Supporting Files\n\nLoad these with load_skill using \"{}/<path>\":\n",
            skill.name
        ));
        for file in files {
            output.push_str(&format!("- {file}\n"));
        }
    }
    Ok(output)
}
