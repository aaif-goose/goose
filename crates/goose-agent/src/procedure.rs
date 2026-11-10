//! Executable finite-state control for long-horizon procedures.
//!
//! Implements the core idea of "Keeping LLM Agents on Procedure:
//! Executable Finite-State Control for Long-Horizon Instruction Fidelity"
//! (arXiv:2611.10884): a procedure (skill or SOP) stated once at the start
//! of a run is compiled into an executable finite-state machine whose state
//! lives in the runtime, not in the model's context, and is enforced at
//! three points:
//!
//! 1. **Tool calls** — only the tools listed for the current state may run.
//!    Anything else is denied with a message naming the tools that are
//!    allowed in the current state, so the model can self-correct.
//! 2. **State transitions** — the state advances only on a transition the
//!    procedure defines, driven by the tool that ran and optionally by a
//!    simple match on its result (contains/equals a string, or success /
//!    failure). A call with no matching transition leaves the state
//!    unchanged.
//! 3. **End of turn** — ending the turn while the procedure is in a
//!    non-terminal state is denied, so the model keeps working the
//!    procedure instead of drifting off to a shortcut finish.
//!
//! No model call is involved anywhere: the FSM is a pure, deterministic
//! state machine over tool names and result text.
//!
//! A procedure is loaded from a JSON file named by the
//! `GOOSE_PROCEDURE_FILE` environment variable (see
//! [`ProcedureConfig::load_from_file`]). Unset means the feature is off. A
//! set-but-unloadable file installs a deny-all runtime instead of failing
//! open, so a typo'd config is loud rather than silently ignored.
//!
//! Run scoping: the runtime tracks the id of the kickoff message (the user
//! message that started the current run, per
//! [`crate::operation::messages_since_kickoff`]). A new kickoff restarts
//! the procedure from its initial state, so state persists across every
//! turn of one run — including turns resumed after a stop-hook denial —
//! and a fresh user request starts the procedure over instead of
//! inheriting a stale state.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use goose_provider_types::conversation::message::ToolResult;
use goose_provider_types::conversation::Conversation;
use rmcp::model::{CallToolResult, ContentBlock, ResourceContents};
use serde::{Deserialize, Serialize};

use crate::operation::messages_since_kickoff;

/// Id of the kickoff message of the current run, if the conversation has
/// one. The procedure runtime uses it to tell a new run (restart the
/// procedure) from a continued turn of the same run (keep the state).
pub fn kickoff_id(conversation: &Conversation) -> Option<String> {
    messages_since_kickoff(conversation)
        .ok()?
        .first()?
        .id
        .clone()
}

/// A tool-name glob: `*` matches everything, a trailing `*` matches a
/// prefix, anything else matches exactly. Same semantics as the spillway
/// target rules, so the two guards read the same way in config.
pub fn matches_tool(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        true
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        pattern == name
    }
}

/// A procedure definition: states, the tools each state allows, the
/// transitions between them, and which states are terminal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProcedureConfig {
    /// Human-readable name, used in denial messages. Optional.
    #[serde(default)]
    pub name: Option<String>,
    /// State the procedure starts in when a run begins.
    #[serde(default)]
    pub initial: String,
    /// States by name.
    #[serde(default)]
    pub states: BTreeMap<String, ProcedureState>,
    /// Set only by [`ProcedureConfig::unavailable`]: the config file could
    /// not be loaded, so every tool call is denied until it is fixed.
    #[serde(skip)]
    pub unavailable_reason: Option<String>,
}

/// One state of a procedure: the tools the model may call while in it, the
/// transitions out of it, and whether reaching it completes the procedure.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProcedureState {
    /// Tool names or `prefix*` globs allowed in this state. A non-terminal
    /// state must list at least one; a terminal state may list none, which
    /// means no further tool calls — the run should end.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Transitions out of this state, tried in order; the first whose
    /// tool and result match fires.
    #[serde(default)]
    pub transitions: Vec<Transition>,
    /// Whether reaching this state completes the procedure (the turn may
    /// end here).
    #[serde(default)]
    pub terminal: bool,
}

/// One transition: when `on_tool` runs (and, if given, its result matches
/// `on_result`), move to `to`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    /// Tool name or `prefix*` glob that triggers this transition.
    pub on_tool: String,
    /// Optional match on the tool's result. All fields present must match.
    #[serde(default)]
    pub on_result: Option<ResultMatch>,
    /// State to move to.
    pub to: String,
}

/// A simple match on a tool result: substring, exact text, or success /
/// failure. The paper's FSM advances on tool outcomes; these three cover
/// the outcomes a procedure author can observe without a model call.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResultMatch {
    /// Result text contains this string (case-insensitive).
    #[serde(default)]
    pub contains: Option<String>,
    /// Result text equals this string exactly, after trimming whitespace.
    #[serde(default)]
    pub equals: Option<String>,
    /// Whether the call succeeded (`Ok` and not flagged `is_error`).
    /// `false` matches errors and error-flagged results.
    #[serde(default)]
    pub success: Option<bool>,
}

impl ResultMatch {
    fn matches(&self, text: &str, success: bool) -> bool {
        if let Some(needle) = &self.contains {
            if !text.to_lowercase().contains(&needle.to_lowercase()) {
                return false;
            }
        }
        if let Some(expected) = &self.equals {
            if text.trim() != expected.trim() {
                return false;
            }
        }
        if let Some(expected) = self.success {
            if success != expected {
                return false;
            }
        }
        true
    }
}

impl ProcedureConfig {
    pub fn from_json(json: &str) -> Result<Self> {
        let config: Self = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read procedure config file {}", path.display()))?;
        Self::from_json(&json)
            .with_context(|| format!("invalid procedure config in {}", path.display()))
    }

    /// A config that denies every tool call. Used when a configured config
    /// file cannot be loaded: the failure is loud (every denial names the
    /// load error) instead of silently fail-open. Ending the turn is still
    /// allowed, so the model can report the broken config and stop instead
    /// of being trapped by the stop hook.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            unavailable_reason: Some(reason.into()),
            ..Self::default()
        }
    }

    pub fn is_unavailable(&self) -> bool {
        self.unavailable_reason.is_some()
    }

    fn validate(&self) -> Result<()> {
        if self.initial.is_empty() {
            return Err(anyhow!("procedure config has no `initial` state"));
        }
        if !self.states.contains_key(&self.initial) {
            return Err(anyhow!(
                "procedure config names unknown initial state {:?}",
                self.initial
            ));
        }
        if !self.states.values().any(|state| state.terminal) {
            return Err(anyhow!(
                "procedure config has no terminal state; at least one state must set `terminal: true`"
            ));
        }
        for (name, state) in &self.states {
            if name.is_empty() {
                return Err(anyhow!("procedure config has a state with an empty name"));
            }
            if !state.terminal && state.tools.is_empty() {
                return Err(anyhow!(
                    "non-terminal state {name:?} lists no tools; the procedure could never leave it"
                ));
            }
            for tool in &state.tools {
                if tool.is_empty() {
                    return Err(anyhow!("state {name:?} lists an empty tool pattern"));
                }
            }
            for transition in &state.transitions {
                if transition.on_tool.is_empty() {
                    return Err(anyhow!(
                        "state {name:?} has a transition with an empty `on_tool`"
                    ));
                }
                if !self.states.contains_key(&transition.to) {
                    return Err(anyhow!(
                        "state {name:?} has a transition to unknown state {:?}",
                        transition.to
                    ));
                }
                // A transition can only fire for a tool the state allows;
                // otherwise it is dead config. Either direction of glob
                // match counts: a `prefix*` tools entry covers a literal
                // `on_tool`, and a `prefix*` `on_tool` covers literal
                // tools entries.
                if !state.tools.iter().any(|tool| {
                    matches_tool(tool, &transition.on_tool)
                        || matches_tool(&transition.on_tool, tool)
                }) {
                    return Err(anyhow!(
                        "state {name:?} has a transition on {:?}, which is not among its allowed tools {:?}",
                        transition.on_tool,
                        state.tools
                    ));
                }
                if let Some(result_match) = &transition.on_result {
                    if result_match.contains.is_none()
                        && result_match.equals.is_none()
                        && result_match.success.is_none()
                    {
                        return Err(anyhow!(
                            "state {name:?} has a transition with an empty `on_result`; specify at least one of `contains`, `equals`, `success`"
                        ));
                    }
                    if result_match.contains.as_deref() == Some("") {
                        return Err(anyhow!(
                            "state {name:?} has a transition with an empty `contains`"
                        ));
                    }
                    if result_match.equals.as_deref() == Some("") {
                        return Err(anyhow!(
                            "state {name:?} has a transition with an empty `equals`"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn display_name(&self) -> String {
        self.name
            .as_deref()
            .map(|name| format!("procedure {name:?}"))
            .unwrap_or_else(|| "the procedure".to_string())
    }
}

/// The executable state of one procedure: the config plus the current
/// state. State lives here, in the runtime — never in the prompt — and the
/// model is never asked to track it.
pub struct ProcedureRuntime {
    config: Arc<ProcedureConfig>,
    current: String,
    last_kickoff: Option<String>,
}

impl ProcedureRuntime {
    pub fn new(config: Arc<ProcedureConfig>) -> Self {
        let current = config.initial.clone();
        Self {
            config,
            current,
            last_kickoff: None,
        }
    }

    pub fn current_state(&self) -> &str {
        &self.current
    }

    pub fn is_terminal(&self) -> bool {
        self.config
            .states
            .get(&self.current)
            .is_some_and(|state| state.terminal)
    }

    pub fn is_unavailable(&self) -> bool {
        self.config.is_unavailable()
    }

    /// A new kickoff (a fresh user request) restarts the procedure from
    /// its initial state; a continued turn of the same run keeps the
    /// state. The first call for a run is a no-op reset, since the
    /// runtime starts at the initial state.
    fn sync_run(&mut self, kickoff_id: Option<&str>) {
        let key = kickoff_id.unwrap_or("").to_string();
        if self.last_kickoff.as_deref() != Some(key.as_str()) {
            self.last_kickoff = Some(key);
            self.current = self.config.initial.clone();
        }
    }

    /// Enforcement point 1 (tool calls): is `tool` allowed in the current
    /// state? Denials name the current state and its allowed tools so the
    /// model can retry with a permitted tool.
    pub fn check_tool(&mut self, kickoff_id: Option<&str>, tool: &str) -> Result<(), String> {
        if let Some(reason) = &self.config.unavailable_reason {
            return Err(reason.clone());
        }
        self.sync_run(kickoff_id);
        let Some(state) = self.config.states.get(&self.current) else {
            return Err(format!(
                "procedure control is in unknown state {:?}; refusing tool {tool:?} until the procedure config is fixed",
                self.current
            ));
        };
        if state
            .tools
            .iter()
            .any(|pattern| matches_tool(pattern, tool))
        {
            return Ok(());
        }
        Err(self.deny_message(tool, state))
    }

    fn deny_message(&self, tool: &str, state: &ProcedureState) -> String {
        let allowed = if state.tools.is_empty() {
            "no further tools — the procedure is complete, end your turn".to_string()
        } else {
            state.tools.join(", ")
        };
        format!(
            "procedure control refused tool {tool:?}: {} is in state {:?}, which only allows: {allowed}. Use one of those tools instead.",
            self.config.display_name(),
            self.current
        )
    }

    /// Enforcement point 2 (state transitions): advance the state if the
    /// first transition of the current state matches this tool and its
    /// result. Returns the new state when a transition fired. A call with
    /// no matching transition leaves the state unchanged.
    pub fn advance(
        &mut self,
        kickoff_id: Option<&str>,
        tool: &str,
        result: &ToolResult<CallToolResult>,
    ) -> Option<String> {
        if self.config.is_unavailable() {
            return None;
        }
        self.sync_run(kickoff_id);
        let (text, success) = result_outcome(result);
        let state = self.config.states.get(&self.current)?;
        let transition = state.transitions.iter().find(|transition| {
            if !matches_tool(&transition.on_tool, tool) {
                return false;
            }
            match &transition.on_result {
                Some(result_match) => result_match.matches(&text, success),
                None => true,
            }
        })?;
        self.current = transition.to.clone();
        Some(self.current.clone())
    }

    /// Enforcement point 3 (end of turn): may the turn end now? Only from
    /// a terminal state — or when the config is unavailable, so the model
    /// can report the broken config and stop instead of being trapped.
    pub fn finish_check(&mut self, kickoff_id: Option<&str>) -> Result<(), String> {
        if self.config.is_unavailable() {
            return Ok(());
        }
        self.sync_run(kickoff_id);
        if self.is_terminal() {
            return Ok(());
        }
        let allowed = self
            .config
            .states
            .get(&self.current)
            .map(|state| state.tools.join(", "))
            .unwrap_or_default();
        Err(format!(
            "{} is incomplete: still in state {:?} (no terminal state reached). Continue the procedure with one of: {allowed}.",
            self.config.display_name(),
            self.current
        ))
    }
}

/// Text and success of a tool result, for result matching. Text is the
/// concatenated text blocks (plus text resource contents); errors
/// contribute their message and count as failures.
fn result_outcome(result: &ToolResult<CallToolResult>) -> (String, bool) {
    match result {
        Ok(result) => (result_text(result), result.is_error != Some(true)),
        Err(error) => (error.message.to_string(), false),
    }
}

fn result_text(result: &CallToolResult) -> String {
    let mut parts = Vec::new();
    for content in &result.content {
        match content {
            ContentBlock::Text(text) => parts.push(text.text.clone()),
            ContentBlock::Resource(resource) => {
                if let ResourceContents::TextResourceContents { text, .. } = &resource.resource {
                    parts.push(text.clone());
                }
            }
            _ => {}
        }
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose_provider_types::conversation::message::Message;
    use serde_json::json;

    const EXAMPLE: &str = r#"{
        "name": "account-processing",
        "initial": "triage",
        "states": {
            "triage": {
                "tools": ["accounts__list", "accounts__read"],
                "transitions": [
                    {"on_tool": "accounts__read", "on_result": {"contains": "verified"}, "to": "verify"},
                    {"on_tool": "accounts__read", "to": "triage"}
                ]
            },
            "verify": {
                "tools": ["accounts__verify"],
                "transitions": [
                    {"on_tool": "accounts__verify", "on_result": {"success": true}, "to": "decide"}
                ]
            },
            "decide": {
                "tools": ["accounts__decide"],
                "terminal": true
            }
        }
    }"#;

    fn example_config() -> ProcedureConfig {
        ProcedureConfig::from_json(EXAMPLE).expect("example config is valid")
    }

    fn example_runtime() -> ProcedureRuntime {
        ProcedureRuntime::new(Arc::new(example_config()))
    }

    fn text_result(text: &str) -> ToolResult<CallToolResult> {
        Ok(CallToolResult::success(vec![ContentBlock::text(
            text.to_string(),
        )]))
    }

    fn error_result() -> ToolResult<CallToolResult> {
        Ok(CallToolResult::error(vec![ContentBlock::text(
            "verification failed".to_string(),
        )]))
    }

    fn transport_error() -> ToolResult<CallToolResult> {
        Err(rmcp::model::ErrorData::internal_error(
            "connection reset".to_string(),
            None,
        ))
    }

    #[test]
    fn example_config_loads() {
        let config = example_config();
        assert_eq!(config.name.as_deref(), Some("account-processing"));
        assert_eq!(config.initial, "triage");
        assert_eq!(config.states.len(), 3);
        assert!(config.states["decide"].terminal);
        assert!(!config.states["triage"].terminal);
    }

    #[test]
    fn load_from_file_round_trip() {
        let path = std::env::temp_dir().join("goose-procedure-test.json");
        std::fs::write(&path, EXAMPLE).unwrap();
        let config = ProcedureConfig::load_from_file(&path).unwrap();
        assert_eq!(config, example_config());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn load_from_file_missing_errors() {
        let path = std::env::temp_dir().join("goose-procedure-does-not-exist.json");
        assert!(ProcedureConfig::load_from_file(&path).is_err());
    }

    #[test]
    fn malformed_json_rejected() {
        assert!(ProcedureConfig::from_json("{not json").is_err());
        assert!(ProcedureConfig::from_json("{\"initial\": 42}").is_err());
    }

    #[test]
    fn unknown_initial_state_rejected() {
        let json = json!({
            "initial": "nope",
            "states": {"a": {"tools": ["x"], "terminal": true}}
        });
        assert!(ProcedureConfig::from_json(&json.to_string()).is_err());
    }

    #[test]
    fn missing_initial_rejected() {
        let json = json!({
            "states": {"a": {"tools": ["x"], "terminal": true}}
        });
        assert!(ProcedureConfig::from_json(&json.to_string()).is_err());
    }

    #[test]
    fn unknown_transition_target_rejected() {
        let json = json!({
            "initial": "a",
            "states": {
                "a": {"tools": ["x"], "transitions": [{"on_tool": "x", "to": "nope"}]},
                "b": {"tools": ["y"], "terminal": true}
            }
        });
        assert!(ProcedureConfig::from_json(&json.to_string()).is_err());
    }

    #[test]
    fn no_terminal_state_rejected() {
        let json = json!({
            "initial": "a",
            "states": {
                "a": {"tools": ["x"], "transitions": [{"on_tool": "x", "to": "b"}]},
                "b": {"tools": ["y"], "transitions": [{"on_tool": "y", "to": "a"}]}
            }
        });
        assert!(ProcedureConfig::from_json(&json.to_string()).is_err());
    }

    #[test]
    fn non_terminal_state_without_tools_rejected() {
        let json = json!({
            "initial": "a",
            "states": {
                "a": {"tools": [], "transitions": [{"on_tool": "x", "to": "b"}]},
                "b": {"tools": ["y"], "terminal": true}
            }
        });
        assert!(ProcedureConfig::from_json(&json.to_string()).is_err());
    }

    #[test]
    fn transition_on_tool_outside_state_tools_rejected() {
        let json = json!({
            "initial": "a",
            "states": {
                "a": {"tools": ["x"], "transitions": [{"on_tool": "z", "to": "b"}]},
                "b": {"tools": ["y"], "terminal": true}
            }
        });
        assert!(ProcedureConfig::from_json(&json.to_string()).is_err());
    }

    #[test]
    fn empty_result_match_rejected() {
        let json = json!({
            "initial": "a",
            "states": {
                "a": {"tools": ["x"], "transitions": [{"on_tool": "x", "on_result": {}, "to": "b"}]},
                "b": {"tools": ["y"], "terminal": true}
            }
        });
        assert!(ProcedureConfig::from_json(&json.to_string()).is_err());
    }

    #[test]
    fn glob_matching() {
        assert!(matches_tool("*", "anything__at_all"));
        assert!(matches_tool("accounts__*", "accounts__read"));
        assert!(!matches_tool("accounts__*", "billing__read"));
        assert!(matches_tool("accounts__read", "accounts__read"));
        assert!(!matches_tool("accounts__read", "accounts__list"));
    }

    #[test]
    fn check_tool_allows_listed_tool() {
        let mut runtime = example_runtime();
        assert!(runtime.check_tool(Some("run-1"), "accounts__list").is_ok());
        assert!(runtime.check_tool(Some("run-1"), "accounts__read").is_ok());
    }

    #[test]
    fn check_tool_allows_globbed_tool() {
        let config = ProcedureConfig::from_json(
            &json!({
                "initial": "a",
                "states": {
                    "a": {"tools": ["shell__*"], "transitions": [{"on_tool": "shell__exec", "to": "b"}]},
                    "b": {"tools": ["x"], "terminal": true}
                }
            })
            .to_string(),
        )
        .unwrap();
        let mut runtime = ProcedureRuntime::new(Arc::new(config));
        assert!(runtime.check_tool(Some("run-1"), "shell__exec").is_ok());
        assert!(runtime.check_tool(Some("run-1"), "shell__bash").is_ok());
    }

    #[test]
    fn check_tool_denies_unlisted_tool_with_state_and_allowed_tools() {
        let mut runtime = example_runtime();
        let error = runtime
            .check_tool(Some("run-1"), "accounts__decide")
            .unwrap_err();
        assert!(error.contains("triage"), "{error}");
        assert!(error.contains("accounts__list"), "{error}");
        assert!(error.contains("accounts__read"), "{error}");
        assert!(error.contains("accounts__decide"), "{error}");
    }

    #[test]
    fn unavailable_config_denies_every_tool() {
        let config = ProcedureConfig::unavailable("boom: config file missing");
        let mut runtime = ProcedureRuntime::new(Arc::new(config));
        assert!(runtime.is_unavailable());
        assert_eq!(
            runtime.check_tool(Some("run-1"), "anything").unwrap_err(),
            "boom: config file missing"
        );
    }

    #[test]
    fn advance_on_tool_name() {
        let mut runtime = example_runtime();
        // accounts__read without "verified" in the result: second
        // transition fires, a self-loop back to triage.
        let moved = runtime.advance(Some("run-1"), "accounts__read", &text_result("pending"));
        assert_eq!(moved.as_deref(), Some("triage"));
        assert_eq!(runtime.current_state(), "triage");
    }

    #[test]
    fn advance_on_result_contains() {
        let mut runtime = example_runtime();
        let moved = runtime.advance(
            Some("run-1"),
            "accounts__read",
            &text_result("Account VERIFIED, ready for review."),
        );
        assert_eq!(moved.as_deref(), Some("verify"));
        assert_eq!(runtime.current_state(), "verify");
    }

    #[test]
    fn advance_no_matching_transition_stays() {
        let mut runtime = example_runtime();
        let moved = runtime.advance(Some("run-1"), "accounts__list", &text_result("listed"));
        assert_eq!(moved, None);
        assert_eq!(runtime.current_state(), "triage");
    }

    #[test]
    fn advance_on_result_success_and_failure() {
        let mut runtime = example_runtime();
        runtime
            .advance(Some("run-1"), "accounts__read", &text_result("verified"))
            .unwrap();
        assert_eq!(runtime.current_state(), "verify");
        // A failed verification must not advance to decide.
        let moved = runtime.advance(Some("run-1"), "accounts__verify", &error_result());
        assert_eq!(moved, None);
        assert_eq!(runtime.current_state(), "verify");
        // A successful one does.
        let moved = runtime.advance(Some("run-1"), "accounts__verify", &text_result("ok"));
        assert_eq!(moved.as_deref(), Some("decide"));
        // A transport error also counts as failure.
        let mut fresh = example_runtime();
        fresh
            .advance(Some("run-1"), "accounts__read", &text_result("verified"))
            .unwrap();
        let moved = fresh.advance(Some("run-1"), "accounts__verify", &transport_error());
        assert_eq!(moved, None);
    }

    #[test]
    fn advance_on_result_equals() {
        let config = Arc::new(
            ProcedureConfig::from_json(
                &json!({
                    "initial": "a",
                    "states": {
                        "a": {"tools": ["x"], "transitions": [{"on_tool": "x", "on_result": {"equals": "APPROVED"}, "to": "b"}]},
                        "b": {"tools": ["y"], "terminal": true}
                    }
                })
                .to_string(),
            )
            .unwrap(),
        );
        let mut runtime = ProcedureRuntime::new(Arc::clone(&config));
        assert_eq!(
            runtime.advance(Some("run-1"), "x", &text_result("APPROVED\n")),
            Some("b".to_string())
        );
        let mut runtime = ProcedureRuntime::new(Arc::clone(&config));
        assert_eq!(
            runtime.advance(Some("run-1"), "x", &text_result("approved")),
            None
        );
    }

    #[test]
    fn first_matching_transition_wins() {
        let mut runtime = example_runtime();
        // "verified" matches the first transition; the self-loop must not
        // swallow it even though it also matches the tool.
        let moved = runtime.advance(Some("run-1"), "accounts__read", &text_result("verified"));
        assert_eq!(moved.as_deref(), Some("verify"));
    }

    #[test]
    fn terminal_state_still_enforces_its_tools() {
        let mut runtime = example_runtime();
        runtime
            .advance(Some("run-1"), "accounts__read", &text_result("verified"))
            .unwrap();
        runtime
            .advance(Some("run-1"), "accounts__verify", &text_result("ok"))
            .unwrap();
        assert!(runtime.is_terminal());
        assert!(runtime
            .check_tool(Some("run-1"), "accounts__decide")
            .is_ok());
        assert!(runtime.check_tool(Some("run-1"), "accounts__read").is_err());
    }

    #[test]
    fn terminal_state_with_no_tools_denies_everything() {
        let config = ProcedureConfig::from_json(
            &json!({
                "initial": "a",
                "states": {
                    "a": {"tools": ["x"], "transitions": [{"on_tool": "x", "to": "b"}]},
                    "b": {"tools": [], "terminal": true}
                }
            })
            .to_string(),
        )
        .unwrap();
        let mut runtime = ProcedureRuntime::new(Arc::new(config));
        runtime.advance(Some("run-1"), "x", &text_result("done"));
        assert!(runtime.is_terminal());
        let error = runtime.check_tool(Some("run-1"), "x").unwrap_err();
        assert!(error.contains("no further tools"), "{error}");
    }

    #[test]
    fn advance_does_nothing_when_unavailable() {
        let config = ProcedureConfig::unavailable("boom");
        let mut runtime = ProcedureRuntime::new(Arc::new(config));
        assert_eq!(
            runtime.advance(Some("run-1"), "x", &text_result("done")),
            None
        );
    }

    #[test]
    fn finish_check_blocks_non_terminal_state() {
        let mut runtime = example_runtime();
        let error = runtime.finish_check(Some("run-1")).unwrap_err();
        assert!(error.contains("triage"), "{error}");
        assert!(error.contains("incomplete"), "{error}");
    }

    #[test]
    fn finish_check_allows_terminal_state() {
        let mut runtime = example_runtime();
        runtime
            .advance(Some("run-1"), "accounts__read", &text_result("verified"))
            .unwrap();
        runtime
            .advance(Some("run-1"), "accounts__verify", &text_result("ok"))
            .unwrap();
        assert!(runtime.finish_check(Some("run-1")).is_ok());
    }

    #[test]
    fn finish_check_allows_unavailable_config() {
        let config = ProcedureConfig::unavailable("boom");
        let mut runtime = ProcedureRuntime::new(Arc::new(config));
        assert!(runtime.finish_check(Some("run-1")).is_ok());
    }

    #[test]
    fn new_kickoff_restarts_the_procedure() {
        let mut runtime = example_runtime();
        runtime
            .advance(Some("run-1"), "accounts__read", &text_result("verified"))
            .unwrap();
        assert_eq!(runtime.current_state(), "verify");
        // A fresh user request restarts from the initial state.
        assert!(runtime.check_tool(Some("run-2"), "accounts__list").is_ok());
        assert_eq!(runtime.current_state(), "triage");
        assert!(runtime
            .check_tool(Some("run-2"), "accounts__decide")
            .is_err());
    }

    #[test]
    fn same_kickoff_keeps_state_across_turns() {
        let mut runtime = example_runtime();
        runtime
            .advance(Some("run-1"), "accounts__read", &text_result("verified"))
            .unwrap();
        assert!(runtime
            .check_tool(Some("run-1"), "accounts__verify")
            .is_ok());
        assert_eq!(runtime.current_state(), "verify");
        assert!(runtime.finish_check(Some("run-1")).is_err());
    }

    #[test]
    fn full_sop_walk() {
        let mut runtime = example_runtime();
        // triage: only list/read; cannot decide yet, cannot finish.
        assert!(runtime.check_tool(Some("run-1"), "accounts__list").is_ok());
        assert!(runtime
            .check_tool(Some("run-1"), "accounts__decide")
            .is_err());
        assert!(runtime.finish_check(Some("run-1")).is_err());
        // Read an unverified account: stays in triage.
        runtime
            .advance(
                Some("run-1"),
                "accounts__read",
                &text_result("pending review"),
            )
            .unwrap();
        assert_eq!(runtime.current_state(), "triage");
        // Read a verified account: moves to verify.
        runtime
            .advance(Some("run-1"), "accounts__read", &text_result("verified"))
            .unwrap();
        assert_eq!(runtime.current_state(), "verify");
        assert!(runtime.check_tool(Some("run-1"), "accounts__read").is_err());
        assert!(runtime
            .check_tool(Some("run-1"), "accounts__verify")
            .is_ok());
        // Verify successfully: moves to decide, which is terminal.
        runtime
            .advance(
                Some("run-1"),
                "accounts__verify",
                &text_result("identity confirmed"),
            )
            .unwrap();
        assert_eq!(runtime.current_state(), "decide");
        assert!(runtime.is_terminal());
        assert!(runtime
            .check_tool(Some("run-1"), "accounts__decide")
            .is_ok());
        assert!(runtime.finish_check(Some("run-1")).is_ok());
    }

    #[test]
    fn kickoff_id_comes_from_the_last_user_message() {
        let message = Message::user()
            .with_text("process these accounts")
            .with_generated_id_if_missing();
        let expected = message.id.clone().expect("message has an id");
        let conversation =
            goose_provider_types::conversation::Conversation::new_unvalidated(vec![message]);
        assert_eq!(
            kickoff_id(&conversation).as_deref(),
            Some(expected.as_str())
        );
    }
}
