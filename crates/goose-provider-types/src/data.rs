//! Portable message and local-tool data, independent of protocol transports.
#[cfg(not(feature = "std"))]
use crate::prelude::*;
use alloc::borrow::Cow;
use core::ops::{Deref, DerefMut};
use serde::{Deserialize, Serialize};
use serde_json::Value;
mod content;
mod resource;
mod tool;
pub use content::*;
pub use resource::*;
pub use tool::*;

pub type JsonObject = serde_json::Map<String, Value>;
pub fn object(value: Value) -> JsonObject {
    match value {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    User,
    Assistant,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ElicitationAction {
    Accept,
    Decline,
    Cancel,
}
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MetaObject(pub JsonObject);
impl MetaObject {
    pub fn new() -> Self {
        Self::default()
    }
}
impl Deref for MetaObject {
    type Target = JsonObject;
    fn deref(&self) -> &JsonObject {
        &self.0
    }
}
impl DerefMut for MetaObject {
    fn deref_mut(&mut self) -> &mut JsonObject {
        &mut self.0
    }
}
impl From<JsonObject> for MetaObject {
    fn from(value: JsonObject) -> Self {
        Self(value)
    }
}
pub type RequestMetaObject = MetaObject;
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<Role>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}
impl Annotations {
    pub fn with_audience(mut self, audience: Vec<Role>) -> Self {
        self.audience = Some(audience);
        self
    }
    pub fn with_priority(mut self, priority: f32) -> Self {
        self.priority = Some(priority);
        self
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconTheme {
    Light,
    Dark,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Icon {
    pub src: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sizes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<IconTheme>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: Role,
    pub content: ContentBlock,
}
impl PromptMessage {
    pub fn new(role: Role, content: ContentBlock) -> Self {
        Self { role, content }
    }
    pub fn new_text(role: Role, text: impl Into<String>) -> Self {
        Self::new(role, ContentBlock::text(text))
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ErrorCode(pub i32);
impl ErrorCode {
    pub const INVALID_REQUEST: Self = Self(-32600);
    pub const INVALID_PARAMS: Self = Self(-32602);
    pub const INTERNAL_ERROR: Self = Self(-32603);
    pub const METHOD_NOT_FOUND: Self = Self(-32601);
    pub const RESOURCE_NOT_FOUND: Self = Self(-32002);
}
impl core::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorData {
    pub code: ErrorCode,
    pub message: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}
impl ErrorData {
    pub fn new(
        code: ErrorCode,
        message: impl Into<Cow<'static, str>>,
        data: Option<Value>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            data,
        }
    }
    pub fn invalid_params(message: impl Into<Cow<'static, str>>, data: Option<Value>) -> Self {
        Self::new(ErrorCode::INVALID_PARAMS, message, data)
    }
    pub fn internal_error(message: impl Into<Cow<'static, str>>, data: Option<Value>) -> Self {
        Self::new(ErrorCode::INTERNAL_ERROR, message, data)
    }
    pub fn invalid_request(message: impl Into<Cow<'static, str>>, data: Option<Value>) -> Self {
        Self::new(ErrorCode::INVALID_REQUEST, message, data)
    }
}
impl core::fmt::Display for ErrorData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl core::error::Error for ErrorData {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolRequestParams {
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaObject>,
    pub name: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<JsonObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_responses: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_state: Option<String>,
}
impl CallToolRequestParams {
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            meta: None,
            arguments: None,
            input_responses: None,
            request_state: None,
        }
    }
    pub fn with_arguments(mut self, arguments: JsonObject) -> Self {
        self.arguments = Some(arguments);
        self
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResultType(pub String);
impl ResultType {
    pub fn is_complete(&self) -> bool {
        self.0 == "complete"
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_type: Option<ResultType>,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaObject>,
}
impl CallToolResult {
    pub fn with_meta(mut self, meta: Option<MetaObject>) -> Self {
        self.meta = meta;
        self
    }
    pub fn success(content: Vec<ContentBlock>) -> Self {
        Self {
            result_type: Some(ResultType("complete".into())),
            content,
            structured_content: None,
            is_error: Some(false),
            meta: None,
        }
    }
    pub fn error(content: Vec<ContentBlock>) -> Self {
        Self {
            is_error: Some(true),
            ..Self::success(content)
        }
    }
    pub fn structured(value: Value) -> Self {
        Self {
            structured_content: Some(value.clone()),
            ..Self::success(vec![ContentBlock::text(value.to_string())])
        }
    }
}
