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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerPreference {
    /// Run a review when a pull request is opened.
    PrOpen,
    /// Re-run on every push (PR opened + synchronize).
    OnEveryPush,
    /// Only run when someone explicitly mentions `@goose-copilot review`.
    ManualOnly,
}

impl Default for TriggerPreference {
    fn default() -> Self {
        Self::PrOpen
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerPermission {
    /// Any GitHub user who can see the PR can mention the bot.
    Anyone,
    /// Only repository collaborators with `write` access or higher.
    WriteAccess,
    /// A user-defined allowlist (allowlist storage not yet wired).
    SpecificUsers,
}

impl Default for TriggerPermission {
    fn default() -> Self {
        Self::Anyone
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewOutputStyle {
    /// Inline review comments with GitHub `suggestion` blocks where possible.
    Inline,
    /// One summary comment listing all findings, no inline annotations.
    Summary,
    /// Inline annotations + summary comment at the top of the review.
    Both,
}

impl Default for ReviewOutputStyle {
    fn default() -> Self {
        Self::Both
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewModelChoice {
    /// Reuse whatever model `goose` is globally configured with.
    Default,
    /// A separate review-specific model. Picker not yet wired.
    Custom,
}

impl Default for ReviewModelChoice {
    fn default() -> Self {
        Self::Default
    }
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

    // -- Routing prefs (forwarded to the switchboard) --
    #[serde(default = "default_true")]
    pub auto_review_on_pr_open: bool,
    #[serde(default)]
    pub trigger_preference: TriggerPreference,
    #[serde(default)]
    pub trigger_permission: TriggerPermission,
    #[serde(default)]
    pub allow_act_on_issues: bool,

    // -- Execution prefs (stay on goosed) --
    #[serde(default)]
    pub allow_commit_on_fix: bool,
    #[serde(default)]
    pub allow_open_new_prs: bool,
    #[serde(default)]
    pub exhaustive_review: bool,
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
            allow_commit_on_fix: false,
            allow_open_new_prs: false,
            exhaustive_review: false,
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
        // `review_model_choice == Custom` with empty provider/model is a
        // legal transitional state — the user has flipped the toggle but
        // hasn't picked a model yet. `run_goose_review` falls back to the
        // global default when either field is empty, so accepting this state
        // here keeps the save flow snappy and avoids spurious 400s.
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
        }
    }
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
        // Untouched fields fall back to defaults.
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
}
