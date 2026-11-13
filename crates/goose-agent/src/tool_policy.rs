//! Declarative, fail-closed tool-call policies.
//!
//! An implementation of the core idea of LEANGUARD
//! ("Lean-Verified Guardrails for Tool-Using Agents", arXiv:2611.02332):
//! every proposed tool call is checked against a policy *before* it reaches
//! the environment, evaluation is fail-closed, and a denied call gets
//! feedback — the violated rule and a hint — plus a repaired call when one
//! can be found, instead of a bare refusal.
//!
//! A policy is a list of [`PolicyRule`]s. Each rule names a tool (exact name
//! or `prefix*` glob) and the conditions a matching call must satisfy:
//!
//! * [`ArgPredicate`]s — state predicates over the call's arguments
//!   (equality, membership, substring, regex, "path must be under this
//!   directory").
//! * [`HistoryCondition`]s — scoped past-time operators over the execution
//!   history: a call must (or must not) have happened before, or at most N
//!   times.
//!
//! A call is admitted only if every matching rule's conditions hold. Anything
//! a rule cannot establish — a missing argument, an unparseable value —
//! counts against the call, so evaluation fails closed. Calls no rule
//! mentions are allowed: policies are deny-rules, not an allow-list.
//!
//! What this deliberately does **not** implement:
//!
//! * No Lean4 prover. LeanGuard proves its decision procedure equivalent to
//!   a LeanLTL semantics; here the decision procedure *is* the semantics,
//!   so a candidate repair is "proved admissible" by re-evaluating it with
//!   the same procedure ([`ToolPolicy::suggest_repair`]).
//! * No proof-guided search over a term language. Repair is a bounded
//!   syntactic search: edits derived from the violated predicates, each
//!   re-checked by the evaluator. History violations cannot be repaired by
//!   editing the call, so they return no repair — the hint tells the model
//!   what to do instead (e.g. call the prerequisite tool first).
//!
//! The guard is meant to run as a [`PreToolCallHook`]: wrap a
//! [`PolicyGuardHook`] in the existing `PreToolCallHookOperation` and it runs
//! before tool execution. A denial becomes a tool error response, so the
//! model sees the violated rule, the hint, and any suggested repair, and can
//! retry — the paper's feedback loop, with no model call of its own.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use goose_provider_types::conversation::message::{MessageContent, ToolRequest};
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::pre_tool_call_hook::{PreToolCallDecision, PreToolCallHook};

/// A single proposed or executed tool call, in plain data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub name: String,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub arguments: serde_json::Map<String, serde_json::Value>,
}

impl ToolCallRecord {
    /// Build a record from a [`ToolRequest`]. Returns `None` when the request
    /// carries a tool-call error instead of a call — a malformed call can
    /// never satisfy a policy, so callers must treat `None` as a denial.
    pub fn from_request(request: &ToolRequest) -> Option<Self> {
        let call = request.tool_call.as_ref().ok()?;
        Some(Self {
            name: call.name.to_string(),
            arguments: call.arguments.clone().unwrap_or_default(),
        })
    }
}

/// A state predicate over one argument of a proposed tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ArgPredicate {
    /// The argument must equal this JSON value.
    Eq {
        key: String,
        value: serde_json::Value,
    },
    /// The argument must be one of these JSON values.
    OneOf {
        key: String,
        values: Vec<serde_json::Value>,
    },
    /// A string argument must not contain this substring.
    NotContains { key: String, substring: String },
    /// A string argument must match this regex.
    Matches { key: String, pattern: String },
    /// A path argument must stay under this directory (lexically resolved,
    /// so `..` cannot escape it).
    PathUnder { key: String, dir: String },
}

/// A past-time condition over the calls already executed in this session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HistoryCondition {
    /// A call to this tool must already have executed.
    Once { tool: String },
    /// No call to this tool may have executed yet.
    Never { tool: String },
    /// At most this many prior executions of this tool.
    AtMost { tool: String, count: usize },
}

/// One deny-rule: calls matching [`PolicyRule::tool`] are admitted only when
/// every [`PolicyRule::require`] predicate and every
/// [`PolicyRule::history`] condition holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyRule {
    pub name: String,
    /// Tool name, a `prefix*` glob, or `*` for every tool.
    pub tool: String,
    #[serde(default)]
    pub require: Vec<ArgPredicate>,
    #[serde(default)]
    pub history: Vec<HistoryCondition>,
    /// Handed to the model when this rule denies a call.
    #[serde(default)]
    pub hint: Option<String>,
}

/// A declarative tool-call policy: a list of deny-rules.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolPolicy {
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

/// The outcome of checking one proposed call against a policy.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyOutcome {
    Allow,
    Deny {
        rule: String,
        reason: String,
        hint: Option<String>,
    },
}

/// The outcome of a check, plus a repaired call when one was found.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyVerdict {
    pub outcome: PolicyOutcome,
    /// A nearby call the policy admits, found by editing the denied call.
    /// `None` when the call was allowed or no admissible edit exists.
    pub repair: Option<ToolCallRecord>,
}

fn matches_tool(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        true
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        pattern == name
    }
}

fn arg_as_str<'a>(
    arguments: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    arguments.get(key)?.as_str()
}

/// Lexically normalize a path (`.` and `..` resolved without touching the
/// filesystem), so a prefix check cannot be escaped with `..`.
fn normalize_path_lexically(path: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let is_absolute = path.starts_with('/');
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if !parts.is_empty() {
                    parts.pop();
                } else if !is_absolute {
                    parts.push("..".to_string());
                }
            }
            part => parts.push(part.to_string()),
        }
    }
    if is_absolute {
        std::iter::once(String::new()).chain(parts).collect()
    } else {
        parts
    }
}

fn path_is_under(candidate: &str, dir: &str) -> bool {
    let candidate = normalize_path_lexically(candidate);
    let dir = normalize_path_lexically(dir);
    candidate.len() >= dir.len() && candidate[..dir.len()] == dir[..]
}

fn eval_predicate(
    predicate: &ArgPredicate,
    arguments: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    match predicate {
        ArgPredicate::Eq { key, value } => arguments.get(key) == Some(value),
        ArgPredicate::OneOf { key, values } => arguments
            .get(key)
            .is_some_and(|actual| values.contains(actual)),
        ArgPredicate::NotContains { key, substring } => {
            arg_as_str(arguments, key).is_some_and(|value| !value.contains(substring.as_str()))
        }
        ArgPredicate::Matches { key, pattern } => arg_as_str(arguments, key)
            .is_some_and(|value| Regex::new(pattern).is_ok_and(|regex| regex.is_match(value))),
        ArgPredicate::PathUnder { key, dir } => {
            arg_as_str(arguments, key).is_some_and(|value| path_is_under(value, dir))
        }
    }
}

fn predicate_failure_reason(
    predicate: &ArgPredicate,
    arguments: &serde_json::Map<String, serde_json::Value>,
) -> String {
    match predicate {
        ArgPredicate::Eq { key, value } => format!("argument {key:?} must equal {value}"),
        ArgPredicate::OneOf { key, values } => {
            format!("argument {key:?} must be one of {values:?}")
        }
        ArgPredicate::NotContains { key, substring } => {
            format!("argument {key:?} must not contain {substring:?}")
        }
        ArgPredicate::Matches { key, pattern } => {
            format!("argument {key:?} must match /{pattern}/")
        }
        ArgPredicate::PathUnder { key, dir } => {
            let actual = arguments
                .get(key)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<missing>".to_string());
            format!("argument {key:?} must be a path under {dir:?} but was {actual}")
        }
    }
}

fn eval_history(condition: &HistoryCondition, history: &[ToolCallRecord]) -> bool {
    match condition {
        HistoryCondition::Once { tool } => {
            history.iter().any(|call| matches_tool(tool, &call.name))
        }
        HistoryCondition::Never { tool } => {
            !history.iter().any(|call| matches_tool(tool, &call.name))
        }
        HistoryCondition::AtMost { tool, count } => {
            history
                .iter()
                .filter(|call| matches_tool(tool, &call.name))
                .count()
                <= *count
        }
    }
}

fn history_failure_reason(condition: &HistoryCondition, history: &[ToolCallRecord]) -> String {
    match condition {
        HistoryCondition::Once { tool } => {
            format!("rule requires a prior call to {tool:?}")
        }
        HistoryCondition::Never { tool } => {
            format!("rule forbids any prior call to {tool:?}")
        }
        HistoryCondition::AtMost { tool, count } => {
            let seen = history
                .iter()
                .filter(|call| matches_tool(tool, &call.name))
                .count();
            format!("rule allows at most {count} prior calls to {tool:?}, saw {seen}")
        }
    }
}

/// The edit a violated predicate suggests, if it has one.
fn fix_predicate(
    predicate: &ArgPredicate,
    arguments: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    match predicate {
        ArgPredicate::Eq { key, value } => {
            arguments.insert(key.clone(), value.clone());
            true
        }
        ArgPredicate::OneOf { key, values } => match values.first() {
            Some(first) => {
                arguments.insert(key.clone(), first.clone());
                true
            }
            None => false,
        },
        ArgPredicate::NotContains { key, substring } => match arguments.get(key) {
            Some(serde_json::Value::String(value)) => {
                let fixed = value.replace(substring.as_str(), "");
                arguments.insert(key.clone(), serde_json::Value::String(fixed));
                true
            }
            _ => false,
        },
        ArgPredicate::Matches { .. } => false,
        ArgPredicate::PathUnder { key, dir } => {
            let fixed = match arguments.get(key).and_then(|value| value.as_str()) {
                Some(path) => {
                    let base = Path::new(path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if base.is_empty() {
                        dir.clone()
                    } else {
                        format!("{dir}/{base}")
                    }
                }
                None => dir.clone(),
            };
            arguments.insert(key.clone(), serde_json::Value::String(fixed));
            true
        }
    }
}

impl ToolPolicy {
    /// Parse a policy from JSON, rejecting invalid regexes so a typo fails
    /// at load time instead of denying calls at runtime.
    pub fn from_json(json: &str) -> Result<Self> {
        let policy: Self = serde_json::from_str(json)?;
        for rule in &policy.rules {
            for predicate in &rule.require {
                if let ArgPredicate::Matches { pattern, .. } = predicate {
                    Regex::new(pattern).with_context(|| {
                        format!("rule '{}' has an invalid regex: {pattern}", rule.name)
                    })?;
                }
            }
        }
        Ok(policy)
    }

    /// Load a policy from a JSON file.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read tool policy file {}", path.display()))?;
        Self::from_json(&json).with_context(|| format!("invalid tool policy in {}", path.display()))
    }

    /// A policy that denies every call. Used when a configured policy file
    /// cannot be loaded: the failure is loud (every denial names this rule
    /// and carries the load error as its hint) instead of silently
    /// fail-open. The predicate keys on an argument no real tool has, so no
    /// call can satisfy it.
    pub fn deny_all(reason: impl Into<String>) -> Self {
        Self {
            rules: vec![PolicyRule {
                name: "tool-policy-unavailable".to_string(),
                tool: "*".to_string(),
                require: vec![ArgPredicate::Eq {
                    key: "goose.tool_policy_unavailable".to_string(),
                    value: serde_json::Value::Bool(true),
                }],
                history: vec![],
                hint: Some(reason.into()),
            }],
        }
    }

    /// Check one proposed call against this policy, given the calls that
    /// already executed. Fail-closed: any condition that cannot be
    /// established denies the call.
    pub fn evaluate(&self, call: &ToolCallRecord, history: &[ToolCallRecord]) -> PolicyOutcome {
        for rule in &self.rules {
            if !matches_tool(&rule.tool, &call.name) {
                continue;
            }
            for predicate in &rule.require {
                if !eval_predicate(predicate, &call.arguments) {
                    return PolicyOutcome::Deny {
                        rule: rule.name.clone(),
                        reason: predicate_failure_reason(predicate, &call.arguments),
                        hint: rule.hint.clone(),
                    };
                }
            }
            for condition in &rule.history {
                if !eval_history(condition, history) {
                    return PolicyOutcome::Deny {
                        rule: rule.name.clone(),
                        reason: history_failure_reason(condition, history),
                        hint: rule.hint.clone(),
                    };
                }
            }
        }
        PolicyOutcome::Allow
    }

    /// Search for a nearby call the policy admits, by applying the edits the
    /// violated predicates suggest and re-evaluating each candidate with the
    /// same decision procedure. Returns `None` when the call was allowed,
    /// when the denial comes from a history condition (no argument edit can
    /// fix the past), or when no candidate is admitted.
    pub fn suggest_repair(
        &self,
        call: &ToolCallRecord,
        history: &[ToolCallRecord],
    ) -> Option<ToolCallRecord> {
        let PolicyOutcome::Deny {
            rule: rule_name, ..
        } = self.evaluate(call, history)
        else {
            return None;
        };
        let rule = self.rules.iter().find(|rule| rule.name == rule_name)?;
        if rule
            .history
            .iter()
            .any(|condition| !eval_history(condition, history))
        {
            return None;
        }
        let failing: Vec<&ArgPredicate> = rule
            .require
            .iter()
            .filter(|predicate| !eval_predicate(predicate, &call.arguments))
            .collect();
        let mut attempts: Vec<ToolCallRecord> = Vec::new();
        let mut fix_all = call.clone();
        if failing
            .iter()
            .all(|predicate| fix_predicate(predicate, &mut fix_all.arguments))
        {
            attempts.push(fix_all);
        }
        for predicate in &failing {
            let mut fixed = call.clone();
            if fix_predicate(predicate, &mut fixed.arguments) {
                attempts.push(fixed);
            }
        }
        attempts
            .into_iter()
            .find(|candidate| matches!(self.evaluate(candidate, history), PolicyOutcome::Allow))
    }
}

/// The calls that already executed in this conversation, in order. A request
/// counts as executed only once it has a successful (non-error) response, so
/// denied or failed attempts do not count toward history conditions.
pub fn history_from_conversation(conversation: &Conversation) -> Vec<ToolCallRecord> {
    let mut requests: Vec<&ToolRequest> = Vec::new();
    let mut succeeded: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for content in conversation
        .messages()
        .iter()
        .flat_map(|message| &message.content)
    {
        match content {
            MessageContent::ToolRequest(request) => requests.push(request),
            MessageContent::ToolResponse(response) => {
                let ok =
                    matches!(&response.tool_result, Ok(result) if result.is_error != Some(true));
                if ok {
                    succeeded.insert(response.id.as_str());
                }
            }
            _ => {}
        }
    }
    requests
        .into_iter()
        .filter(|request| succeeded.contains(request.id.as_str()))
        .filter_map(ToolCallRecord::from_request)
        .collect()
}

/// Check one proposed tool call against a policy, with history taken from
/// the conversation. A request that cannot be parsed is denied: a call the
/// guard cannot read is a call it cannot admit.
pub fn evaluate_request(
    policy: &ToolPolicy,
    conversation: &Conversation,
    request: &ToolRequest,
) -> PolicyVerdict {
    let Some(call) = ToolCallRecord::from_request(request) else {
        return PolicyVerdict {
            outcome: PolicyOutcome::Deny {
                rule: "malformed-tool-call".to_string(),
                reason: "proposed tool call could not be parsed".to_string(),
                hint: None,
            },
            repair: None,
        };
    };
    let history = history_from_conversation(conversation);
    let outcome = policy.evaluate(&call, &history);
    let repair = match outcome {
        PolicyOutcome::Allow => None,
        PolicyOutcome::Deny { .. } => policy.suggest_repair(&call, &history),
    };
    PolicyVerdict { outcome, repair }
}

/// Render a denied verdict as the message the model receives: the violated
/// rule, its hint, and any repaired call. Returns `None` when the verdict
/// allows the call.
pub fn denial_reason(verdict: &PolicyVerdict) -> Option<String> {
    let PolicyOutcome::Deny { rule, reason, hint } = &verdict.outcome else {
        return None;
    };
    let mut message = format!("Denied by tool policy rule '{rule}': {reason}.");
    if let Some(hint) = hint {
        message.push_str(&format!(" Hint: {hint}"));
    }
    if let Some(repair) = &verdict.repair {
        if let Ok(json) = serde_json::to_string(repair) {
            message.push_str(&format!(" Suggested admissible call: {json}"));
        }
    }
    Some(message)
}

/// A [`PreToolCallHook`] that enforces a [`ToolPolicy`]. Denials carry the
/// violated rule, its hint, and any repaired call, so the model can retry
/// with an admissible call instead of hitting a bare refusal.
pub struct PolicyGuardHook {
    policy: Arc<ToolPolicy>,
}

impl PolicyGuardHook {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S> PreToolCallHook<S> for PolicyGuardHook
where
    S: MaybeSend + MaybeSync + 'static,
{
    async fn before_call(
        &self,
        _session: &S,
        conversation: &Conversation,
        request: &ToolRequest,
    ) -> Result<PreToolCallDecision> {
        let verdict = evaluate_request(&self.policy, conversation, request);
        match denial_reason(&verdict) {
            Some(reason) => Ok(PreToolCallDecision::Deny { reason }),
            None => Ok(PreToolCallDecision::Allow),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose_provider_types::conversation::message::Message;
    use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
    use serde_json::json;

    fn record(name: &str, arguments: serde_json::Value) -> ToolCallRecord {
        ToolCallRecord {
            name: name.to_string(),
            arguments: arguments.as_object().cloned().unwrap_or_default(),
        }
    }

    fn policy_from(json: &str) -> ToolPolicy {
        ToolPolicy::from_json(json).expect("test policy must parse")
    }

    fn shell_rule() -> ToolPolicy {
        policy_from(
            r#"{
                "rules": [{
                    "name": "shell-stays-in-workspace",
                    "tool": "developer__shell",
                    "require": [{ "type": "path_under", "key": "command", "dir": "/workspace" }],
                    "hint": "Run shell commands inside /workspace."
                }]
            }"#,
        )
    }

    #[test]
    fn empty_policy_allows_everything() {
        let policy = ToolPolicy::default();
        assert_eq!(
            policy.evaluate(
                &record("developer__shell", json!({"command": "rm -rf /"})),
                &[]
            ),
            PolicyOutcome::Allow
        );
    }

    #[test]
    fn rule_ignores_other_tools() {
        let policy = shell_rule();
        assert_eq!(
            policy.evaluate(
                &record("developer__text_editor", json!({"path": "/etc/passwd"})),
                &[]
            ),
            PolicyOutcome::Allow
        );
    }

    #[test]
    fn glob_prefix_matches_extension_tools() {
        let policy = policy_from(
            r#"{"rules": [{"name": "no-writes", "tool": "developer__*", "require": [
                {"type": "not_contains", "key": "command", "substring": "rm -rf"}
            ]}]}"#,
        );
        let denied = policy.evaluate(
            &record("developer__shell", json!({"command": "rm -rf /tmp"})),
            &[],
        );
        assert!(matches!(denied, PolicyOutcome::Deny { .. }));
        assert_eq!(
            policy.evaluate(
                &record("other__shell", json!({"command": "rm -rf /tmp"})),
                &[]
            ),
            PolicyOutcome::Allow
        );
    }

    #[test]
    fn eq_and_one_of_predicates() {
        let policy = policy_from(
            r#"{"rules": [{"name": "transfer-limits", "tool": "bank__transfer", "require": [
                {"type": "eq", "key": "currency", "value": "USD"},
                {"type": "one_of", "key": "channel", "values": ["ach", "wire"]}
            ]}]}"#,
        );
        assert_eq!(
            policy.evaluate(
                &record(
                    "bank__transfer",
                    json!({"currency": "USD", "channel": "ach"})
                ),
                &[]
            ),
            PolicyOutcome::Allow
        );
        assert!(matches!(
            policy.evaluate(
                &record(
                    "bank__transfer",
                    json!({"currency": "EUR", "channel": "ach"})
                ),
                &[]
            ),
            PolicyOutcome::Deny { .. }
        ));
        assert!(matches!(
            policy.evaluate(
                &record(
                    "bank__transfer",
                    json!({"currency": "USD", "channel": "crypto"})
                ),
                &[]
            ),
            PolicyOutcome::Deny { .. }
        ));
    }

    #[test]
    fn missing_argument_denies_fail_closed() {
        let policy = shell_rule();
        assert!(matches!(
            policy.evaluate(&record("developer__shell", json!({})), &[]),
            PolicyOutcome::Deny { .. }
        ));
    }

    #[test]
    fn path_under_blocks_dotdot_escape() {
        let policy = shell_rule();
        assert_eq!(
            policy.evaluate(
                &record(
                    "developer__shell",
                    json!({"command": "/workspace/src/main.rs"})
                ),
                &[]
            ),
            PolicyOutcome::Allow
        );
        assert!(matches!(
            policy.evaluate(
                &record(
                    "developer__shell",
                    json!({"command": "/workspace/../etc/passwd"})
                ),
                &[]
            ),
            PolicyOutcome::Deny { .. }
        ));
        assert!(matches!(
            policy.evaluate(
                &record("developer__shell", json!({"command": "/etc/passwd"})),
                &[]
            ),
            PolicyOutcome::Deny { .. }
        ));
    }

    #[test]
    fn invalid_regex_denies_fail_closed() {
        let policy = ToolPolicy {
            rules: vec![PolicyRule {
                name: "bad-regex".to_string(),
                tool: "*".to_string(),
                require: vec![ArgPredicate::Matches {
                    key: "command".to_string(),
                    pattern: "([invalid".to_string(),
                }],
                history: vec![],
                hint: None,
            }],
        };
        assert!(matches!(
            policy.evaluate(&record("developer__shell", json!({"command": "ls"})), &[]),
            PolicyOutcome::Deny { .. }
        ));
    }

    #[test]
    fn from_json_rejects_invalid_regex() {
        let err = ToolPolicy::from_json(
            r#"{"rules": [{"name": "x", "tool": "*", "require": [
                {"type": "matches", "key": "command", "pattern": "([invalid"}
            ]}]}"#,
        );
        assert!(err.is_err());
    }

    #[test]
    fn history_once_requires_a_prior_call() {
        let policy = policy_from(
            r#"{"rules": [{"name": "read-before-write", "tool": "developer__text_editor",
                "history": [{"type": "once", "tool": "developer__read_file"}],
                "hint": "Read the file before editing it."}]}"#,
        );
        let call = record("developer__text_editor", json!({}));
        assert!(matches!(
            policy.evaluate(&call, &[]),
            PolicyOutcome::Deny { .. }
        ));
        let history = vec![record("developer__read_file", json!({}))];
        assert_eq!(policy.evaluate(&call, &history), PolicyOutcome::Allow);
    }

    #[test]
    fn history_never_and_at_most() {
        let policy = policy_from(
            r#"{"rules": [
                {"name": "one-transfer", "tool": "bank__transfer",
                 "history": [{"type": "never", "tool": "bank__transfer"}]},
                {"name": "shell-budget", "tool": "developer__shell",
                 "history": [{"type": "at_most", "tool": "developer__shell", "count": 2}]}
            ]}"#,
        );
        let transfer = record("bank__transfer", json!({}));
        assert_eq!(policy.evaluate(&transfer, &[]), PolicyOutcome::Allow);
        assert!(matches!(
            policy.evaluate(&transfer, std::slice::from_ref(&transfer)),
            PolicyOutcome::Deny { .. }
        ));
        let shell = record("developer__shell", json!({}));
        let two_shells = vec![shell.clone(), shell.clone()];
        assert_eq!(policy.evaluate(&shell, &two_shells), PolicyOutcome::Allow);
        let three_shells = vec![shell.clone(), shell.clone(), shell.clone()];
        assert!(matches!(
            policy.evaluate(&shell, &three_shells),
            PolicyOutcome::Deny { .. }
        ));
    }

    #[test]
    fn repair_rewrites_path_under_the_allowed_dir() {
        let policy = shell_rule();
        let denied = record("developer__shell", json!({"command": "/etc/passwd"}));
        let repaired = policy
            .suggest_repair(&denied, &[])
            .expect("a repair must exist");
        assert_eq!(repaired.name, "developer__shell");
        assert_eq!(
            repaired.arguments.get("command"),
            Some(&json!("/workspace/passwd"))
        );
        assert_eq!(policy.evaluate(&repaired, &[]), PolicyOutcome::Allow);
    }

    #[test]
    fn repair_picks_first_allowed_value() {
        let policy = policy_from(
            r#"{"rules": [{"name": "channel", "tool": "bank__transfer", "require": [
                {"type": "one_of", "key": "channel", "values": ["ach", "wire"]}
            ]}]}"#,
        );
        let denied = record("bank__transfer", json!({"channel": "crypto"}));
        let repaired = policy
            .suggest_repair(&denied, &[])
            .expect("a repair must exist");
        assert_eq!(repaired.arguments.get("channel"), Some(&json!("ach")));
        assert_eq!(policy.evaluate(&repaired, &[]), PolicyOutcome::Allow);
    }

    #[test]
    fn repair_returns_none_for_history_violations() {
        let policy = policy_from(
            r#"{"rules": [{"name": "read-before-write", "tool": "developer__text_editor",
                "history": [{"type": "once", "tool": "developer__read_file"}]}]}"#,
        );
        let denied = record("developer__text_editor", json!({}));
        assert!(policy.suggest_repair(&denied, &[]).is_none());
    }

    #[test]
    fn deny_all_denies_everything() {
        let policy = ToolPolicy::deny_all("policy file failed to load");
        assert!(matches!(
            policy.evaluate(&record("anything__tool", json!({})), &[]),
            PolicyOutcome::Deny { .. }
        ));
    }

    fn params(name: &str, arguments: serde_json::Value) -> CallToolRequestParams {
        let mut call = CallToolRequestParams::new(name.to_string());
        if let Some(arguments) = arguments.as_object().cloned() {
            call = call.with_arguments(arguments);
        }
        call
    }

    fn tool_request(id: &str, name: &str, arguments: serde_json::Value) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(params(name, arguments)),
            metadata: None,
            tool_meta: None,
        }
    }

    #[test]
    fn evaluate_request_builds_history_from_conversation() {
        let policy = policy_from(
            r#"{"rules": [{"name": "read-before-write", "tool": "developer__text_editor",
                "history": [{"type": "once", "tool": "developer__read_file"}]}]}"#,
        );
        let prior_response = Message::user().with_tool_response(
            "call-0",
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "contents",
            )])),
        );
        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("edit the file"),
            Message::assistant()
                .with_tool_request("call-0", Ok(params("developer__read_file", json!({})))),
            prior_response,
            Message::assistant()
                .with_tool_request("call-1", Ok(params("developer__text_editor", json!({})))),
        ]);
        let request = tool_request("call-1", "developer__text_editor", json!({}));
        let verdict = evaluate_request(&policy, &conversation, &request);
        assert_eq!(verdict.outcome, PolicyOutcome::Allow);
        assert!(verdict.repair.is_none());
    }

    #[test]
    fn evaluate_request_denies_malformed_calls() {
        let policy = ToolPolicy::default();
        let request = ToolRequest {
            id: "call-1".to_string(),
            tool_call: Err(rmcp::model::ErrorData::internal_error("boom", None)),
            metadata: None,
            tool_meta: None,
        };
        let conversation = Conversation::new_unvalidated(vec![Message::user().with_text("hi")]);
        let verdict = evaluate_request(&policy, &conversation, &request);
        assert!(matches!(verdict.outcome, PolicyOutcome::Deny { .. }));
    }

    #[tokio::test]
    async fn hook_denies_with_rule_hint_and_repair() {
        let policy = Arc::new(shell_rule());
        let hook = PolicyGuardHook::new(policy);
        let conversation = Conversation::new_unvalidated(vec![Message::user().with_text("go")]);
        let request = tool_request(
            "call-1",
            "developer__shell",
            json!({"command": "/etc/passwd"}),
        );
        let decision = hook
            .before_call(&(), &conversation, &request)
            .await
            .expect("hook must not error");
        match decision {
            PreToolCallDecision::Deny { reason } => {
                assert!(reason.contains("shell-stays-in-workspace"), "{reason}");
                assert!(reason.contains("/workspace"), "{reason}");
                assert!(reason.contains("/workspace/passwd"), "{reason}");
            }
            PreToolCallDecision::Allow => panic!("hook must deny this call"),
        }
        let allowed = tool_request(
            "call-2",
            "developer__shell",
            json!({"command": "/workspace/x"}),
        );
        assert!(matches!(
            hook.before_call(&(), &conversation, &allowed)
                .await
                .unwrap(),
            PreToolCallDecision::Allow
        ));
    }
}
