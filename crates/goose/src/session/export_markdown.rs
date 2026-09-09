use crate::conversation::message::{
    ActionRequiredData, Message, MessageContent, ToolNameParts, ToolRequest, ToolResponse,
};
use crate::utils::safe_truncate;
use rmcp::model::{ContentBlock, ResourceContents, Role};
use serde_json::Value;

const MAX_STRING_LENGTH_MD_EXPORT: usize = 4096; // Generous limit for export
const REDACTED_PREFIX_LENGTH: usize = 100; // Show first 100 chars before trimming

fn value_to_simple_markdown_string(value: &Value, export_full_strings: bool) -> String {
    match value {
        Value::String(s) => {
            if !export_full_strings && s.chars().count() > MAX_STRING_LENGTH_MD_EXPORT {
                let prefix = safe_truncate(s, REDACTED_PREFIX_LENGTH);
                let trimmed_chars = s.chars().count() - prefix.chars().count();
                format!("`{}[ ... trimmed : {} chars ... ]`", prefix, trimmed_chars)
            } else {
                // Escape backticks and newlines for inline code.
                let escaped = s.replace('`', "\\`").replace("\n", "\\\\n");
                format!("`{}`", escaped)
            }
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => format!("*{}*", b),
        Value::Null => "_null_".to_string(),
        _ => "`[Complex Value]`".to_string(),
    }
}

fn value_to_markdown(value: &Value, depth: usize, export_full_strings: bool) -> String {
    let mut md_string = String::new();
    let base_indent_str = "  ".repeat(depth); // Basic indentation for nesting

    match value {
        Value::Object(map) => {
            if map.is_empty() {
                md_string.push_str(&format!("{}*empty object*\n", base_indent_str));
            } else {
                for (key, val) in map {
                    md_string.push_str(&format!("{}*   **{}**: ", base_indent_str, key));
                    match val {
                        Value::String(s) => {
                            if s.contains('\n') || s.chars().count() > 80 {
                                // Heuristic for block
                                md_string.push_str(&format!(
                                    "\n{}    ```\n{}{}\n{}    ```\n",
                                    base_indent_str,
                                    base_indent_str,
                                    s.trim(),
                                    base_indent_str
                                ));
                            } else {
                                md_string.push_str(&format!("`{}`\n", s.replace('`', "\\`")));
                            }
                        }
                        _ => {
                            // Use recursive call for all values including complex objects/arrays
                            md_string.push('\n');
                            md_string.push_str(&value_to_markdown(
                                val,
                                depth + 2,
                                export_full_strings,
                            ));
                        }
                    }
                }
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                md_string.push_str(&format!("{}*   *empty list*\n", base_indent_str));
            } else {
                for item in arr {
                    md_string.push_str(&format!("{}*   - ", base_indent_str));
                    match item {
                        Value::String(s) => {
                            if s.contains('\n') || s.chars().count() > 80 {
                                // Heuristic for block
                                md_string.push_str(&format!(
                                    "\n{}      ```\n{}{}\n{}      ```\n",
                                    base_indent_str,
                                    base_indent_str,
                                    s.trim(),
                                    base_indent_str
                                ));
                            } else {
                                md_string.push_str(&format!("`{}`\n", s.replace('`', "\\`")));
                            }
                        }
                        _ => {
                            // Use recursive call for all values including complex objects/arrays
                            md_string.push('\n');
                            md_string.push_str(&value_to_markdown(
                                item,
                                depth + 2,
                                export_full_strings,
                            ));
                        }
                    }
                }
            }
        }
        _ => {
            md_string.push_str(&format!(
                "{}{}\n",
                base_indent_str,
                value_to_simple_markdown_string(value, export_full_strings)
            ));
        }
    }
    md_string
}

fn is_shell_tool_name(tool_name: &str) -> bool {
    matches!(tool_name, "shell")
}

fn is_developer_file_tool_name(tool_name: &str) -> bool {
    matches!(tool_name, "write" | "edit")
}

pub fn tool_request_to_markdown(req: &ToolRequest, export_all_content: bool) -> String {
    let mut md = String::new();
    match &req.tool_call {
        Ok(call) => {
            let name_parts = ToolNameParts::from(call.name.as_ref());
            let namespace = match name_parts.extension_name {
                Some(extension_name) => extension_name,
                None if is_shell_tool_name(call.name.as_ref())
                    || is_developer_file_tool_name(call.name.as_ref()) =>
                {
                    "developer"
                }
                None => "Tool",
            };

            md.push_str(&format!(
                "#### Tool Call: `{}` (namespace: `{}`)\n",
                name_parts.tool_name, namespace
            ));
            md.push_str("**Arguments:**\n");

            match call.name.as_ref() {
                name if is_shell_tool_name(name) => {
                    if let Some(Value::String(command)) =
                        call.arguments.as_ref().and_then(|args| args.get("command"))
                    {
                        md.push_str(&format!(
                            "*   **command**:\n    ```sh\n    {}\n    ```\n",
                            command.trim()
                        ));
                    }
                    let other_args: serde_json::Map<String, Value> = call
                        .arguments
                        .as_ref()
                        .map(|obj| {
                            obj.iter()
                                .filter(|(k, _)| k.as_str() != "command")
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    if !other_args.is_empty() {
                        md.push_str(&value_to_markdown(
                            &Value::Object(other_args),
                            0,
                            export_all_content,
                        ));
                    }
                }
                name if is_developer_file_tool_name(name) => {
                    if let Some(Value::String(path)) =
                        call.arguments.as_ref().and_then(|args| args.get("path"))
                    {
                        md.push_str(&format!("*   **path**: `{}`\n", path));
                    }

                    if let Some(args) = &call.arguments {
                        let mut other_args = args.clone();
                        other_args.remove("path");
                        if !other_args.is_empty() {
                            md.push_str(&value_to_markdown(
                                &Value::Object(other_args),
                                0,
                                export_all_content,
                            ));
                        }
                    } else {
                        md.push_str("*No arguments*\n");
                    }
                }
                _ => {
                    if let Some(args) = &call.arguments {
                        md.push_str(&value_to_markdown(
                            &Value::Object(args.clone()),
                            0,
                            export_all_content,
                        ));
                    } else {
                        md.push_str("*No arguments*\n");
                    }
                }
            }
        }
        Err(e) => {
            md.push_str(&format!(
                "**Error in Tool Call:**\n```\n{}
```\n",
                e
            ));
        }
    }
    md
}

#[cfg(test)]
pub fn tool_response_to_markdown(resp: &ToolResponse, export_all_content: bool) -> String {
    let audience = (!export_all_content).then_some(Role::Assistant);
    tool_response_to_markdown_for_audience(resp, audience)
}

fn tool_response_to_markdown_for_audience(resp: &ToolResponse, audience: Option<Role>) -> String {
    let mut md = String::new();
    md.push_str("#### Tool Response:\n");

    match &resp.tool_result {
        Ok(result) => {
            if result.content.is_empty() {
                md.push_str("*No textual output from tool.*\n");
            }

            for content in &result.content {
                if let Some(ref role) = audience {
                    let content_audience = match content {
                        ContentBlock::Text(t) => {
                            t.annotations.as_ref().and_then(|a| a.audience.as_ref())
                        }
                        ContentBlock::Image(i) => {
                            i.annotations.as_ref().and_then(|a| a.audience.as_ref())
                        }
                        ContentBlock::Audio(a) => {
                            a.annotations.as_ref().and_then(|a| a.audience.as_ref())
                        }
                        ContentBlock::Resource(r) => {
                            r.annotations.as_ref().and_then(|a| a.audience.as_ref())
                        }
                        ContentBlock::ResourceLink(r) => {
                            r.annotations.as_ref().and_then(|a| a.audience.as_ref())
                        }
                        _ => None,
                    };
                    if let Some(content_audience) = content_audience {
                        if !content_audience.contains(role) {
                            continue;
                        }
                    }
                }

                match content {
                    ContentBlock::Text(text_content) => {
                        let trimmed_text = text_content.text.trim();
                        if (trimmed_text.starts_with('{') && trimmed_text.ends_with('}'))
                            || (trimmed_text.starts_with('[') && trimmed_text.ends_with(']'))
                        {
                            md.push_str(&format!("```json\n{}\n```\n", trimmed_text));
                        } else if trimmed_text.starts_with('<')
                            && trimmed_text.ends_with('>')
                            && trimmed_text.contains("</")
                        {
                            md.push_str(&format!("```xml\n{}\n```\n", trimmed_text));
                        } else {
                            md.push_str(&text_content.text);
                            md.push_str("\n\n");
                        }
                    }
                    ContentBlock::Image(image_content) => {
                        if image_content.mime_type.starts_with("image/") {
                            // For actual images, provide a placeholder that indicates it's an image
                            md.push_str(&format!(
                                "**Image:** `(type: {}, data: first 30 chars of base64...)`\n\n",
                                image_content.mime_type
                            ));
                        } else {
                            // For non-image mime types, just indicate it's binary data
                            md.push_str(&format!(
                                "**Binary Content:** `(type: {}, length: {} bytes)`\n\n",
                                image_content.mime_type,
                                image_content.data.len()
                            ));
                        }
                    }
                    ContentBlock::Resource(resource) => {
                        match &resource.resource {
                            ResourceContents::TextResourceContents {
                                uri,
                                mime_type,
                                text,
                                meta: _,
                            } => {
                                // Extract file extension from the URI for syntax highlighting
                                let file_extension = uri.split('.').next_back().unwrap_or("");
                                let syntax_type = match file_extension {
                                    "rs" => "rust",
                                    "js" => "javascript",
                                    "ts" => "typescript",
                                    "py" => "python",
                                    "json" => "json",
                                    "yaml" | "yml" => "yaml",
                                    "md" => "markdown",
                                    "html" => "html",
                                    "css" => "css",
                                    "sh" => "bash",
                                    _ => mime_type
                                        .as_ref()
                                        .map(|mime| if mime == "text" { "" } else { mime })
                                        .unwrap_or(""),
                                };

                                md.push_str(&format!("**File:** `{}`\n", uri));
                                md.push_str(&format!(
                                    "```{}\n{}\n```\n\n",
                                    syntax_type,
                                    text.trim()
                                ));
                            }
                            ResourceContents::BlobResourceContents {
                                uri,
                                mime_type,
                                blob,
                                ..
                            } => {
                                md.push_str(&format!(
                                    "**Binary File:** `{}` (type: {}, {} bytes)\n\n",
                                    uri,
                                    mime_type.as_ref().map(|s| s.as_str()).unwrap_or("unknown"),
                                    blob.len()
                                ));
                            }
                            _ => {}
                        }
                    }
                    ContentBlock::ResourceLink(_link) => {
                        // Show a simple placeholder for resource links when exporting
                        md.push_str("[resource link]\n\n");
                    }
                    ContentBlock::Audio(_) => {
                        md.push_str("[audio content not displayed in Markdown export]\n\n")
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            md.push_str(&format!(
                "**Error in Tool Response:**\n```\n{}
```\n",
                e
            ));
        }
    }
    md
}

pub fn message_to_markdown(message: &Message, export_all_content: bool) -> String {
    let audience = (!export_all_content).then_some(Role::Assistant);
    message_to_markdown_for_audience(message, export_all_content, audience)
}

pub fn user_projected_message_to_markdown(message: &Message) -> String {
    message_to_markdown_for_audience(message, false, Some(Role::User))
}

fn message_to_markdown_for_audience(
    message: &Message,
    export_all_content: bool,
    audience: Option<Role>,
) -> String {
    let mut md = String::new();
    for content in &message.content {
        match content {
            MessageContent::ActionRequired(action) => match &action.data {
                ActionRequiredData::ToolConfirmation { tool_name, .. } => {
                    md.push_str(&format!(
                        "**Action Required** (tool_confirmation): {}\n\n",
                        tool_name
                    ));
                }
                ActionRequiredData::Elicitation { message, .. } => {
                    md.push_str(&format!(
                        "**Action Required** (elicitation): {}\n\n",
                        message
                    ));
                }
                ActionRequiredData::ElicitationResponse { id, user_data, .. } => {
                    md.push_str(&format!(
                        "**Action Required** (elicitation_response): {}\n```json\n{}\n```\n\n",
                        id,
                        serde_json::to_string_pretty(user_data)
                            .unwrap_or_else(|_| "{}".to_string())
                    ));
                }
                ActionRequiredData::ToolConfirmationResponse { id, permission } => {
                    md.push_str(&format!(
                        "**Action Required** (tool_confirmation_response): {} {:?}\n\n",
                        id, permission
                    ));
                }
            },
            MessageContent::Text(text) => {
                md.push_str(&text.text);
                md.push_str("\n\n");
            }
            MessageContent::ToolRequest(req) => {
                md.push_str(&tool_request_to_markdown(req, export_all_content));
                md.push('\n');
            }
            MessageContent::ToolResponse(resp) => {
                md.push_str(&tool_response_to_markdown_for_audience(
                    resp,
                    audience.clone(),
                ));
                md.push('\n');
            }
            MessageContent::Image(image) => {
                md.push_str(&format!(
                    "**Image:** `(type: {}, data placeholder: {}...)`\n\n",
                    image.mime_type,
                    image.data.chars().take(30).collect::<String>()
                ));
            }
            MessageContent::Thinking(thinking) => {
                md.push_str("**Thinking:**\n");
                md.push_str("> ");
                md.push_str(&thinking.thinking.replace("\n", "\n> "));
                md.push_str("\n\n");
            }
            MessageContent::RedactedThinking(_) => {
                md.push_str("**Thinking:**\n");
                md.push_str("> *Thinking was redacted*\n\n");
            }
            MessageContent::Error(error) => {
                md.push_str(&format!("**Error**: {}\n\n", error.message));
            }
            MessageContent::SystemNotification(notification) => {
                md.push_str(&format!("*{}*\n\n", notification.msg));
            }
            _ => {
                md.push_str(
                    "`WARNING: Message content type could not be rendered to Markdown`\n\n",
                );
            }
        }
    }
    md.trim_end_matches("\n").to_string()
}

pub fn export_session_to_markdown(messages: Vec<Message>, session_name: &str) -> String {
    let mut markdown_output = String::new();

    markdown_output.push_str(&format!("# Session Export: {}\n\n", session_name));

    if messages.is_empty() {
        markdown_output.push_str("*(This session has no messages)*\n");
        return markdown_output;
    }

    markdown_output.push_str(&format!("*Total messages: {}*\n\n---\n\n", messages.len()));

    let mut skip_next_if_tool_response = false;

    for message in &messages {
        let is_only_tool_response = message.role == Role::User
            && message
                .content
                .iter()
                .all(|content| matches!(content, MessageContent::ToolResponse(_)));

        if skip_next_if_tool_response && is_only_tool_response {
            markdown_output.push_str(&user_projected_message_to_markdown(message));
            markdown_output.push_str("\n\n---\n\n");
            skip_next_if_tool_response = false;
            continue;
        }

        skip_next_if_tool_response = false;

        if !is_only_tool_response {
            let role_prefix = match message.role {
                Role::User => "### User:\n",
                Role::Assistant => "### Assistant:\n",
            };
            markdown_output.push_str(role_prefix);
        }

        markdown_output.push_str(&user_projected_message_to_markdown(message));
        markdown_output.push_str("\n\n---\n\n");

        if message
            .content
            .iter()
            .any(|content| matches!(content, MessageContent::ToolRequest(_)))
        {
            skip_next_if_tool_response = true;
        }
    }

    markdown_output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{Message, ToolRequest, ToolResponse};
    use crate::conversation::Conversation;
    use rmcp::model::{Annotations, CallToolRequestParams, ContentBlock, TextContent};
    use rmcp::object;
    use serde_json::json;

    #[test]
    fn test_value_to_simple_markdown_string_normal() {
        let value = json!("hello world");
        let result = value_to_simple_markdown_string(&value, true);
        assert_eq!(result, "`hello world`");
    }

    #[test]
    fn test_value_to_simple_markdown_string_with_backticks() {
        let value = json!("hello `world`");
        let result = value_to_simple_markdown_string(&value, true);
        assert_eq!(result, "`hello \\`world\\``");
    }

    #[test]
    fn test_value_to_simple_markdown_string_long_string_full_export() {
        let long_string = "a".repeat(5000);
        let value = json!(long_string);
        let result = value_to_simple_markdown_string(&value, true);
        // When export_full_strings is true, should return full string
        assert!(result.starts_with("`"));
        assert!(result.ends_with("`"));
        assert!(result.contains(&"a".repeat(5000)));
    }

    #[test]
    fn test_value_to_simple_markdown_string_long_string_trimmed() {
        let long_string = "a".repeat(5000);
        let value = json!(long_string);
        let result = value_to_simple_markdown_string(&value, false);
        // When export_full_strings is false, should trim long strings
        assert!(result.starts_with("`"));
        assert!(result.contains("[ ... trimmed : "));
        assert!(result.contains("4900 chars ... ]`"));
        assert!(result.contains(&"a".repeat(97))); // Should contain the prefix (100 - 3 for "...")
    }

    #[test]
    fn test_value_to_simple_markdown_string_numbers_and_bools() {
        assert_eq!(value_to_simple_markdown_string(&json!(42), true), "42");
        assert_eq!(
            value_to_simple_markdown_string(&json!(true), true),
            "*true*"
        );
        assert_eq!(
            value_to_simple_markdown_string(&json!(false), true),
            "*false*"
        );
        assert_eq!(
            value_to_simple_markdown_string(&json!(null), true),
            "_null_"
        );
    }

    #[test]
    fn test_value_to_markdown_empty_object() {
        let value = json!({});
        let result = value_to_markdown(&value, 0, true);
        assert!(result.contains("*empty object*"));
    }

    #[test]
    fn test_value_to_markdown_empty_array() {
        let value = json!([]);
        let result = value_to_markdown(&value, 0, true);
        assert!(result.contains("*empty list*"));
    }

    #[test]
    fn test_value_to_markdown_simple_object() {
        let value = json!({
            "name": "test",
            "count": 42,
            "active": true
        });
        let result = value_to_markdown(&value, 0, true);
        assert!(result.contains("**name**"));
        assert!(result.contains("`test`"));
        assert!(result.contains("**count**"));
        assert!(result.contains("42"));
        assert!(result.contains("**active**"));
        assert!(result.contains("*true*"));
    }

    #[test]
    fn test_value_to_markdown_nested_object() {
        let value = json!({
            "user": {
                "name": "Alice",
                "age": 30
            }
        });
        let result = value_to_markdown(&value, 0, true);
        assert!(result.contains("**user**"));
        assert!(result.contains("**name**"));
        assert!(result.contains("`Alice`"));
        assert!(result.contains("**age**"));
        assert!(result.contains("30"));
    }

    #[test]
    fn test_value_to_markdown_array_with_items() {
        let value = json!(["item1", "item2", 42]);
        let result = value_to_markdown(&value, 0, true);
        assert!(result.contains("- `item1`"));
        assert!(result.contains("- `item2`"));
        // Numbers are handled by recursive call, so they get formatted differently
        assert!(result.contains("42"));
    }

    #[test]
    fn test_tool_request_to_markdown_shell() {
        let tool_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "ls -la",
            "working_dir": "/home/user"
        }));
        let tool_request = ToolRequest {
            id: "test-id".to_string(),
            tool_call: Ok(tool_call),
            metadata: None,
            tool_meta: None,
        };

        let result = tool_request_to_markdown(&tool_request, true);
        assert!(result.contains("#### Tool Call: `shell`"));
        assert!(result.contains("namespace: `developer`"));
        assert!(result.contains("**command**:"));
        assert!(result.contains("```sh"));
        assert!(result.contains("ls -la"));
        assert!(result.contains("**working_dir**"));
    }

    #[test]
    fn test_tool_request_to_markdown_edit() {
        let tool_call = CallToolRequestParams::new("edit").with_arguments(object!({
            "path": "/path/to/file.txt",
            "before": "Hello",
            "after": "World"
        }));
        let tool_request = ToolRequest {
            id: "test-id".to_string(),
            tool_call: Ok(tool_call),
            metadata: None,
            tool_meta: None,
        };

        let result = tool_request_to_markdown(&tool_request, true);
        assert!(result.contains("#### Tool Call: `edit`"));
        assert!(result.contains("namespace: `developer`"));
        assert!(result.contains("**path**: `/path/to/file.txt`"));
        assert!(result.contains("**before**"));
        assert!(result.contains("**after**"));
    }

    #[test]
    fn test_tool_request_to_markdown_splits_at_first_separator() {
        let tool_request = ToolRequest {
            id: "test-id".to_string(),
            tool_call: Ok(CallToolRequestParams::new("calendar__events__list")),
            metadata: None,
            tool_meta: None,
        };

        let result = tool_request_to_markdown(&tool_request, true);

        assert!(result.contains("#### Tool Call: `events__list`"));
        assert!(result.contains("namespace: `calendar`"));
    }

    #[test]
    fn test_tool_response_to_markdown_text() {
        let text_content = TextContent::new("Command executed successfully".to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "test-id".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let result = tool_response_to_markdown(&tool_response, true);
        assert!(result.contains("#### Tool Response:"));
        assert!(result.contains("Command executed successfully"));
    }

    #[test]
    fn test_tool_response_to_markdown_json() {
        let json_text = r#"{"status": "success", "data": "test"}"#;
        let text_content = TextContent::new(json_text.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "test-id".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let result = tool_response_to_markdown(&tool_response, true);
        assert!(result.contains("#### Tool Response:"));
        assert!(result.contains("```json"));
        assert!(result.contains(json_text));
    }

    #[test]
    fn test_message_to_markdown_text() {
        let message = Message::user().with_text("Hello, this is a test message");

        let result = message_to_markdown(&message, true);
        assert_eq!(result, "Hello, this is a test message");
    }

    #[test]
    fn test_message_to_markdown_with_tool_request() {
        let tool_call =
            CallToolRequestParams::new("test_tool").with_arguments(object!({"param": "value"}));

        let message = Message::assistant().with_tool_request("test-id", Ok(tool_call));

        let result = message_to_markdown(&message, true);
        assert!(result.contains("#### Tool Call: `test_tool`"));
        assert!(result.contains("**param**"));
    }

    #[test]
    fn test_message_to_markdown_thinking() {
        let message = Message::assistant()
            .with_thinking("I need to analyze this problem...", "test-signature");

        let result = message_to_markdown(&message, true);
        assert!(result.contains("**Thinking:**"));
        assert!(result.contains("> I need to analyze this problem..."));
    }

    #[test]
    fn test_message_to_markdown_redacted_thinking() {
        let message = Message::assistant().with_redacted_thinking("redacted-data");

        let result = message_to_markdown(&message, true);
        assert!(result.contains("**Thinking:**"));
        assert!(result.contains("> *Thinking was redacted*"));
    }

    #[test]
    fn test_recursive_value_to_markdown() {
        // Test that complex nested structures are properly handled with recursion
        let value = json!({
            "level1": {
                "level2": {
                    "data": "nested value"
                },
                "array": [
                    {"item": "first"},
                    {"item": "second"}
                ]
            }
        });

        let result = value_to_markdown(&value, 0, true);
        assert!(result.contains("**level1**"));
        assert!(result.contains("**level2**"));
        assert!(result.contains("**data**"));
        assert!(result.contains("`nested value`"));
        assert!(result.contains("**array**"));
        assert!(result.contains("**item**"));
        assert!(result.contains("`first`"));
        assert!(result.contains("`second`"));
    }

    #[test]
    fn test_shell_tool_with_code_output() {
        let tool_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "cat main.py"
        }));
        let tool_request = ToolRequest {
            id: "shell-cat".to_string(),
            tool_call: Ok(tool_call),
            metadata: None,
            tool_meta: None,
        };

        let python_code = r#"#!/usr/bin/env python3
def hello_world():
    print("Hello, World!")

if __name__ == "__main__":
    hello_world()"#;

        let text_content = TextContent::new(python_code.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "shell-cat".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let request_result = tool_request_to_markdown(&tool_request, true);
        let response_result = tool_response_to_markdown(&tool_response, true);

        // Check request formatting
        assert!(request_result.contains("#### Tool Call: `shell`"));
        assert!(request_result.contains("```sh"));
        assert!(request_result.contains("cat main.py"));

        // Check response formatting - text content is output as plain text
        assert!(response_result.contains("#### Tool Response:"));
        assert!(response_result.contains("def hello_world():"));
        assert!(response_result.contains("print(\"Hello, World!\")"));
    }

    #[test]
    fn test_shell_tool_with_git_commands() {
        let git_status_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "git status --porcelain"
        }));
        let tool_request = ToolRequest {
            id: "git-status".to_string(),
            tool_call: Ok(git_status_call),
            metadata: None,
            tool_meta: None,
        };

        let git_output = " M src/main.rs\n?? temp.txt\n A new_feature.rs";
        let text_content = TextContent::new(git_output.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "git-status".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let request_result = tool_request_to_markdown(&tool_request, true);
        let response_result = tool_response_to_markdown(&tool_response, true);

        // Check request formatting
        assert!(request_result.contains("git status --porcelain"));
        assert!(request_result.contains("```sh"));

        // Check response formatting - git output as plain text
        assert!(response_result.contains("M src/main.rs"));
        assert!(response_result.contains("?? temp.txt"));
    }

    #[test]
    fn test_shell_tool_with_build_output() {
        let cargo_build_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "cargo build"
        }));
        let _tool_request = ToolRequest {
            id: "cargo-build".to_string(),
            tool_call: Ok(cargo_build_call),
            metadata: None,
            tool_meta: None,
        };

        let build_output = r#"   Compiling goose-cli v0.1.0 (/Users/user/goose)
warning: unused variable `x`
 --> src/main.rs:10:9
   |
10 |     let x = 5;
   |         ^ help: if this is intentional, prefix it with an underscore: `_x`
   |
   = note: `#[warn(unused_variables)]` on by default

    Finished dev [unoptimized + debuginfo] target(s) in 2.45s"#;

        let text_content = TextContent::new(build_output.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "cargo-build".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let response_result = tool_response_to_markdown(&tool_response, true);

        // Should format as plain text since it's build output, not code
        assert!(response_result.contains("Compiling goose-cli"));
        assert!(response_result.contains("warning: unused variable"));
        assert!(response_result.contains("Finished dev"));
    }

    #[test]
    fn test_shell_tool_with_json_api_response() {
        let curl_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "curl -s https://api.github.com/repos/microsoft/vscode/releases/latest"
        }));
        let _tool_request = ToolRequest {
            id: "curl-api".to_string(),
            tool_call: Ok(curl_call),
            metadata: None,
            tool_meta: None,
        };

        let api_response = r#"{
  "url": "https://api.github.com/repos/microsoft/vscode/releases/90543298",
  "tag_name": "1.85.0",
  "name": "1.85.0",
  "published_at": "2023-12-07T16:54:32Z",
  "assets": [
    {
      "name": "VSCode-darwin-universal.zip",
      "download_count": 123456
    }
  ]
}"#;

        let text_content = TextContent::new(api_response.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "curl-api".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let response_result = tool_response_to_markdown(&tool_response, true);

        // Should detect and format as JSON
        assert!(response_result.contains("```json"));
        assert!(response_result.contains("\"tag_name\": \"1.85.0\""));
        assert!(response_result.contains("\"download_count\": 123456"));
    }

    #[test]
    fn test_write_tool_with_code_creation() {
        let editor_call = CallToolRequestParams::new("write")
                    .with_arguments(object!({
                "path": "/tmp/fibonacci.js",
                "content": "function fibonacci(n) {\n  if (n <= 1) return n;\n  return fibonacci(n - 1) + fibonacci(n - 2);\n}\n\nconsole.log(fibonacci(10));"
            }));
        let tool_request = ToolRequest {
            id: "editor-write".to_string(),
            tool_call: Ok(editor_call),
            metadata: None,
            tool_meta: None,
        };

        let text_content = TextContent::new("File created successfully".to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "editor-write".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let request_result = tool_request_to_markdown(&tool_request, true);
        let response_result = tool_response_to_markdown(&tool_response, true);

        // Check request formatting - should format code in content properly
        assert!(request_result.contains("#### Tool Call: `write`"));
        assert!(request_result.contains("**path**: `/tmp/fibonacci.js`"));
        assert!(request_result.contains("**content**:"));
        assert!(request_result.contains("function fibonacci(n)"));
        assert!(request_result.contains("return fibonacci(n - 1)"));

        // Check response formatting
        assert!(response_result.contains("File created successfully"));
    }

    #[test]
    fn test_shell_tool_with_error_output() {
        let error_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "python nonexistent_script.py"
        }));
        let _tool_request = ToolRequest {
            id: "shell-error".to_string(),
            tool_call: Ok(error_call),
            metadata: None,
            tool_meta: None,
        };

        let error_output = r#"python: can't open file 'nonexistent_script.py': [Errno 2] No such file or directory
Command failed with exit code 2"#;

        let text_content = TextContent::new(error_output.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "shell-error".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let response_result = tool_response_to_markdown(&tool_response, true);

        // Error output should be formatted as plain text
        assert!(response_result.contains("can't open file"));
        assert!(response_result.contains("Command failed with exit code 2"));
    }

    #[test]
    fn test_shell_tool_complex_script_execution() {
        let script_call = CallToolRequestParams::new("shell")
                    .with_arguments(object!({
                "command": "python -c \"import sys; print(f'Python {sys.version}'); [print(f'{i}^2 = {i**2}') for i in range(1, 6)]\""
            }));
        let tool_request = ToolRequest {
            id: "script-exec".to_string(),
            tool_call: Ok(script_call),
            metadata: None,
            tool_meta: None,
        };

        let script_output = r#"Python 3.11.5 (main, Aug 24 2023, 15:18:16) [Clang 14.0.3 ]
1^2 = 1
2^2 = 4
3^2 = 9
4^2 = 16
5^2 = 25"#;

        let text_content = TextContent::new(script_output.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "script-exec".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let request_result = tool_request_to_markdown(&tool_request, true);
        let response_result = tool_response_to_markdown(&tool_response, true);

        // Check request formatting for complex command
        assert!(request_result.contains("```sh"));
        assert!(request_result.contains("python -c"));
        assert!(request_result.contains("sys.version"));

        // Check response formatting
        assert!(response_result.contains("Python 3.11.5"));
        assert!(response_result.contains("1^2 = 1"));
        assert!(response_result.contains("5^2 = 25"));
    }

    #[test]
    fn test_shell_tool_with_multi_command() {
        let multi_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "cd /tmp && ls -la | head -5 && pwd"
        }));
        let _tool_request = ToolRequest {
            id: "multi-cmd".to_string(),
            tool_call: Ok(multi_call),
            metadata: None,
            tool_meta: None,
        };

        let multi_output = r#"total 24
drwxrwxrwt  15 root  wheel   480 Dec  7 10:30 .
drwxr-xr-x   6 root  wheel   192 Nov 15 09:15 ..
-rw-r--r--   1 user  staff   256 Dec  7 09:45 config.json
drwx------   3 user  staff    96 Dec  6 16:20 com.apple.launchd.abc
/tmp"#;

        let text_content = TextContent::new(multi_output.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "multi-cmd".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let request_result = tool_request_to_markdown(&_tool_request, true);
        let response_result = tool_response_to_markdown(&tool_response, true);

        // Check request formatting for chained commands
        assert!(request_result.contains("cd /tmp && ls -la | head -5 && pwd"));

        // Check response formatting
        assert!(response_result.contains("drwxrwxrwt"));
        assert!(response_result.contains("config.json"));
        assert!(response_result.contains("/tmp"));
    }

    #[test]
    fn test_developer_tool_grep_code_search() {
        let grep_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "rg 'async fn' --type rust -n"
        }));
        let tool_request = ToolRequest {
            id: "grep-search".to_string(),
            tool_call: Ok(grep_call),
            metadata: None,
            tool_meta: None,
        };

        let grep_output = r#"src/main.rs:15:async fn process_request(req: Request) -> Result<Response> {
src/handler.rs:8:async fn handle_connection(stream: TcpStream) {
src/database.rs:23:async fn query_users(pool: &Pool) -> Result<Vec<User>> {
src/middleware.rs:12:async fn auth_middleware(req: Request, next: Next) -> Result<Response> {"#;

        let text_content = TextContent::new(grep_output.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "grep-search".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let request_result = tool_request_to_markdown(&tool_request, true);
        let response_result = tool_response_to_markdown(&tool_response, true);

        // Check request formatting
        assert!(request_result.contains("rg 'async fn' --type rust -n"));

        // Check response formatting - should be formatted as search results
        assert!(response_result.contains("src/main.rs:15:"));
        assert!(response_result.contains("async fn process_request"));
        assert!(response_result.contains("src/database.rs:23:"));
    }

    #[test]
    fn test_shell_tool_json_detection_works() {
        // This test shows that JSON detection in tool responses DOES work
        let tool_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "echo '{\"test\": \"json\"}'"
        }));
        let _tool_request = ToolRequest {
            id: "json-test".to_string(),
            tool_call: Ok(tool_call),
            metadata: None,
            tool_meta: None,
        };

        let json_output = r#"{"status": "success", "data": {"count": 42}}"#;
        let text_content = TextContent::new(json_output.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "json-test".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let response_result = tool_response_to_markdown(&tool_response, true);

        // JSON should be auto-detected and formatted
        assert!(response_result.contains("```json"));
        assert!(response_result.contains("\"status\": \"success\""));
        assert!(response_result.contains("\"count\": 42"));
    }

    #[test]
    fn test_shell_tool_with_package_management() {
        let npm_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "npm install express typescript @types/node --save-dev"
        }));
        let tool_request = ToolRequest {
            id: "npm-install".to_string(),
            tool_call: Ok(npm_call),
            metadata: None,
            tool_meta: None,
        };

        let npm_output = r#"added 57 packages, and audited 58 packages in 3s

8 packages are looking for funding
  run `npm fund` for details

found 0 vulnerabilities"#;

        let text_content = TextContent::new(npm_output.to_string());
        let tool_response = ToolResponse {
            metadata: None,
            id: "npm-install".to_string(),
            tool_result: Ok(rmcp::model::CallToolResult::success(vec![
                ContentBlock::text(text_content.text),
            ])),
        };

        let request_result = tool_request_to_markdown(&tool_request, true);
        let response_result = tool_response_to_markdown(&tool_response, true);

        // Check request formatting
        assert!(request_result.contains("npm install express typescript"));
        assert!(request_result.contains("--save-dev"));

        // Check response formatting
        assert!(response_result.contains("added 57 packages"));
        assert!(response_result.contains("found 0 vulnerabilities"));
    }

    #[test]
    fn markdown_export_preserves_user_audience_tool_output() {
        let user_output = ContentBlock::Text(
            TextContent::new("user-visible output")
                .with_annotations(Annotations::default().with_audience(vec![Role::User])),
        );
        let assistant_output = ContentBlock::Text(
            TextContent::new("assistant-only output")
                .with_annotations(Annotations::default().with_audience(vec![Role::Assistant])),
        );
        let conversation = Conversation::new_unvalidated([Message::user().with_tool_response(
            "tool-1",
            Ok(rmcp::model::CallToolResult::success(vec![
                user_output,
                assistant_output,
                ContentBlock::text("shared output"),
            ])),
        )]);

        let markdown =
            export_session_to_markdown(conversation.user_visible_messages(), "Audience export");

        assert!(markdown.contains("user-visible output"));
        assert!(markdown.contains("shared output"));
        assert!(!markdown.contains("assistant-only output"));
    }

    #[test]
    fn markdown_export_handles_empty_conversation() {
        let markdown = export_session_to_markdown(Vec::new(), "Empty session");

        assert!(markdown.contains("# Session Export: Empty session"));
        assert!(markdown.contains("*(This session has no messages)*"));
    }
}
