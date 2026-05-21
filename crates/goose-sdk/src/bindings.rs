//! In-process uniffi bindings for the Goose agent.

use std::sync::Arc;
use std::sync::OnceLock;

use futures::StreamExt;
use goose::agents::extension::{Envs, ExtensionConfig};
use goose::agents::types::SessionConfig as CoreSessionConfig;
use goose::agents::{Agent as CoreAgent, AgentEvent as CoreAgentEvent};
use goose::config::{Config, GooseMode, DEFAULT_EXTENSION_TIMEOUT};
use goose::conversation::message::{Message, MessageContent};
use goose::model::ModelConfig;
use goose::providers;
use goose::session::session_manager::SessionType;
use tokio::runtime::Runtime;

pub use goose_sdk_types::{AgentEvent, ExtensionSpec, ProviderSpec};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn rt() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime::new().expect("failed to build tokio runtime"))
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum GooseError {
    #[error("{0}")]
    Generic(String),
}

macro_rules! err_from {
    ($($t:ty),* $(,)?) => {$(
        impl From<$t> for GooseError {
            fn from(e: $t) -> Self { GooseError::Generic(e.to_string()) }
        }
    )*};
}
err_from!(anyhow::Error, goose::model::ConfigError, std::io::Error);

fn extension_spec_into_config(spec: ExtensionSpec) -> ExtensionConfig {
    match spec {
        ExtensionSpec::Builtin { name } => ExtensionConfig::Builtin {
            name,
            description: String::new(),
            display_name: None,
            timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
            bundled: Some(true),
            available_tools: vec![],
        },
        ExtensionSpec::Stdio {
            name,
            cmd,
            args,
            envs,
        } => ExtensionConfig::Stdio {
            name,
            description: String::new(),
            cmd,
            args,
            envs: Envs::new(envs),
            env_keys: vec![],
            timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
            bundled: None,
            available_tools: vec![],
        },
        ExtensionSpec::StreamableHttp { name, uri, headers } => ExtensionConfig::StreamableHttp {
            name,
            description: String::new(),
            uri,
            envs: Envs::new(std::collections::HashMap::new()),
            env_keys: vec![],
            headers,
            timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
            socket: None,
            bundled: None,
            available_tools: vec![],
        },
    }
}

#[uniffi::export(callback_interface)]
pub trait EventSink: Send + Sync {
    fn on_event(&self, event: AgentEvent);
    fn on_error(&self, error: String);
    fn on_done(&self);
}

#[derive(uniffi::Object)]
pub struct Agent {
    inner: Arc<CoreAgent>,
    session_id: tokio::sync::Mutex<Option<String>>,
}

#[uniffi::export]
impl Agent {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        let inner = rt().block_on(async { Arc::new(CoreAgent::new()) });
        Arc::new(Self {
            inner,
            session_id: tokio::sync::Mutex::new(None),
        })
    }

    pub fn configure(
        &self,
        provider: ProviderSpec,
        extensions: Vec<ExtensionSpec>,
    ) -> Result<String, GooseError> {
        rt().block_on(async {
            let cfg = Config::global();
            let provider_name = provider
                .name
                .or_else(|| cfg.get_goose_provider().ok())
                .ok_or_else(|| {
                    GooseError::Generic(
                        "no provider: set ProviderSpec.name or run `goose configure`".into(),
                    )
                })?;
            let model_name = provider
                .model
                .or_else(|| cfg.get_goose_model().ok())
                .ok_or_else(|| {
                    GooseError::Generic(
                        "no model: set ProviderSpec.model or run `goose configure`".into(),
                    )
                })?;

            let cwd = std::env::current_dir()?;
            let session = self
                .inner
                .config
                .session_manager
                .create_session(
                    cwd,
                    "uniffi-sdk".to_string(),
                    SessionType::User,
                    GooseMode::default(),
                )
                .await?;

            let ext_configs: Vec<ExtensionConfig> = extensions
                .into_iter()
                .map(extension_spec_into_config)
                .collect();

            let model_config = ModelConfig::new(&model_name)?.with_canonical_limits(&provider_name);
            let prov = providers::create(&provider_name, model_config, ext_configs.clone()).await?;
            self.inner.update_provider(prov, &session.id).await?;

            if !ext_configs.is_empty() {
                let results = self
                    .inner
                    .add_extensions_bulk(ext_configs, &session.id)
                    .await?;
                for r in &results {
                    if !r.success {
                        return Err(GooseError::Generic(format!(
                            "extension {} failed to load: {}",
                            r.name,
                            r.error.clone().unwrap_or_default()
                        )));
                    }
                }
            }

            *self.session_id.lock().await = Some(session.id.clone());
            Ok(session.id)
        })
    }

    pub fn reply(&self, prompt: String, sink: Box<dyn EventSink>) -> Result<(), GooseError> {
        rt().block_on(async {
            let session_id = self
                .session_id
                .lock()
                .await
                .clone()
                .ok_or_else(|| GooseError::Generic("call configure() first".into()))?;

            let session_config = CoreSessionConfig {
                id: session_id,
                schedule_id: None,
                max_turns: None,
                retry_config: None,
            };

            let user_message = Message::user().with_text(&prompt);

            let mut stream = self.inner.reply(user_message, session_config, None).await?;

            while let Some(item) = stream.next().await {
                match item {
                    Ok(CoreAgentEvent::Message(msg)) => {
                        for content in &msg.content {
                            if let Some(ev) = content_to_event(content) {
                                sink.on_event(ev);
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        sink.on_error(e.to_string());
                        return Err(e.into());
                    }
                }
            }
            sink.on_done();
            Ok(())
        })
    }
}

fn content_to_event(content: &MessageContent) -> Option<AgentEvent> {
    use rmcp::model::RawContent;
    match content {
        MessageContent::Text(t) => Some(AgentEvent::AssistantText {
            text: t.text.clone(),
        }),
        MessageContent::Thinking(t) => Some(AgentEvent::Thinking {
            text: t.thinking.clone(),
        }),
        MessageContent::ToolRequest(req) => {
            let (name, arguments) = match &req.tool_call {
                Ok(call) => (
                    call.name.to_string(),
                    call.arguments
                        .as_ref()
                        .map(|a| serde_json::to_string(a).unwrap_or_default())
                        .unwrap_or_default(),
                ),
                Err(e) => ("<error>".into(), e.to_string()),
            };
            Some(AgentEvent::ToolRequest {
                id: req.id.clone(),
                name,
                arguments,
            })
        }
        MessageContent::ToolResponse(resp) => {
            let (output, is_error) = match &resp.tool_result {
                Ok(result) => {
                    let text = result
                        .content
                        .iter()
                        .filter_map(|c| match &c.raw {
                            RawContent::Text(t) => Some(t.text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    (text, result.is_error.unwrap_or(false))
                }
                Err(e) => (e.to_string(), true),
            };
            Some(AgentEvent::ToolResponse {
                id: resp.id.clone(),
                output,
                is_error,
            })
        }
        _ => None,
    }
}
