//! Lifecycle hooks support, modelled after the Open Plugins
//! [hooks specification](https://open-plugins.com/agent-builders/components/hooks).
//!
//! Hooks live in `<plugin-root>/hooks/hooks.json` of any plugin discovered by
//! [`crate::plugins::discovery::discover_enabled_plugins`]. The schema is:
//!
//! ```json
//! {
//!   "hooks": {
//!     "PostToolUse": [
//!       {
//!         "matcher": "developer__shell|developer__text_editor",
//!         "hooks": [
//!           { "type": "command", "command": "${PLUGIN_ROOT}/scripts/log.sh" }
//!         ]
//!       }
//!     ]
//!   }
//! }
//! ```
//!
//! Goose currently supports `type: "command"` actions. Unknown event names and
//! action types are ignored per the spec. Hook scripts receive the JSON event
//! context on stdin and SHOULD exit 0 on success.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::plugins::discovery::{discover_enabled_plugins, DiscoveredPlugin};

/// Default per-hook timeout when the plugin does not specify one.
const DEFAULT_HOOK_TIMEOUT_SECS: u64 = 30;

/// Lifecycle events a hook can subscribe to.
///
/// The variant names match the event names used in `hooks.json`. Unknown
/// events in user config are ignored at load time, per the spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    BeforeReadFile,
    AfterFileEdit,
    BeforeShellExecution,
    AfterShellExecution,
    Stop,
}

impl HookEvent {
    fn name(&self) -> &'static str {
        match self {
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::PostToolUseFailure => "PostToolUseFailure",
            HookEvent::SessionStart => "SessionStart",
            HookEvent::SessionEnd => "SessionEnd",
            HookEvent::UserPromptSubmit => "UserPromptSubmit",
            HookEvent::BeforeReadFile => "BeforeReadFile",
            HookEvent::AfterFileEdit => "AfterFileEdit",
            HookEvent::BeforeShellExecution => "BeforeShellExecution",
            HookEvent::AfterShellExecution => "AfterShellExecution",
            HookEvent::Stop => "Stop",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "PreToolUse" => HookEvent::PreToolUse,
            "PostToolUse" => HookEvent::PostToolUse,
            "PostToolUseFailure" => HookEvent::PostToolUseFailure,
            "SessionStart" => HookEvent::SessionStart,
            "SessionEnd" => HookEvent::SessionEnd,
            "UserPromptSubmit" => HookEvent::UserPromptSubmit,
            "BeforeReadFile" => HookEvent::BeforeReadFile,
            "AfterFileEdit" => HookEvent::AfterFileEdit,
            "BeforeShellExecution" => HookEvent::BeforeShellExecution,
            "AfterShellExecution" => HookEvent::AfterShellExecution,
            "Stop" => HookEvent::Stop,
            _ => return None,
        })
    }
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Top-level `hooks.json` shape.
#[derive(Debug, Default, Deserialize)]
struct HooksFile {
    #[serde(default)]
    hooks: HashMap<String, Vec<RawHookRule>>,
}

/// One rule within a `hooks.json` event entry.
#[derive(Debug, Deserialize)]
struct RawHookRule {
    #[serde(default)]
    matcher: Option<String>,
    #[serde(default)]
    hooks: Vec<RawHookAction>,
}

/// One action entry under a rule's `hooks` array. We only run `command`
/// today, but we deserialize the others so that loading a plugin which uses
/// them does not fail.
#[derive(Debug, Deserialize)]
struct RawHookAction {
    #[serde(default, rename = "type")]
    action_type: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    timeout: Option<u64>,
}

/// A loaded, plugin-bound hook rule ready to execute.
#[derive(Debug, Clone)]
struct LoadedRule {
    plugin_name: String,
    plugin_root: PathBuf,
    matcher: Option<Regex>,
    actions: Vec<LoadedAction>,
}

#[derive(Debug, Clone)]
enum LoadedAction {
    Command { command: String, timeout: Duration },
}

/// Context passed to a hook as JSON on stdin.
///
/// The `matcher_context` is the string the rule's `matcher` regex is tested
/// against — tool name for tool events, file path for file events, command
/// string for shell events. Other fields carry the same value plus the
/// raw JSON payload of the underlying event so scripts can do richer things
/// without needing to parse a hook-specific schema.
#[derive(Debug, Clone, Serialize)]
pub struct HookContext {
    pub event: String,
    pub session_id: String,
    pub matcher_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
}

impl HookContext {
    pub fn new(event: HookEvent, session_id: impl Into<String>) -> Self {
        Self {
            event: event.to_string(),
            session_id: session_id.into(),
            matcher_context: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            message: None,
            working_dir: None,
        }
    }

    pub fn with_tool(mut self, tool_name: impl Into<String>, tool_input: Option<Value>) -> Self {
        let name = tool_name.into();
        self.matcher_context = Some(name.clone());
        self.tool_name = Some(name);
        self.tool_input = tool_input;
        self
    }

    pub fn with_tool_output(mut self, output: Value) -> Self {
        self.tool_output = Some(output);
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        let msg = message.into();
        self.matcher_context.get_or_insert_with(|| msg.clone());
        self.message = Some(msg);
        self
    }

    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
}

/// Loads and executes plugin hooks.
#[derive(Debug, Default, Clone)]
pub struct HookManager {
    rules: HashMap<HookEvent, Vec<LoadedRule>>,
}

impl HookManager {
    /// Build a manager by scanning all enabled plugins for `hooks/hooks.json`.
    pub fn load(project_root: Option<&Path>) -> Self {
        let plugins = discover_enabled_plugins(project_root);
        Self::from_plugins(plugins)
    }

    fn from_plugins(plugins: Vec<DiscoveredPlugin>) -> Self {
        let mut rules: HashMap<HookEvent, Vec<LoadedRule>> = HashMap::new();
        let mut total = 0usize;

        for plugin in plugins {
            let hooks_path = plugin.root.join("hooks").join("hooks.json");
            if !hooks_path.is_file() {
                continue;
            }
            match load_hooks_file(&hooks_path, &plugin.name, &plugin.root) {
                Ok(loaded) => {
                    for (event, plugin_rules) in loaded {
                        total += plugin_rules.len();
                        rules.entry(event).or_default().extend(plugin_rules);
                    }
                }
                Err(err) => warn!(
                    plugin = %plugin.name,
                    path = %hooks_path.display(),
                    error = %err,
                    "Failed to load plugin hooks; skipping",
                ),
            }
        }

        if total > 0 {
            info!(
                rule_count = total,
                events = ?rules.keys().map(|e| e.name()).collect::<Vec<_>>(),
                "Loaded plugin hooks",
            );
        }

        Self { rules }
    }

    /// Returns true if any rule is registered for `event`.
    pub fn has_hooks(&self, event: HookEvent) -> bool {
        self.rules.get(&event).is_some_and(|r| !r.is_empty())
    }

    /// Fire all rules whose matcher matches the event context. Errors from
    /// individual hooks are logged but never propagated — a misbehaving hook
    /// MUST NOT crash the host tool.
    pub async fn emit(&self, event: HookEvent, ctx: HookContext) {
        let Some(rules) = self.rules.get(&event) else {
            return;
        };
        if rules.is_empty() {
            return;
        }

        let payload = match serde_json::to_string(&ctx) {
            Ok(s) => s,
            Err(err) => {
                warn!(event = %event, error = %err, "Failed to serialize hook context");
                return;
            }
        };

        for rule in rules {
            if let Some(matcher) = &rule.matcher {
                let target = ctx.matcher_context.as_deref().unwrap_or("");
                if !matcher.is_match(target) {
                    continue;
                }
            }

            for action in &rule.actions {
                let LoadedAction::Command { command, timeout } = action;
                debug!(
                    plugin = %rule.plugin_name,
                    event = %event,
                    command = %command,
                    "Running plugin hook",
                );
                if let Err(err) =
                    run_command_hook(command, &rule.plugin_root, &payload, *timeout).await
                {
                    warn!(
                        plugin = %rule.plugin_name,
                        event = %event,
                        command = %command,
                        error = %err,
                        "Plugin hook failed",
                    );
                }
            }
        }
    }
}

fn load_hooks_file(
    path: &Path,
    plugin_name: &str,
    plugin_root: &Path,
) -> Result<HashMap<HookEvent, Vec<LoadedRule>>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: HooksFile =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;

    let mut out: HashMap<HookEvent, Vec<LoadedRule>> = HashMap::new();
    for (event_name, raw_rules) in parsed.hooks {
        let Some(event) = HookEvent::from_name(&event_name) else {
            debug!(plugin = plugin_name, event = %event_name, "Ignoring unknown hook event");
            continue;
        };

        for raw in raw_rules {
            let matcher = match raw.matcher.as_deref().filter(|s| !s.is_empty()) {
                Some(pattern) => match Regex::new(pattern) {
                    Ok(re) => Some(re),
                    Err(err) => {
                        warn!(
                            plugin = plugin_name,
                            pattern,
                            error = %err,
                            "Invalid hook matcher regex; skipping rule",
                        );
                        continue;
                    }
                },
                None => None,
            };

            let mut actions = Vec::new();
            for raw_action in raw.hooks {
                match raw_action.action_type.as_deref().unwrap_or("command") {
                    "command" => {
                        if let Some(cmd) = raw_action.command {
                            let timeout = Duration::from_secs(
                                raw_action.timeout.unwrap_or(DEFAULT_HOOK_TIMEOUT_SECS),
                            );
                            actions.push(LoadedAction::Command {
                                command: cmd,
                                timeout,
                            });
                        }
                    }
                    other => {
                        debug!(
                            plugin = plugin_name,
                            action_type = other,
                            "Ignoring unsupported hook action type",
                        );
                    }
                }
            }

            if actions.is_empty() {
                continue;
            }

            out.entry(event).or_default().push(LoadedRule {
                plugin_name: plugin_name.to_string(),
                plugin_root: plugin_root.to_path_buf(),
                matcher,
                actions,
            });
        }
    }

    Ok(out)
}

async fn run_command_hook(
    raw_command: &str,
    plugin_root: &Path,
    payload: &str,
    timeout: Duration,
) -> Result<()> {
    let command = expand_plugin_root(raw_command, plugin_root);
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .env("PLUGIN_ROOT", plugin_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawning hook `{command}`"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(res) => res.with_context(|| format!("waiting on hook `{command}`"))?,
        Err(_) => anyhow::bail!("hook `{command}` timed out after {:?}", timeout),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "hook `{command}` exited with {:?}: {}",
            output.status.code(),
            stderr.trim()
        );
    }

    Ok(())
}

fn expand_plugin_root(command: &str, plugin_root: &Path) -> String {
    command.replace("${PLUGIN_ROOT}", &plugin_root.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::discovery::{DiscoveredPlugin, PluginSource};

    fn write_plugin(root: &Path, name: &str, hooks_json: &str) -> PathBuf {
        let plugin = root.join(name);
        std::fs::create_dir_all(plugin.join("hooks")).unwrap();
        std::fs::write(plugin.join("hooks").join("hooks.json"), hooks_json).unwrap();
        plugin
    }

    fn make_manager(plugins: Vec<DiscoveredPlugin>) -> HookManager {
        HookManager::from_plugins(plugins)
    }

    #[test]
    fn ignores_unknown_events() {
        let tmp = tempfile::tempdir().unwrap();
        let root = write_plugin(
            tmp.path(),
            "p",
            r#"{"hooks":{"NotARealEvent":[{"hooks":[{"type":"command","command":"echo"}]}]}}"#,
        );
        let mgr = make_manager(vec![DiscoveredPlugin {
            name: "p".into(),
            root,
            source: PluginSource::UserPlaced,
        }]);
        assert!(!mgr.has_hooks(HookEvent::PreToolUse));
    }

    #[test]
    fn loads_matcher_and_command() {
        let tmp = tempfile::tempdir().unwrap();
        let root = write_plugin(
            tmp.path(),
            "p",
            r#"{"hooks":{"PostToolUse":[{"matcher":"developer__.*","hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
        );
        let mgr = make_manager(vec![DiscoveredPlugin {
            name: "p".into(),
            root,
            source: PluginSource::UserPlaced,
        }]);
        assert!(mgr.has_hooks(HookEvent::PostToolUse));
    }

    #[test]
    fn invalid_matcher_skipped_without_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let root = write_plugin(
            tmp.path(),
            "p",
            r#"{"hooks":{"PostToolUse":[{"matcher":"[invalid","hooks":[{"type":"command","command":"echo"}]}]}}"#,
        );
        let mgr = make_manager(vec![DiscoveredPlugin {
            name: "p".into(),
            root,
            source: PluginSource::UserPlaced,
        }]);
        assert!(!mgr.has_hooks(HookEvent::PostToolUse));
    }

    #[tokio::test]
    async fn emit_runs_command_with_plugin_root_substitution() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("ran.txt");
        let marker_path = marker.to_string_lossy().into_owned();
        let hooks = format!(
            r#"{{"hooks":{{"SessionStart":[{{"hooks":[{{"type":"command","command":"sh -c 'echo $PLUGIN_ROOT > {marker}'"}}]}}]}}}}"#,
            marker = marker_path,
        );
        let root = write_plugin(tmp.path(), "p", &hooks);
        let mgr = make_manager(vec![DiscoveredPlugin {
            name: "p".into(),
            root: root.clone(),
            source: PluginSource::UserPlaced,
        }]);

        mgr.emit(
            HookEvent::SessionStart,
            HookContext::new(HookEvent::SessionStart, "session-1"),
        )
        .await;

        let written = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(written.trim(), root.to_string_lossy());
    }

    #[tokio::test]
    async fn matcher_filters_by_tool_name() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("ran.txt");
        let hooks = format!(
            r#"{{"hooks":{{"PreToolUse":[{{"matcher":"developer__shell","hooks":[{{"type":"command","command":"touch {}"}}]}}]}}}}"#,
            marker.to_string_lossy(),
        );
        let root = write_plugin(tmp.path(), "p", &hooks);
        let mgr = make_manager(vec![DiscoveredPlugin {
            name: "p".into(),
            root,
            source: PluginSource::UserPlaced,
        }]);

        // Non-matching tool: marker not created.
        mgr.emit(
            HookEvent::PreToolUse,
            HookContext::new(HookEvent::PreToolUse, "s").with_tool("other__tool", None),
        )
        .await;
        assert!(!marker.exists());

        // Matching tool: marker created.
        mgr.emit(
            HookEvent::PreToolUse,
            HookContext::new(HookEvent::PreToolUse, "s").with_tool("developer__shell", None),
        )
        .await;
        assert!(marker.exists());
    }
}
