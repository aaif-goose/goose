//! Runtime-loaded JavaScript implementations of state-machine Operations.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use rmcp::model::Tool;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::js_runtime;
use super::state_machine::{
    ConversationEffect, Emitter, GooseEffect, MachineEffect, Operation, OperationResult, RunStatus,
    SlashCommand, StepResult,
};
use crate::config::{Config, ConfigError};
use crate::conversation::message::{ActionRequiredData, Message, MessageContent};
use crate::conversation::Conversation;
use crate::session::Session;

mod placement;
pub use placement::{place_operations, ConfiguredOperation, OperationPlacement};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const INVOKE: &str = r#"
async function run() {
    if (typeof operation !== "object" || operation === null) {
        throw new TypeError("Define const operation = { name, ...hooks }");
    }
    const hooks = ["run", "run_command", "inference_tools", "prompt_parts", "moim_parts", "finalize_cancellation"];
    if (typeof operation.name !== "string" || operation.name.trim().length === 0) {
        throw new TypeError("operation.name must be a nonempty string");
    }
    for (const key of Object.keys(operation)) {
        if (key !== "name" && (!hooks.includes(key) || typeof operation[key] !== "function")) {
            throw new TypeError("Unknown or invalid operation hook: " + key);
        }
    }
    if (__goose_input.hook === "describe") {
        return { name: operation.name, hooks: hooks.filter(key => typeof operation[key] === "function") };
    }
    return await operation[__goose_input.hook](__goose_input.args);
}
"#;

/// Loads its source once; each hook executes in a fresh, bounded Boa context.
pub struct JavaScriptOperation {
    name: String,
    source: String,
    path: PathBuf,
    hooks: HashSet<String>,
    timeout: Duration,
}

#[derive(Deserialize)]
struct Description {
    name: String,
    hooks: HashSet<String>,
}

impl JavaScriptOperation {
    pub async fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = tokio::fs::canonicalize(path.as_ref())
            .await
            .with_context(|| format!("JavaScript operation {}", path.as_ref().display()))?;
        let source = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("JavaScript operation {}", path.display()))?;
        let source = format!("{source}\n;{INVOKE}");
        let description = js_runtime::execute_json(
            source.clone(),
            path.clone(),
            json!({ "hook": "describe" }),
            DEFAULT_TIMEOUT,
            CancellationToken::new(),
        )
        .await
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("JavaScript operation {}", path.display()))?;
        let description: Description = serde_json::from_value(description)?;
        Ok(Self {
            name: format!("js:{}", description.name),
            source,
            path,
            hooks: description.hooks,
            timeout: DEFAULT_TIMEOUT,
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    async fn invoke<T: DeserializeOwned>(
        &self,
        hook: &str,
        mut args: Value,
        cancellation: CancellationToken,
    ) -> Result<T> {
        args["operation_name"] = json!(self.name);
        let value = js_runtime::execute_json(
            self.source.clone(),
            self.path.clone(),
            json!({ "hook": hook, "args": args }),
            self.timeout,
            cancellation,
        )
        .await
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("{}: {hook} ({})", self.name, self.path.display()))?;
        serde_json::from_value(value)
            .with_context(|| format!("{}: invalid {hook} result", self.name))
    }

    fn input(session: &Session, conversation: Option<&Conversation>) -> Result<Value> {
        let mut session = serde_json::to_value(session)?;
        session
            .as_object_mut()
            .expect("session object")
            .remove("conversation");
        Ok(json!({
            "session": session,
            "conversation": conversation.map(|c| json!({ "messages": c.messages() })),
        }))
    }

    fn effects(&self, effects: Vec<JsEffect>, emit: &Emitter) -> Vec<GooseEffect> {
        effects
            .into_iter()
            .map(|effect| {
                let mut effect = match effect {
                    JsEffect::AppendMessage { message } => {
                        ConversationEffect::AppendMessage(message)
                    }
                    JsEffect::ReplaceConversation { messages } => {
                        ConversationEffect::ReplaceConversation(Conversation::new_unvalidated(
                            messages,
                        ))
                    }
                    JsEffect::PatchToolRequestMeta {
                        tool_call_id,
                        patch,
                    } => ConversationEffect::PatchToolRequestMeta {
                        tool_call_id,
                        patch,
                    },
                    JsEffect::SetMessageOperationNote {
                        message_id,
                        key,
                        value,
                    } => ConversationEffect::SetMessageOperationNote {
                        message_id,
                        operation: self.name.clone(),
                        key,
                        value,
                    },
                    JsEffect::SetMessageVisibility {
                        message_id,
                        user_visible,
                        agent_visible,
                    } => ConversationEffect::SetMessageVisibility {
                        message_id,
                        user_visible,
                        agent_visible,
                    },
                };
                effect.ensure_message_ids();
                if let ConversationEffect::AppendMessage(message) = &effect {
                    // Confirmation events are published by the effect handler after persistence.
                    let confirmation = message.content.iter().any(|content| matches!(content,
                        MessageContent::ActionRequired(action)
                            if matches!(&action.data, ActionRequiredData::ToolConfirmation { .. })
                    ));
                    if !confirmation {
                        emit.message(message.clone());
                    }
                }
                effect.into()
            })
            .collect()
    }

    async fn run_hook(
        &self,
        hook: &str,
        input: Value,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        if !self.hooks.contains(hook) {
            return Ok(OperationResult::NotApplicable);
        }
        let result = self
            .invoke(hook, input, emit.cancel_token().clone())
            .await?;
        Ok(match result {
            JsOperationResult::NotApplicable => OperationResult::NotApplicable,
            JsOperationResult::Applied { status, effects } => {
                OperationResult::Applied(StepResult {
                    effects: self.effects(effects, emit),
                    applied_step: None,
                    status: match status {
                        JsRunStatus::Continuing => RunStatus::Continuing,
                        JsRunStatus::Yielded => RunStatus::Yielded,
                    },
                })
            }
        })
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for JavaScriptOperation {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        self.run_hook("run", Self::input(session, Some(conversation))?, emit)
            .await
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let mut input = Self::input(session, Some(conversation))?;
        input["command"] = json!({ "command": command.command, "params_str": command.params_str });
        self.run_hook("run_command", input, emit).await
    }

    async fn inference_tools(&self, session: &Session) -> Result<Vec<Tool>> {
        if !self.hooks.contains("inference_tools") {
            return Ok(Vec::new());
        }
        self.invoke(
            "inference_tools",
            Self::input(session, session.conversation.as_ref())?,
            CancellationToken::new(),
        )
        .await
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        if !self.hooks.contains("prompt_parts") {
            return Ok(Vec::new());
        }
        self.invoke(
            "prompt_parts",
            Self::input(session, Some(conversation))?,
            CancellationToken::new(),
        )
        .await
    }

    async fn moim_parts(
        &self,
        session: &Session,
        conversation: &Conversation,
    ) -> Result<Vec<String>> {
        if !self.hooks.contains("moim_parts") {
            return Ok(Vec::new());
        }
        self.invoke(
            "moim_parts",
            Self::input(session, Some(conversation))?,
            CancellationToken::new(),
        )
        .await
    }

    async fn finalize_cancellation(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<GooseEffect> {
        if !self.hooks.contains("finalize_cancellation") {
            return Vec::new();
        }
        let result = async {
            let effects = self
                .invoke(
                    "finalize_cancellation",
                    Self::input(session, Some(conversation))?,
                    CancellationToken::new(),
                )
                .await?;
            Ok::<_, anyhow::Error>(self.effects(effects, emit))
        }
        .await;
        match result {
            Ok(effects) => effects,
            Err(error) => {
                tracing::error!(operation = self.name, %error, "JavaScript cancellation finalizer failed");
                Vec::new()
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum JsOperationResult {
    NotApplicable,
    Applied {
        status: JsRunStatus,
        #[serde(default)]
        effects: Vec<JsEffect>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum JsRunStatus {
    Continuing,
    Yielded,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum JsEffect {
    AppendMessage {
        message: Message,
    },
    ReplaceConversation {
        messages: Vec<Message>,
    },
    PatchToolRequestMeta {
        tool_call_id: String,
        patch: Value,
    },
    SetMessageOperationNote {
        message_id: String,
        key: String,
        value: Value,
    },
    SetMessageVisibility {
        message_id: String,
        user_visible: bool,
        agent_visible: bool,
    },
}

/// Loads the configured files in order and rejects duplicate operation identities.
pub async fn load_operations(config: &Config) -> Result<Vec<ConfiguredOperation>> {
    let files = match config.get_param::<Vec<OperationFile>>("GOOSE_JS_OPERATIONS") {
        Ok(files) => files,
        Err(ConfigError::NotFound(_)) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut names = HashSet::new();
    let mut operations = Vec::new();
    for file in files {
        let (path, placement) = match file {
            OperationFile::Path(path) => (path, OperationPlacement::Before("llm".into())),
            OperationFile::Before(file) => (file.path, OperationPlacement::Before(file.before)),
            OperationFile::After(file) => (file.path, OperationPlacement::After(file.after)),
            OperationFile::Replace(file) => (file.path, OperationPlacement::Replace(file.replace)),
        };
        let operation = JavaScriptOperation::from_file(path).await?;
        if !names.insert(operation.name.clone()) {
            bail!("Duplicate JavaScript operation name: {}", operation.name);
        }
        operations.push(ConfiguredOperation {
            operation: Arc::new(operation),
            placement,
        });
    }
    Ok(operations)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OperationFile {
    Path(PathBuf),
    Before(BeforeOperationFile),
    After(AfterOperationFile),
    Replace(ReplaceOperationFile),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BeforeOperationFile {
    path: PathBuf,
    before: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AfterOperationFile {
    path: PathBuf,
    after: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceOperationFile {
    path: PathBuf,
    replace: String,
}
