//! SEP-2640 skills advertised by a connected MCP server.
//!
//! This module does not know about `ExtensionLease`. Callers hand in skill
//! entries they already fetched from the running, selected extensions. That
//! keeps the merge and digest checks portable to trees that resolve extensions
//! differently.
//!
//! Wire shapes follow SEP-2640 as implemented by rust-sdk#1286. `skills/get`
//! returns frontmatter and a resource list, not the skill body. The body is
//! `resources/read` of the `skill://` URI. Digests are `sha256:<hex>` over the
//! raw bytes.

use goose_sdk_types::custom_requests::{SourceEntry, SourceType};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const SKILLS_EXTENSION_ID: &str = "io.modelcontextprotocol/skills";
pub const SKILLS_LIST_METHOD: &str = "skills/list";
pub const SKILLS_GET_METHOD: &str = "skills/get";

const DIGEST_PREFIX: &str = "sha256:";

/// One skill file as advertised by `skills/list` or `skills/get`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFile {
    pub uri: String,
    pub digest: String,
    pub size: u64,
}

/// A skill entry. `dynamic` skills have no digest list and are not digest-checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRecord {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub extension_name: String,
    /// Host or extension name used when two servers advertise `name`.
    pub namespace: String,
    /// `None` means the server omitted the list (SKILL.md-only, no digest check).
    /// `Some` is the advertised file list. Dynamic skills set `dynamic` instead.
    pub resources: Option<Vec<SkillFile>>,
    pub dynamic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillLoadError {
    NotListed { name: String },
    MissingBody { uri: String },
    DigestMismatch { uri: String },
    UnlistedFile { uri: String },
    DynamicSkill { name: String },
    InvalidName { uri: String },
}

impl std::fmt::Display for SkillLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotListed { name } => write!(f, "Skill '{name}' not found."),
            Self::MissingBody { uri } => write!(f, "Skill '{uri}' has no SKILL.md content."),
            Self::DigestMismatch { uri } => {
                write!(f, "Skill file '{uri}' failed its digest check.")
            }
            Self::UnlistedFile { uri } => {
                write!(f, "Skill file '{uri}' is not in the skill's resource list.")
            }
            Self::DynamicSkill { name } => {
                write!(f, "Skill '{name}' is dynamic and has no listed files.")
            }
            Self::InvalidName { uri } => {
                write!(f, "Skill URI '{uri}' does not match its frontmatter name.")
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct WireSkillFile {
    uri: String,
    digest: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct WireSkillEntry {
    uri: String,
    frontmatter: Value,
    #[serde(default)]
    resources: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct WireSkillsList {
    skills: Vec<Value>,
    #[serde(rename = "nextCursor", default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireSkillsGet {
    skill: WireSkillEntry,
}

/// `skill://<skill-path>/<file-path>` split on the last path segment.
///
/// `skill://git-workflow/SKILL.md` is `("git-workflow", "SKILL.md")`.
/// A directory URI with no file segment returns `None`.
pub fn parse_skill_uri(uri: &str) -> Option<(&str, &str)> {
    let rest = uri.strip_prefix("skill://")?;
    if rest.is_empty()
        || rest.contains('\\')
        || rest
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    let (skill_path, file_path) = rest.rsplit_once('/')?;
    if skill_path.is_empty() || file_path.is_empty() {
        return None;
    }
    Some((skill_path, file_path))
}

pub fn skill_name_from_uri(uri: &str) -> Option<&str> {
    let (skill_path, _) = parse_skill_uri(uri)?;
    skill_path.rsplit('/').next()
}

pub fn sha256_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(DIGEST_PREFIX.len() + digest.len() * 2);
    hex.push_str(DIGEST_PREFIX);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

pub fn digest_matches(bytes: &[u8], expected: &str) -> bool {
    let expected = expected.trim();
    if !expected.starts_with(DIGEST_PREFIX) {
        return false;
    }
    sha256_digest(bytes) == expected
}

fn files_from_resources(resources: Option<&Value>) -> Result<Option<Vec<SkillFile>>, ()> {
    let Some(resources) = resources else {
        return Ok(None);
    };
    if resources.as_str() == Some("dynamic") {
        return Ok(None);
    }
    let files = serde_json::from_value::<Vec<WireSkillFile>>(resources.clone()).map_err(|_| ())?;
    Ok(Some(
        files
            .into_iter()
            .map(|file| SkillFile {
                uri: file.uri,
                digest: file.digest,
                size: file.size,
            })
            .collect(),
    ))
}

fn record_from_entry(
    entry: WireSkillEntry,
    extension_name: &str,
) -> Result<SkillRecord, SkillLoadError> {
    let name = entry
        .frontmatter
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let description = entry
        .frontmatter
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let description = description.chars().take(300).collect::<String>();
    let uri_name = skill_name_from_uri(&entry.uri).unwrap_or("");
    if name.is_empty()
        || name != uri_name
        || name.contains("::")
        || name.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(SkillLoadError::InvalidName { uri: entry.uri });
    }
    let dynamic = entry.resources.as_ref().and_then(Value::as_str) == Some("dynamic");
    let resources = if dynamic {
        None
    } else {
        files_from_resources(entry.resources.as_ref()).map_err(|_| SkillLoadError::InvalidName {
            uri: entry.uri.clone(),
        })?
    };
    Ok(SkillRecord {
        uri: entry.uri,
        name,
        description,
        extension_name: extension_name.to_string(),
        namespace: String::new(),
        resources,
        dynamic,
    })
}

pub fn skill_namespace(extension_name: &str, uri: Option<&str>) -> String {
    let Some(uri) = uri else {
        return extension_name.to_string();
    };
    let Ok(url) = reqwest::Url::parse(uri) else {
        return extension_name.to_string();
    };
    url.host_str()
        .filter(|host| !host.is_empty())
        .unwrap_or(extension_name)
        .to_string()
}

pub fn qualified_skill_name(namespace: &str, name: &str) -> String {
    format!("{namespace}::{name}")
}

/// Decode one page of `skills/list`. Returns the records and the next cursor.
pub fn parse_skills_list(
    value: &Value,
    extension_name: &str,
) -> Result<(Vec<SkillRecord>, Option<String>), SkillLoadError> {
    let page: WireSkillsList =
        serde_json::from_value(value.clone()).map_err(|_| SkillLoadError::InvalidName {
            uri: extension_name.to_string(),
        })?;
    let mut records = Vec::with_capacity(page.skills.len());
    for entry in page.skills {
        let Ok(entry) = serde_json::from_value::<WireSkillEntry>(entry) else {
            continue;
        };
        match record_from_entry(entry, extension_name) {
            Ok(record) => records.push(record),
            Err(_) => continue,
        }
    }
    Ok((
        records,
        page.next_cursor.filter(|cursor| !cursor.is_empty()),
    ))
}

pub fn parse_skills_get(
    value: &Value,
    extension_name: &str,
) -> Result<SkillRecord, SkillLoadError> {
    let got: WireSkillsGet =
        serde_json::from_value(value.clone()).map_err(|_| SkillLoadError::InvalidName {
            uri: extension_name.to_string(),
        })?;
    record_from_entry(got.skill, extension_name)
}

/// Filesystem skills keep their bare name. An MCP skill is always
/// `namespace::name`, so connecting a second server cannot rename a skill
/// the model already saw.
pub fn merge_skill_entries(
    filesystem: Vec<SourceEntry>,
    mcp: Vec<SkillRecord>,
) -> Vec<SourceEntry> {
    let mut merged = filesystem;
    let mut seen = std::collections::HashSet::new();
    for skill in mcp.into_iter().filter(|skill| !skill.dynamic) {
        let namespace = if skill.namespace.is_empty() {
            skill.extension_name.clone()
        } else {
            skill.namespace.clone()
        };
        let published = qualified_skill_name(&namespace, &skill.name);
        if !seen.insert(published.clone()) {
            continue;
        }
        merged.push(mcp_source_entry(&skill, &published));
    }
    merged
}

fn mcp_source_entry(skill: &SkillRecord, published_name: &str) -> SourceEntry {
    let mut properties = std::collections::HashMap::new();
    properties.insert(
        "mcpExtension".to_string(),
        Value::String(skill.extension_name.clone()),
    );
    properties.insert("skillUri".to_string(), Value::String(skill.uri.clone()));
    SourceEntry {
        source_type: SourceType::Skill,
        name: published_name.to_string(),
        description: skill.description.clone(),
        content: String::new(),
        path: skill.uri.clone(),
        global: false,
        writable: false,
        supporting_files: skill
            .resources
            .as_ref()
            .map(|files| {
                files
                    .iter()
                    .filter(|file| file.uri != skill.uri)
                    .map(|file| file.uri.clone())
                    .collect()
            })
            .unwrap_or_default(),
        properties,
    }
}

pub fn is_mcp_skill(skill: &SourceEntry) -> bool {
    skill.path.starts_with("skill://")
}

pub fn listed_file<'a>(skill: &'a SkillRecord, uri: &str) -> Result<&'a SkillFile, SkillLoadError> {
    if skill.dynamic {
        return Err(SkillLoadError::DynamicSkill {
            name: skill.name.clone(),
        });
    }
    let Some(files) = skill.resources.as_deref() else {
        return Err(SkillLoadError::UnlistedFile {
            uri: uri.to_string(),
        });
    };
    files
        .iter()
        .find(|file| file.uri == uri)
        .ok_or_else(|| SkillLoadError::UnlistedFile {
            uri: uri.to_string(),
        })
}

/// Check raw file bytes against the digest advertised for `uri`.
///
/// A SKILL.md-only skill omits `resources` and is not digest-checked. A
/// dynamic skill has no stable digest and is rejected. A listed file with the
/// wrong digest is rejected. A URI that is not in the list is rejected.
pub fn verify_file(skill: &SkillRecord, uri: &str, bytes: &[u8]) -> Result<(), SkillLoadError> {
    if skill.dynamic {
        return Err(SkillLoadError::DynamicSkill {
            name: skill.name.clone(),
        });
    }
    // An omitted resource list means SKILL.md-only. The body has nothing to
    // check against. Any other URI is unlisted and must not be read.
    if skill.resources.is_none() {
        return if uri == skill.uri {
            Ok(())
        } else {
            Err(SkillLoadError::UnlistedFile {
                uri: uri.to_string(),
            })
        };
    }
    let file = listed_file(skill, uri)?;
    if !digest_matches(bytes, &file.digest) {
        return Err(SkillLoadError::DigestMismatch {
            uri: uri.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, description: &str) -> SourceEntry {
        SourceEntry {
            source_type: SourceType::Skill,
            name: name.to_string(),
            description: description.to_string(),
            path: format!("/skills/{name}"),
            writable: true,
            ..SourceEntry::default()
        }
    }

    fn record(name: &str, extension: &str) -> SkillRecord {
        SkillRecord {
            uri: format!("skill://{name}/SKILL.md"),
            name: name.to_string(),
            description: format!("{name} from {extension}"),
            extension_name: extension.to_string(),
            resources: Some(vec![SkillFile {
                uri: format!("skill://{name}/SKILL.md"),
                digest: sha256_digest(b"hello"),
                size: 5,
            }]),
            dynamic: false,
            namespace: extension.to_string(),
        }
    }

    #[test]
    fn filesystem_skill_hides_mcp_skill_with_the_same_name() {
        let merged = merge_skill_entries(
            vec![entry("billing", "local")],
            vec![record("billing", "stripe"), record("refunds", "stripe")],
        );
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].description, "local");
        assert_eq!(merged[1].name, "stripe::billing");
        assert_eq!(merged[2].name, "stripe::refunds");
        assert!(!merged[1].writable);
        assert_eq!(merged[2].path, "skill://refunds/SKILL.md");
    }

    #[test]
    fn two_mcp_servers_with_the_same_skill_keep_both_qualified_names() {
        let mut stripe = record("billing", "stripe");
        stripe.namespace = "billing.stripe.com".to_string();
        let mut other = record("billing", "other");
        other.namespace = "billing.other.test".to_string();
        let names: Vec<String> = merge_skill_entries(vec![], vec![stripe, other])
            .into_iter()
            .map(|skill| skill.name)
            .collect();
        assert_eq!(
            names,
            vec![
                "billing.stripe.com::billing".to_string(),
                "billing.other.test::billing".to_string()
            ]
        );
    }

    #[test]
    fn an_mcp_skill_is_always_qualified_even_when_it_is_the_only_one() {
        let merged = merge_skill_entries(vec![], vec![record("billing", "stripe")]);
        assert_eq!(merged[0].name, "stripe::billing");
    }

    #[test]
    fn skill_namespace_uses_the_url_host() {
        assert_eq!(
            skill_namespace("stripe", Some("https://billing.stripe.com/mcp")),
            "billing.stripe.com"
        );
        assert_eq!(skill_namespace("local-tools", None), "local-tools");
    }

    #[test]
    fn parse_skill_uri_rejects_traversal() {
        assert_eq!(
            parse_skill_uri("skill://git-workflow/SKILL.md"),
            Some(("git-workflow", "SKILL.md"))
        );
        assert_eq!(
            parse_skill_uri("skill://team/billing/SKILL.md"),
            Some(("team/billing", "SKILL.md"))
        );
        assert!(parse_skill_uri("skill://git-workflow/../SKILL.md").is_none());
        assert!(parse_skill_uri("file:///tmp/SKILL.md").is_none());
        assert!(parse_skill_uri("skill://git-workflow").is_none());
    }

    #[test]
    fn digest_mismatch_is_rejected_and_match_is_accepted() {
        let skill = record("billing", "stripe");
        assert!(verify_file(&skill, "skill://billing/SKILL.md", b"hello").is_ok());
        assert!(matches!(
            verify_file(&skill, "skill://billing/SKILL.md", b"nope"),
            Err(SkillLoadError::DigestMismatch { .. })
        ));
        assert!(matches!(
            verify_file(&skill, "skill://billing/secret.md", b"hello"),
            Err(SkillLoadError::UnlistedFile { .. })
        ));
    }

    #[test]
    fn list_payload_skips_a_bad_entry_and_keeps_the_valid_one() {
        let payload = serde_json::json!({
            "skills": [
                { "frontmatter": { "name": "broken" } },
                {
                    "uri": "skill://billing/SKILL.md",
                    "frontmatter": { "name": "other", "description": "nope" }
                },
                {
                    "uri": "skill://refunds/SKILL.md",
                    "frontmatter": { "name": "refunds", "description": "Keep this" }
                }
            ]
        });
        let (skills, _) = parse_skills_list(&payload, "stripe").unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "refunds");
    }

    #[test]
    fn list_payload_parses_a_valid_skill_and_cursor() {
        let payload = serde_json::json!({
            "skills": [{
                "uri": "skill://billing/SKILL.md",
                "frontmatter": { "name": "billing", "description": "Refund flow" },
                "resources": [{
                    "uri": "skill://billing/SKILL.md",
                    "digest": "sha256:abc",
                    "size": 4
                }]
            }],
            "nextCursor": "page-2"
        });
        let (skills, cursor) = parse_skills_list(&payload, "stripe").unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "Refund flow");
        assert_eq!(cursor.as_deref(), Some("page-2"));
    }
}
