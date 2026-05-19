use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Bump on every schema change. Older payloads with a missing/lower
/// `schema_version` still deserialize via field-level defaults; an explicit
/// *higher* version from the client is rejected by `validate`.
pub const SCHEMA_VERSION: u32 = 1;

/// Soft cap on custom_instructions length so users can't blow up the model's
/// context budget by accident. 16 KiB is generous (~4k tokens).
pub const MAX_CUSTOM_INSTRUCTIONS_BYTES: usize = 16 * 1024;

/// Hard cap on the specific-users allowlist so a single user can't push
/// thousands of entries into KV.
pub const MAX_ALLOWLIST_ENTRIES: usize = 256;

/// GitHub username max length (per GitHub's docs) plus a small margin.
pub const MAX_GITHUB_USERNAME_LEN: usize = 39;

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerPreference {
    /// Run a review when a pull request is opened.
    #[default]
    PrOpen,
    /// Re-run on every push (PR opened + synchronize).
    OnEveryPush,
    /// Only run when someone explicitly mentions `@goose-copilot review`.
    ManualOnly,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerPermission {
    /// Any GitHub user who can see the PR can mention the bot.
    #[default]
    Anyone,
    /// Only repository collaborators with `write` access or higher.
    WriteAccess,
    /// A user-defined allowlist (allowlist storage not yet wired).
    SpecificUsers,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewOutputStyle {
    /// Inline review comments with GitHub `suggestion` blocks where possible.
    Inline,
    /// One summary comment listing all findings, no inline annotations.
    Summary,
    /// Inline annotations + summary comment at the top of the review.
    #[default]
    Both,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewSeverity {
    /// Surface every finding the model produces. Highest signal *and* noise.
    Low,
    /// Drop low-severity findings; surface medium and above.
    #[default]
    Medium,
    /// Only high and critical findings.
    High,
    /// Only the most severe blocking issues.
    Critical,
}

impl ReviewSeverity {
    pub fn as_cli_flag(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewModelChoice {
    /// Reuse whatever model `goose` is globally configured with.
    #[default]
    Default,
    /// A separate review-specific model. Picker not yet wired.
    Custom,
}

/// Full Copilot preference set. The user sees this as one object in Desktop;
/// the backend splits it into "routing prefs" (shipped to the switchboard for
/// fast webhook decisions) and "execution prefs" (kept local on goosed).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CopilotPrefs {
    /// Set on the client; rejected if higher than the server understands.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    #[serde(default = "default_true")]
    pub auto_review_on_pr_open: bool,
    #[serde(default)]
    pub trigger_preference: TriggerPreference,
    #[serde(default)]
    pub trigger_permission: TriggerPermission,
    #[serde(default)]
    pub allow_act_on_issues: bool,
    /// GitHub usernames allowed to trigger the bot when `trigger_permission`
    /// is `SpecificUsers`. Case-insensitive on lookup; stored as user typed.
    #[serde(default)]
    pub specific_users_allowlist: Vec<String>,

    #[serde(default)]
    pub allow_commit_on_fix: bool,
    /// When `true` and the mention came from an issue, goosed pushes the
    /// agent's edits to a fresh branch and opens a PR linked to the issue.
    /// Ignored on PR mentions (they push to the existing PR branch).
    #[serde(default)]
    pub allow_open_new_prs: bool,
    #[serde(default)]
    pub review_severity: ReviewSeverity,
    #[serde(default)]
    pub custom_instructions: String,
    #[serde(default)]
    pub review_output_style: ReviewOutputStyle,
    #[serde(default)]
    pub review_model_choice: ReviewModelChoice,
    /// Provider name (e.g. "openai", "anthropic") used when
    /// `review_model_choice` is `Custom`. Ignored when `Default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_provider: Option<String>,
    /// Model name used when `review_model_choice` is `Custom`.
    /// Ignored when `Default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_model: Option<String>,
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn default_true() -> bool {
    true
}

impl Default for CopilotPrefs {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            auto_review_on_pr_open: true,
            trigger_preference: TriggerPreference::default(),
            trigger_permission: TriggerPermission::default(),
            allow_act_on_issues: false,
            specific_users_allowlist: Vec::new(),
            allow_commit_on_fix: false,
            allow_open_new_prs: false,
            review_severity: ReviewSeverity::default(),
            custom_instructions: String::new(),
            review_output_style: ReviewOutputStyle::default(),
            review_model_choice: ReviewModelChoice::default(),
            review_provider: None,
            review_model: None,
        }
    }
}

impl CopilotPrefs {
    /// Validate domain rules. Field-level type errors are caught by serde
    /// before this is called.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version > SCHEMA_VERSION {
            bail!(
                "schema_version {} is newer than this server understands ({})",
                self.schema_version,
                SCHEMA_VERSION
            );
        }
        if self.custom_instructions.len() > MAX_CUSTOM_INSTRUCTIONS_BYTES {
            bail!(
                "custom_instructions exceeds {} bytes",
                MAX_CUSTOM_INSTRUCTIONS_BYTES
            );
        }
        if self
            .custom_instructions
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
        {
            bail!("custom_instructions contains disallowed control characters");
        }
        if self.specific_users_allowlist.len() > MAX_ALLOWLIST_ENTRIES {
            bail!(
                "specific_users_allowlist exceeds {} entries",
                MAX_ALLOWLIST_ENTRIES
            );
        }
        for entry in &self.specific_users_allowlist {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                bail!("specific_users_allowlist contains a blank entry");
            }
            if trimmed.len() > MAX_GITHUB_USERNAME_LEN {
                bail!("allowlist entry `{trimmed}` exceeds {MAX_GITHUB_USERNAME_LEN} chars",);
            }
            if !is_valid_github_username(trimmed) {
                bail!("allowlist entry `{trimmed}` is not a valid GitHub username");
            }
        }
        Ok(())
    }

    /// The subset the switchboard needs for routing decisions. Kept narrow
    /// so we never leak execution prefs (custom instructions, model choice,
    /// etc.) to the Worker.
    pub fn routing_subset(&self) -> RoutingPrefs {
        RoutingPrefs {
            schema_version: self.schema_version,
            auto_review_on_pr_open: self.auto_review_on_pr_open,
            trigger_preference: self.trigger_preference.clone(),
            trigger_permission: self.trigger_permission.clone(),
            allow_act_on_issues: self.allow_act_on_issues,
            specific_users_allowlist: self.specific_users_allowlist.clone(),
        }
    }
}

/// GitHub usernames: alphanumeric and single hyphens, no leading/trailing hyphen.
fn is_valid_github_username(s: &str) -> bool {
    if s.is_empty() || s.starts_with('-') || s.ends_with('-') {
        return false;
    }
    if s.contains("--") {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The strict subset of `CopilotPrefs` shipped to the switchboard. Nothing
/// here is sensitive — it's behavior-shaping for webhook routing only.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RoutingPrefs {
    pub schema_version: u32,
    pub auto_review_on_pr_open: bool,
    pub trigger_preference: TriggerPreference,
    pub trigger_permission: TriggerPermission,
    pub allow_act_on_issues: bool,
    #[serde(default)]
    pub specific_users_allowlist: Vec<String>,
}

impl Default for RoutingPrefs {
    fn default() -> Self {
        CopilotPrefs::default().routing_subset()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip() {
        let p = CopilotPrefs::default();
        let json = serde_json::to_string(&p).unwrap();
        let p2: CopilotPrefs = serde_json::from_str(&json).unwrap();
        assert_eq!(p, p2);
    }

    #[test]
    fn missing_fields_get_defaults() {
        let p: CopilotPrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(p, CopilotPrefs::default());
    }

    #[test]
    fn partial_payload_merges_defaults() {
        let p: CopilotPrefs = serde_json::from_str(
            r#"{"auto_review_on_pr_open": false, "custom_instructions": "be nice"}"#,
        )
        .unwrap();
        assert!(!p.auto_review_on_pr_open);
        assert_eq!(p.custom_instructions, "be nice");
        assert_eq!(p.trigger_preference, TriggerPreference::PrOpen);
        assert_eq!(p.review_output_style, ReviewOutputStyle::Both);
    }

    #[test]
    fn kebab_case_enum_serialization() {
        let p = CopilotPrefs {
            trigger_preference: TriggerPreference::OnEveryPush,
            ..Default::default()
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(
            json.contains("\"trigger_preference\":\"on-every-push\""),
            "got {json}"
        );
    }

    #[test]
    fn validate_accepts_default() {
        CopilotPrefs::default().validate().unwrap();
    }

    #[test]
    fn validate_rejects_future_schema_version() {
        let p = CopilotPrefs {
            schema_version: SCHEMA_VERSION + 1,
            ..Default::default()
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn validate_rejects_oversized_instructions() {
        let p = CopilotPrefs {
            custom_instructions: "x".repeat(MAX_CUSTOM_INSTRUCTIONS_BYTES + 1),
            ..Default::default()
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn validate_rejects_control_chars() {
        let p = CopilotPrefs {
            custom_instructions: "evil \x07 bell".to_string(),
            ..Default::default()
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn validate_allows_newlines_and_tabs() {
        let p = CopilotPrefs {
            custom_instructions: "line one\nline two\twith tab\r\nend".to_string(),
            ..Default::default()
        };
        p.validate().unwrap();
    }

    #[test]
    fn routing_subset_drops_execution_prefs() {
        let p = CopilotPrefs {
            custom_instructions: "secret prompt".to_string(),
            review_model_choice: ReviewModelChoice::Custom,
            ..Default::default()
        };
        let routing = p.routing_subset();
        let json = serde_json::to_string(&routing).unwrap();
        assert!(!json.contains("custom_instructions"));
        assert!(!json.contains("review_model_choice"));
    }

    #[test]
    fn review_severity_cli_flag_strings() {
        assert_eq!(ReviewSeverity::Low.as_cli_flag(), "low");
        assert_eq!(ReviewSeverity::Medium.as_cli_flag(), "medium");
        assert_eq!(ReviewSeverity::High.as_cli_flag(), "high");
        assert_eq!(ReviewSeverity::Critical.as_cli_flag(), "critical");
    }

    #[test]
    fn validate_accepts_well_formed_allowlist() {
        let p = CopilotPrefs {
            specific_users_allowlist: vec!["octocat".into(), "abhi-jay".into(), "user123".into()],
            ..Default::default()
        };
        p.validate().unwrap();
    }

    #[test]
    fn validate_rejects_blank_allowlist_entry() {
        let p = CopilotPrefs {
            specific_users_allowlist: vec!["octocat".into(), "  ".into()],
            ..Default::default()
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn validate_rejects_malformed_username() {
        for bad in [
            "-leading",
            "trailing-",
            "double--hyphen",
            "has space",
            "has_underscore",
        ] {
            let p = CopilotPrefs {
                specific_users_allowlist: vec![bad.into()],
                ..Default::default()
            };
            assert!(p.validate().is_err(), "expected reject for `{bad}`");
        }
    }

    #[test]
    fn validate_rejects_oversize_allowlist() {
        let p = CopilotPrefs {
            specific_users_allowlist: (0..(MAX_ALLOWLIST_ENTRIES + 1))
                .map(|i| format!("user{i}"))
                .collect(),
            ..Default::default()
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn routing_subset_carries_allowlist() {
        let p = CopilotPrefs {
            specific_users_allowlist: vec!["octocat".into()],
            ..Default::default()
        };
        let routing = p.routing_subset();
        assert_eq!(
            routing.specific_users_allowlist,
            vec!["octocat".to_string()]
        );
    }
}
