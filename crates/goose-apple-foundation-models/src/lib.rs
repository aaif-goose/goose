//! In-process access to Apple's on-device model. Tools are returned to the caller,
//! never executed by this crate. Requires macOS 27 at runtime and Xcode 27 to build.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Unavailable,
    InvalidRequest,
    ContextLengthExceeded,
    Refusal,
    Cancelled,
    Generation,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    pub context_size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Request {
    pub instructions: String,
    pub history: Vec<Entry>,
    pub tools: Vec<Tool>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Entry {
    User {
        text: String,
    },
    Assistant {
        text: String,
    },
    ToolCalls {
        calls: Vec<ToolCall>,
    },
    ToolOutput {
        id: String,
        name: String,
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Serialized separately so Swift can use GeneratedContent's JSON initializer.
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub schema: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    pub entries: Vec<Entry>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cached_tokens: u32,
}

fn invalid(message: impl Into<String>) -> Error {
    Error {
        kind: ErrorKind::InvalidRequest,
        message: message.into(),
    }
}

/// Converts the supported JSON Schema subset to Apple's ordered schema encoding.
/// Unsupported constraints fail explicitly instead of silently changing a tool's contract.
pub fn tool_schema(schema: &Value, name: &str) -> Result<String, Error> {
    fn visit(value: &Value, name: &str) -> Result<Value, Error> {
        let mut object = value
            .as_object()
            .cloned()
            .ok_or_else(|| invalid("Tool schemas must be objects"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "type"
                    | "title"
                    | "description"
                    | "properties"
                    | "required"
                    | "additionalProperties"
                    | "items"
                    | "enum"
                    | "anyOf"
                    | "$defs"
                    | "$ref"
                    | "minimum"
                    | "maximum"
                    | "minItems"
                    | "maxItems"
                    | "pattern"
                    | "format"
                    | "default"
                    | "$schema"
            ) {
                return Err(invalid(format!("Unsupported tool schema keyword: {key}")));
            }
        }
        object.remove("$schema");
        object.remove("default");
        if let Some(format) = object.remove("format") {
            let format = format
                .as_str()
                .ok_or_else(|| invalid("Schema format must be a string"))?;
            // JSON Schema formats are annotations; Apple needs the hint in the
            // description because its schema decoder does not preserve format.
            let description = match object.get("description") {
                Some(Value::String(description)) => format!("{description}\nFormat: {format}."),
                None => format!("Format: {format}."),
                Some(_) => return Err(invalid("Schema description must be a string")),
            };
            object.insert("description".into(), Value::String(description));
        }
        if let Some(Value::Array(types)) = object.get("type") {
            if types.is_empty() || object.contains_key("anyOf") {
                return Err(invalid("Invalid or ambiguous type union"));
            }
            let mut choices = Vec::new();
            for kind in types {
                if !kind.is_string() {
                    return Err(invalid("Schema types must be strings"));
                }
                let mut branch = object.clone();
                branch.insert("type".into(), kind.clone());
                choices.push(visit(
                    &Value::Object(branch),
                    &format!("{name}_{}", kind.as_str().unwrap()),
                )?);
            }
            return Ok(serde_json::json!({"title": name, "anyOf": choices}));
        }
        if object.contains_key("anyOf") || object.contains_key("enum") {
            object
                .entry("title")
                .or_insert_with(|| Value::String(name.into()));
        }
        if object.get("type").and_then(Value::as_str) == Some("object") {
            if object.get("additionalProperties").map_or_else(
                || {
                    object
                        .get("properties")
                        .and_then(Value::as_object)
                        .is_none_or(|p| p.is_empty())
                },
                |v| v != &Value::Bool(false),
            ) {
                return Err(invalid(
                    "Foundation Models requires fixed object properties",
                ));
            }
            let properties = object
                .entry("properties")
                .or_insert_with(|| serde_json::json!({}));
            let properties = properties
                .as_object_mut()
                .ok_or_else(|| invalid("Invalid schema properties"))?;
            for (key, value) in properties.iter_mut() {
                *value = visit(value, &format!("{name}_{key}"))?;
            }
            let order = properties.keys().cloned().map(Value::String).collect();
            object.insert("x-order".into(), Value::Array(order));
            object
                .entry("title")
                .or_insert_with(|| Value::String(name.into()));
            object.insert("additionalProperties".into(), Value::Bool(false));
            object
                .entry("required")
                .or_insert_with(|| serde_json::json!([]));
        }
        if let Some(items) = object.get_mut("items") {
            *items = visit(items, &format!("{name}Item"))?;
        }
        if let Some(choices) = object.get_mut("anyOf") {
            for choice in choices
                .as_array_mut()
                .ok_or_else(|| invalid("Invalid anyOf schema"))?
            {
                *choice = visit(choice, name)?;
            }
        }
        if let Some(definitions) = object.get_mut("$defs") {
            for (key, value) in definitions
                .as_object_mut()
                .ok_or_else(|| invalid("Invalid schema definitions"))?
            {
                *value = visit(value, key)?;
            }
        }
        Ok(Value::Object(object))
    }
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(invalid("Tool parameters must have type object"));
    }
    let schema = visit(schema, name)?.to_string();
    #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "native"))]
    if is_supported() {
        native::validate_schema(&schema)?;
    }
    Ok(schema)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "native"))]
mod native;

/// Whether this build and OS support the provider, independent of model readiness.
pub fn is_supported() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "native"))]
    {
        native::is_supported()
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64", feature = "native")))]
    {
        false
    }
}

/// Checks runtime availability without starting a generation or downloading a model.
pub fn model_info() -> Result<ModelInfo, Error> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "native"))]
    {
        native::model_info()
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64", feature = "native")))]
    {
        Err(Error {
            kind: ErrorKind::Unavailable,
            message: "Apple Foundation Models requires the native feature, macOS 27, and Apple Intelligence".into(),
        })
    }
}

/// Generates one model turn. Dropping this future cancels the native Swift task.
/// The caller must execute returned tools and provide their results in the next request.
pub async fn generate(request: Request) -> Result<Response, Error> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "native"))]
    {
        native::generate(request).await
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64", feature = "native")))]
    {
        let _ = request;
        model_info()?;
        unreachable!()
    }
}
