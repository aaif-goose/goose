use crate::conversation::message::{Message, MessageContent, ProviderMetadata};
use crate::model::{ModelConfig, ThinkingEffort};
use crate::providers::formats::openai;
use rmcp::model::Role;
use serde_json::{json, Value};

pub const REASONING_DETAILS_KEY: &str = "reasoning_details";

fn has_assistant_content(message: &Message) -> bool {
    message.content.iter().any(|c| match c {
        MessageContent::Text(t) => !t.text.is_empty(),
        MessageContent::Image(_) => true,
        MessageContent::ToolRequest(req) => req.tool_call.is_ok(),
        MessageContent::FrontendToolRequest(req) => req.tool_call.is_ok(),
        _ => false,
    })
}

pub fn extract_reasoning_details(response: &Value) -> Option<Vec<Value>> {
    response
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|m| m.get("message"))
        .and_then(|msg| msg.get("reasoning_details"))
        .and_then(|d| d.as_array())
        .cloned()
}

pub fn get_reasoning_details(metadata: &Option<ProviderMetadata>) -> Option<Vec<Value>> {
    metadata
        .as_ref()
        .and_then(|m| m.get(REASONING_DETAILS_KEY))
        .and_then(|v| v.as_array())
        .cloned()
}

pub fn response_to_message(response: &Value) -> anyhow::Result<Message> {
    let mut message = openai::response_to_message(response)?;

    if let Some(details) = extract_reasoning_details(response) {
        for content in &mut message.content {
            if let MessageContent::ToolRequest(req) = content {
                let mut meta = req.metadata.clone().unwrap_or_default();
                meta.insert(REASONING_DETAILS_KEY.to_string(), json!(details));
                req.metadata = Some(meta);
            }
        }
    }

    Ok(message)
}

pub fn add_reasoning_details_to_request(payload: &mut Value, messages: &[Message]) {
    let mut assistant_reasoning: Vec<Option<Vec<Value>>> = messages
        .iter()
        .filter(|m| m.is_agent_visible())
        .filter(|m| m.role == Role::Assistant)
        .filter(|m| has_assistant_content(m))
        .map(|message| {
            message.content.iter().find_map(|c| match c {
                MessageContent::ToolRequest(req) => get_reasoning_details(&req.metadata),
                _ => None,
            })
        })
        .collect();

    if let Some(payload_messages) = payload
        .as_object_mut()
        .and_then(|obj| obj.get_mut("messages"))
        .and_then(|m| m.as_array_mut())
    {
        let mut assistant_idx = 0;
        for payload_msg in payload_messages.iter_mut() {
            if payload_msg.get("role").and_then(|r| r.as_str()) == Some("assistant") {
                if assistant_idx < assistant_reasoning.len() {
                    if let Some(details) = assistant_reasoning
                        .get_mut(assistant_idx)
                        .and_then(|d| d.take())
                    {
                        if let Some(obj) = payload_msg.as_object_mut() {
                            obj.insert("reasoning_details".to_string(), json!(details));
                        }
                    }
                }
                assistant_idx += 1;
            }
        }
    }
}

fn reasoning_effort_for_openrouter(effort: ThinkingEffort) -> &'static str {
    match effort {
        ThinkingEffort::Off => "none",
        ThinkingEffort::Low => "low",
        ThinkingEffort::Medium => "medium",
        ThinkingEffort::High => "high",
        ThinkingEffort::Max => "xhigh",
    }
}

pub fn apply_reasoning_config(payload: &mut Value, model_config: &ModelConfig) {
    let Some(effort) = model_config.thinking_effort() else {
        return;
    };

    if let Some(obj) = payload.as_object_mut() {
        let clamped_effort = obj
            .remove("reasoning_effort")
            .and_then(|value| value.as_str().map(str::to_owned));
        if clamped_effort.is_none() && !model_config.is_reasoning_model() {
            return;
        }

        obj.insert(
            "reasoning".to_string(),
            json!({ "effort": clamped_effort.as_deref().unwrap_or_else(|| reasoning_effort_for_openrouter(effort)) }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_extract_reasoning_details() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": "Hello",
                    "reasoning_details": [
                        {"type": "text", "text": "Let me think..."},
                        {"type": "encrypted", "data": "abc123signature"}
                    ]
                }
            }]
        });

        let details = extract_reasoning_details(&response).unwrap();
        assert_eq!(details.len(), 2);
    }

    #[test]
    fn test_response_to_message_with_tool_calls() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\": \"NYC\"}"
                        }
                    }],
                    "reasoning_details": [
                        {"type": "encrypted", "data": "sig456"}
                    ]
                }
            }]
        });

        let message = response_to_message(&response).unwrap();
        assert!(!message.content.is_empty());

        let tool_request = message
            .content
            .iter()
            .find_map(|c| {
                if let MessageContent::ToolRequest(req) = c {
                    Some(req)
                } else {
                    None
                }
            })
            .unwrap();

        assert!(tool_request.metadata.is_some());
        let details = get_reasoning_details(&tool_request.metadata).unwrap();
        assert_eq!(details.len(), 1);
    }

    #[test]
    fn test_apply_reasoning_config_uses_openrouter_reasoning_object() {
        let mut payload = json!({
            "model": "openai/gpt-5",
            "messages": [],
            "reasoning_effort": "high"
        });
        let mut model_config = ModelConfig::new_or_fail("openai/gpt-5");
        let mut params = HashMap::new();
        params.insert("thinking_effort".to_string(), json!("max"));
        model_config.request_params = Some(params);

        apply_reasoning_config(&mut payload, &model_config);

        assert_eq!(payload["reasoning"], json!({ "effort": "high" }));
        assert!(payload.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_apply_reasoning_config_uses_reasoning_metadata() {
        let mut payload = json!({
            "model": "x-ai/grok-4",
            "messages": []
        });
        let mut model_config = ModelConfig::new_or_fail("x-ai/grok-4");
        let mut params = HashMap::new();
        params.insert("thinking_effort".to_string(), json!("high"));
        model_config.request_params = Some(params);
        model_config.reasoning = Some(true);

        apply_reasoning_config(&mut payload, &model_config);

        assert_eq!(payload["reasoning"], json!({ "effort": "high" }));
    }

    #[test]
    fn test_apply_reasoning_config_uses_model_detection() {
        let mut payload = json!({
            "model": "anthropic/claude-sonnet-4",
            "messages": []
        });
        let mut model_config = ModelConfig::new_or_fail("anthropic/claude-sonnet-4");
        let mut params = HashMap::new();
        params.insert("thinking_effort".to_string(), json!("high"));
        model_config.request_params = Some(params);

        apply_reasoning_config(&mut payload, &model_config);

        assert_eq!(payload["reasoning"], json!({ "effort": "high" }));
    }

    #[test]
    fn test_apply_reasoning_config_skips_non_reasoning_models() {
        let mut payload = json!({
            "model": "openai/gpt-4o",
            "messages": []
        });
        let mut model_config = ModelConfig::new_or_fail("openai/gpt-4o");
        let mut params = HashMap::new();
        params.insert("thinking_effort".to_string(), json!("high"));
        model_config.request_params = Some(params);
        model_config.reasoning = Some(false);

        apply_reasoning_config(&mut payload, &model_config);

        assert!(payload.get("reasoning").is_none());
    }

    #[test]
    fn test_apply_reasoning_config_off_disables_reasoning() {
        let mut payload = json!({
            "model": "x-ai/grok-4",
            "messages": []
        });
        let mut model_config = ModelConfig::new_or_fail("x-ai/grok-4");
        let mut params = HashMap::new();
        params.insert("thinking_effort".to_string(), json!("off"));
        model_config.request_params = Some(params);
        model_config.reasoning = Some(true);

        apply_reasoning_config(&mut payload, &model_config);

        assert_eq!(payload["reasoning"], json!({ "effort": "none" }));
    }
}
