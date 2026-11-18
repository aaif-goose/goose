//! What the user last approved from each MCP skill server, so a later approval
//! prompt can say what changed before changed content enters the model's context.
//!
//! Per-skill digests only prove that bytes match what the server says today; a
//! server can change body and digest together between sessions. The baseline
//! pins what was approved: for each `(extension, skill URI)` the SKILL.md
//! digest, frontmatter and supporting-file digests, and for each extension the
//! set of skills it listed at the last approval (its surface), so skills added
//! or removed since then are visible too.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tracing::warn;

use super::mcp::{SkillRecord, SkillResources};

const BASELINE_FILE: &str = "mcp_skill_approvals.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovedSkill {
    pub name: String,
    pub skill_md_digest: String,
    pub frontmatter: Map<String, Value>,
    /// Supporting files by URI, SKILL.md excluded.
    pub files: BTreeMap<String, String>,
    pub approved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct ExtensionBaseline {
    skills: BTreeMap<String, ApprovedSkill>,
    /// Skill URIs the server listed when the user last approved one of its skills.
    surface: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    Unchanged,
    New,
    Review,
    Escalate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// SKILL.md changed under identical frontmatter, so its body did.
    InstructionsRewritten,
    /// SKILL.md and its frontmatter both changed; the body may have too.
    FrontmatterChanged {
        fields: Vec<String>,
    },
    AllowedToolsExpanded {
        added: Vec<String>,
    },
    SupportingFilesChanged {
        added: Vec<String>,
        changed: Vec<String>,
        removed: Vec<String>,
    },
}

impl Finding {
    fn grade(&self) -> Grade {
        match self {
            Self::InstructionsRewritten | Self::AllowedToolsExpanded { .. } => Grade::Escalate,
            Self::FrontmatterChanged { .. } | Self::SupportingFilesChanged { .. } => Grade::Review,
        }
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InstructionsRewritten => write!(f, "instructions rewritten"),
            Self::FrontmatterChanged { fields } => write!(
                f,
                "frontmatter changed ({}); the instructions may have changed too",
                fields.join(", ")
            ),
            Self::AllowedToolsExpanded { added } => {
                write!(f, "allowed-tools expanded: {}", added.join(", "))
            }
            Self::SupportingFilesChanged {
                added,
                changed,
                removed,
            } => {
                let mut parts = Vec::new();
                for (label, files) in [("added", added), ("changed", changed), ("removed", removed)]
                {
                    if !files.is_empty() {
                        parts.push(format!("{label} {}", files.join(", ")));
                    }
                }
                write!(f, "supporting files {}", parts.join("; "))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkillChange {
    pub grade: Grade,
    pub findings: Vec<Finding>,
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SurfaceChange {
    /// Names of skills listed now that were not listed at the last approval.
    pub added: Vec<String>,
    /// Names of skills listed at the last approval that are no longer listed.
    pub removed: Vec<String>,
}

impl SurfaceChange {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

fn skill_md_digest(skill: &SkillRecord) -> Option<&str> {
    let SkillResources::Files(files) = &skill.resources else {
        return None;
    };
    files
        .iter()
        .find(|file| file.uri == skill.uri)
        .map(|file| file.digest.as_str())
}

fn supporting_files(skill: &SkillRecord) -> BTreeMap<String, String> {
    match &skill.resources {
        SkillResources::Files(files) => files
            .iter()
            .filter(|file| file.uri != skill.uri)
            .map(|file| (file.uri.clone(), file.digest.clone()))
            .collect(),
        SkillResources::Dynamic => BTreeMap::new(),
    }
}

/// `allowed-tools` is a space-delimited string in the Agent Skills format;
/// some servers send a list.
fn allowed_tools(frontmatter: &Map<String, Value>) -> BTreeSet<String> {
    match frontmatter.get("allowed-tools") {
        Some(Value::String(tools)) => tools.split_whitespace().map(str::to_string).collect(),
        Some(Value::Array(tools)) => tools
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn changed_fields(before: &Map<String, Value>, after: &Map<String, Value>) -> Vec<String> {
    let keys: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    keys.into_iter()
        .filter(|key| before.get(*key) != after.get(*key))
        .cloned()
        .collect()
}

pub fn grade(approved: Option<&ApprovedSkill>, current: &SkillRecord) -> SkillChange {
    let Some(approved) = approved else {
        return SkillChange {
            grade: Grade::New,
            findings: Vec::new(),
            approved_at: None,
        };
    };
    let mut findings = Vec::new();
    if skill_md_digest(current) != Some(approved.skill_md_digest.as_str()) {
        let fields = changed_fields(&approved.frontmatter, &current.frontmatter);
        if fields.is_empty() {
            findings.push(Finding::InstructionsRewritten);
        } else {
            findings.push(Finding::FrontmatterChanged { fields });
        }
        let before = allowed_tools(&approved.frontmatter);
        let added: Vec<String> = allowed_tools(&current.frontmatter)
            .difference(&before)
            .cloned()
            .collect();
        if !added.is_empty() {
            findings.push(Finding::AllowedToolsExpanded { added });
        }
    }
    let files = supporting_files(current);
    let added: Vec<String> = files
        .keys()
        .filter(|uri| !approved.files.contains_key(*uri))
        .cloned()
        .collect();
    let changed: Vec<String> = files
        .iter()
        .filter(|(uri, digest)| approved.files.get(*uri).is_some_and(|old| old != *digest))
        .map(|(uri, _)| uri.clone())
        .collect();
    let removed: Vec<String> = approved
        .files
        .keys()
        .filter(|uri| !files.contains_key(*uri))
        .cloned()
        .collect();
    if !(added.is_empty() && changed.is_empty() && removed.is_empty()) {
        findings.push(Finding::SupportingFilesChanged {
            added,
            changed,
            removed,
        });
    }
    SkillChange {
        grade: findings
            .iter()
            .map(Finding::grade)
            .max()
            .unwrap_or(Grade::Unchanged),
        findings,
        approved_at: Some(approved.approved_at),
    }
}

/// Persisted next to `permission.yaml`. Keyed by the host-assigned extension
/// name, so a server re-added under the same name is compared with what was
/// approved from it before.
pub struct SkillBaselineStore {
    path: PathBuf,
    extensions: RwLock<BTreeMap<String, ExtensionBaseline>>,
}

impl SkillBaselineStore {
    pub fn new(config_dir: &Path) -> Self {
        let path = config_dir.join(BASELINE_FILE);
        let extensions = match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|error| {
                warn!(%error, path = %path.display(), "ignoring unreadable MCP skill approvals");
                BTreeMap::new()
            }),
            Err(_) => BTreeMap::new(),
        };
        Self {
            path,
            extensions: RwLock::new(extensions),
        }
    }

    pub fn change(&self, current: &SkillRecord) -> SkillChange {
        let extensions = self.extensions.read().expect("baseline lock poisoned");
        let approved = extensions
            .get(&current.extension_name)
            .and_then(|extension| extension.skills.get(&current.uri));
        grade(approved, current)
    }

    /// Empty until a skill from the extension has been approved once: the
    /// first approval has nothing to compare the listing against.
    pub fn surface_change(&self, extension_name: &str, listed: &[SkillRecord]) -> SurfaceChange {
        let extensions = self.extensions.read().expect("baseline lock poisoned");
        let Some(baseline) = extensions
            .get(extension_name)
            .filter(|baseline| !baseline.surface.is_empty())
        else {
            return SurfaceChange::default();
        };
        let listed: Vec<&SkillRecord> = listed
            .iter()
            .filter(|skill| skill.extension_name == extension_name)
            .collect();
        let listed_uris: BTreeSet<&str> = listed.iter().map(|skill| skill.uri.as_str()).collect();
        SurfaceChange {
            added: listed
                .iter()
                .filter(|skill| !baseline.surface.contains(&skill.uri))
                .map(|skill| skill.name.clone())
                .collect(),
            removed: baseline
                .surface
                .iter()
                .filter(|uri| !listed_uris.contains(uri.as_str()))
                .map(|uri| {
                    baseline
                        .skills
                        .get(uri)
                        .map_or_else(|| uri.clone(), |skill| skill.name.clone())
                })
                .collect(),
        }
    }

    /// Records `skill` as approved. When `listed` holds the server's current
    /// listing, it becomes the extension's surface: the approval prompt showed
    /// what it added and removed.
    pub fn approve(&self, skill: &SkillRecord, listed: Option<&[SkillRecord]>) {
        let Some(digest) = skill_md_digest(skill) else {
            return;
        };
        let mut extensions = self.extensions.write().expect("baseline lock poisoned");
        let baseline = extensions.entry(skill.extension_name.clone()).or_default();
        baseline.skills.insert(
            skill.uri.clone(),
            ApprovedSkill {
                name: skill.name.clone(),
                skill_md_digest: digest.to_string(),
                frontmatter: skill.frontmatter.clone(),
                files: supporting_files(skill),
                approved_at: Utc::now(),
            },
        );
        if let Some(listed) = listed {
            baseline.surface = listed
                .iter()
                .filter(|listed| listed.extension_name == skill.extension_name)
                .map(|listed| listed.uri.clone())
                .collect();
        }
        baseline.surface.insert(skill.uri.clone());
        // Written under the lock so a slower writer cannot persist an older state.
        let written = serde_json::to_string_pretty(&*extensions)
            .map_err(std::io::Error::other)
            .and_then(|json| {
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let temp = self.path.with_extension("json.tmp");
                std::fs::write(&temp, json)?;
                std::fs::rename(&temp, &self.path)
            });
        if let Err(error) = written {
            warn!(%error, path = %self.path.display(), "failed to persist MCP skill approval");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::SkillResource;

    fn skill(skill_md: &str, frontmatter: Value, files: &[(&str, &str)]) -> SkillRecord {
        let uri = "skill://acme/billing/SKILL.md";
        let mut resources = vec![SkillResource::from_bytes(uri, skill_md.as_bytes())];
        resources.extend(
            files
                .iter()
                .map(|(uri, body)| SkillResource::from_bytes(*uri, body.as_bytes())),
        );
        SkillRecord {
            uri: uri.to_string(),
            name: "billing".to_string(),
            description: "Billing".to_string(),
            frontmatter: frontmatter.as_object().unwrap().clone(),
            extension_name: "acme".to_string(),
            resources: SkillResources::Files(resources),
        }
    }

    fn base() -> Value {
        serde_json::json!({ "name": "billing", "description": "Billing" })
    }

    fn approved(record: &SkillRecord) -> ApprovedSkill {
        let store = SkillBaselineStore::new(tempfile::tempdir().unwrap().path());
        store.approve(record, None);
        let extensions = store.extensions.read().unwrap();
        extensions["acme"].skills[&record.uri].clone()
    }

    const GUIDE: &str = "skill://acme/billing/GUIDE.md";

    #[test]
    fn grades_follow_what_changed() {
        let original = skill("v1", base(), &[(GUIDE, "guide")]);
        let baseline = approved(&original);

        assert_eq!(grade(None, &original).grade, Grade::New);
        assert_eq!(grade(Some(&baseline), &original).grade, Grade::Unchanged);

        let rewritten = grade(Some(&baseline), &skill("v2", base(), &[(GUIDE, "guide")]));
        assert_eq!(rewritten.grade, Grade::Escalate);
        assert_eq!(rewritten.findings, [Finding::InstructionsRewritten]);

        let mut with_tools = base();
        with_tools["allowed-tools"] = Value::String("Bash Read".to_string());
        let widened = grade(
            Some(&baseline),
            &skill("v2", with_tools, &[(GUIDE, "guide")]),
        );
        assert_eq!(widened.grade, Grade::Escalate);
        assert!(widened.findings.contains(&Finding::AllowedToolsExpanded {
            added: vec!["Bash".to_string(), "Read".to_string()]
        }));

        let mut described = base();
        described["description"] = Value::String("Billing, reworded".to_string());
        let metadata = grade(
            Some(&baseline),
            &skill("v2", described, &[(GUIDE, "guide")]),
        );
        assert_eq!(metadata.grade, Grade::Review);
        assert_eq!(
            metadata.findings,
            [Finding::FrontmatterChanged {
                fields: vec!["description".to_string()]
            }]
        );

        let files = grade(
            Some(&baseline),
            &skill("v1", base(), &[(GUIDE, "new guide")]),
        );
        assert_eq!(files.grade, Grade::Review);
        assert_eq!(
            files.findings,
            [Finding::SupportingFilesChanged {
                added: vec![],
                changed: vec![GUIDE.to_string()],
                removed: vec![],
            }]
        );
    }

    #[test]
    fn narrowing_allowed_tools_is_not_an_expansion() {
        let mut wide = base();
        wide["allowed-tools"] = serde_json::json!(["Bash", "Read"]);
        let baseline = approved(&skill("v1", wide, &[]));
        let mut narrow = base();
        narrow["allowed-tools"] = serde_json::json!(["Read"]);
        let change = grade(Some(&baseline), &skill("v2", narrow, &[]));
        assert_eq!(change.grade, Grade::Review);
    }

    #[test]
    fn surface_reports_added_and_removed_skills_after_the_first_approval() {
        let dir = tempfile::tempdir().unwrap();
        let store = SkillBaselineStore::new(dir.path());
        let billing = skill("v1", base(), &[]);
        let mut refunds = billing.clone();
        refunds.uri = "skill://acme/refunds/SKILL.md".to_string();
        refunds.name = "refunds".to_string();
        let mut sleeper = billing.clone();
        sleeper.uri = "skill://acme/sleeper/SKILL.md".to_string();
        sleeper.name = "sleeper".to_string();

        assert!(store
            .surface_change("acme", &[billing.clone(), refunds.clone()])
            .is_empty());
        store.approve(&billing, Some(&[billing.clone(), refunds.clone()]));

        let change = store.surface_change("acme", &[billing.clone(), sleeper.clone()]);
        assert_eq!(change.added, ["sleeper"]);
        assert_eq!(change.removed, ["skill://acme/refunds/SKILL.md"]);
    }

    #[test]
    fn approvals_persist_across_stores() {
        let dir = tempfile::tempdir().unwrap();
        let record = skill("v1", base(), &[]);
        SkillBaselineStore::new(dir.path()).approve(&record, None);
        let reopened = SkillBaselineStore::new(dir.path());
        assert_eq!(reopened.change(&record).grade, Grade::Unchanged);
        assert_eq!(
            reopened.change(&skill("v2", base(), &[])).findings,
            [Finding::InstructionsRewritten]
        );
    }
}
