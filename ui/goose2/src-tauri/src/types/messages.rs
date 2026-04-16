use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub role: MessageRole,
    pub created: i64,
    pub content: Vec<MessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MessageContent {
    Text {
        text: String,
    },
    Image {
        source: ImageSource,
    },
    ToolRequest {
        id: String,
        name: String,
        arguments: serde_json::Value,
        status: ToolCallStatus,
    },
    ToolResponse {
        id: String,
        name: String,
        result: String,
        #[serde(rename = "isError")]
        is_error: bool,
    },
    Thinking {
        text: String,
    },
    RedactedThinking {},
    Reasoning {
        text: String,
    },
    ActionRequired {
        id: String,
        #[serde(rename = "actionType")]
        action_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    SystemNotification {
        #[serde(rename = "notificationType")]
        notification_type: String,
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolCallStatus {
    Pending,
    Executing,
    Completed,
    Error,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MessageMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<MessageAttachment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_persona_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_persona_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_status: Option<MessageCompletionStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageCompletionStatus {
    InProgress,
    Completed,
    Error,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageAttachment {
    #[serde(rename = "type")]
    pub attachment_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenState {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub accumulated_input: u64,
    pub accumulated_output: u64,
    pub accumulated_total: u64,
}
