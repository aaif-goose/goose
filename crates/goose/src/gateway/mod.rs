pub mod handler;
pub mod manager;
pub mod pairing;
pub mod telegram;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

use handler::GatewayHandler;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformUser {
    pub platform: String,
    pub user_id: String,
    pub display_name: Option<String>,
}

impl PartialEq for PlatformUser {
    fn eq(&self, other: &Self) -> bool {
        self.platform == other.platform && self.user_id == other.user_id
    }
}

impl Eq for PlatformUser {}

impl std::hash::Hash for PlatformUser {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.platform.hash(state);
        self.user_id.hash(state);
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IncomingMessage {
    pub user: PlatformUser,
    pub text: String,
    pub platform_message_id: Option<String>,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Attachment {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutgoingMessage {
    Text { body: String },
    Typing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PairingState {
    Unpaired,
    PendingCode { code: String, expires_at: i64 },
    Paired { session_id: String, paired_at: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub gateway_type: String,
    pub platform_config: serde_json::Value,
    pub max_sessions: usize,
}

#[async_trait]
#[allow(dead_code)]
pub trait Gateway: Send + Sync + 'static {
    fn gateway_type(&self) -> &str;

    async fn start(&self, handler: GatewayHandler, cancel: CancellationToken)
        -> anyhow::Result<()>;

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()>;

    async fn validate_config(&self) -> anyhow::Result<()>;

    fn info(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

pub fn create_gateway(config: &mut GatewayConfig) -> anyhow::Result<std::sync::Arc<dyn Gateway>> {
    match config.gateway_type.as_str() {
        "telegram" => Ok(std::sync::Arc::new(telegram::TelegramGateway::new(config)?)),
        other => anyhow::bail!("Unknown gateway type: {}", other),
    }
}
