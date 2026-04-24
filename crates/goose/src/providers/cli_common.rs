use serde_json::Value;

use super::base::{ProviderUsage, Usage};
use super::errors::ProviderError;
use crate::conversation::message::{Message, MessageContent};
use crate::utils::safe_truncate;
use rmcp::model::Role;

pub(crate) fn extract_usage_tokens(usage_info: &Value) -> Usage {
    let get = |key: &str| {
        usage_info
            .get(key)
            .and_then(|v| v.as_i64())
            .and_then(|v| i32::try_from(v).ok())
    };
    Usage::new(
        get("input_tokens"),
        get("output_tokens"),
        get("total_tokens"),
    )
}

pub(crate) fn error_from_event(provider_name: &str, parsed: &Value) -> ProviderError {
    let error_msg = parsed
        .get("error")
        .and_then(|e| e.as_str())
        .or_else(|| parsed.get("message").and_then(|m| m.as_str()))
        .unwrap_or("Unknown error");
    if error_msg.contains("context window exceeded") {
        ProviderError::ContextLengthExceeded(error_msg.to_string())
    } else {
        ProviderError::RequestFailed(format!("{provider_name} error: {error_msg}"))
    }
}

pub(crate) const SESSION_NAME_BEGIN_MARKER: &str = "---BEGIN USER MESSAGES---";
pub(crate) const SESSION_NAME_END_MARKER: &str = "---END USER MESSAGES---";
pub(crate) const SESSION_NAME_SUFFIX: &str = "Generate a short title for the above messages.";

pub(crate) fn is_session_description_request(system: &str) -> bool {
    system.contains("four words or less") || system.contains("4 words or less")
}

pub(crate) fn generate_simple_session_description(
    model_name: &str,
    messages: &[Message],
) -> Result<(Message, ProviderUsage), ProviderError> {
    let description = messages
        .iter()
        .find(|m| m.role == Role::User)
        .and_then(|m| {
            m.content.iter().find_map(|c| match c {
                MessageContent::Text(text_content) => Some(&text_content.text),
                _ => None,
            })
        })
        .map(|text| {
            // Strip the wrapper added by generate_session_name so we get
            // the actual user content. First strip the optional background context section.
            let text = text
                .rfind(SESSION_NAME_BEGIN_MARKER)
                .and_then(|idx| text.get(idx..))
                .unwrap_or(text);
            let stripped = text
                .strip_prefix(SESSION_NAME_BEGIN_MARKER)
                .unwrap_or(text)
                .trim_start_matches(['\n', '\r']);
            let full_suffix = format!("{}\n\n{}", SESSION_NAME_END_MARKER, SESSION_NAME_SUFFIX);
            let stripped = stripped
                .strip_suffix(&full_suffix)
                .or_else(|| stripped.strip_suffix(SESSION_NAME_END_MARKER))
                .unwrap_or(stripped)
                .trim();

            let desc: String = stripped
                .split_whitespace()
                .take(4)
                .collect::<Vec<_>>()
                .join(" ");
            if desc.is_empty() {
                "Simple task".to_string()
            } else {
                safe_truncate(&desc, 100)
            }
        })
        .unwrap_or_else(|| "Simple task".to_string());

    tracing::debug!(
        description = %description,
        "Generated simple session description, skipped subprocess"
    );

    let message = Message::new(
        Role::Assistant,
        chrono::Utc::now().timestamp(),
        vec![MessageContent::text(description)],
    );

    Ok((
        message,
        ProviderUsage::new(model_name.to_string(), Usage::default()),
    ))
}
