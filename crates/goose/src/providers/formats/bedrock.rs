use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use crate::mcp_utils::ToolResult;
use anyhow::{anyhow, bail, Result};
use aws_sdk_bedrockruntime::types as bedrock;
use aws_smithy_types::{Document, Number};
use base64::Engine;
use chrono::Utc;
use rmcp::model::{
    object, CallToolRequestParams, Content, ErrorCode, ErrorData, RawContent, ResourceContents,
    Role, Tool,
};
use serde_json::Value;

use super::super::base::Usage;
use crate::conversation::message::{Message, MessageContent};

pub fn to_bedrock_message_with_caching(
    message: &Message,
    enable_caching: bool,
) -> Result<bedrock::Message> {
    let mut content_blocks: Vec<bedrock::ContentBlock> = message
        .content
        .iter()
        .map(to_bedrock_message_content)
        .collect::<Result<_>>()?;

    if enable_caching && !content_blocks.is_empty() {
        content_blocks.push(bedrock::ContentBlock::CachePoint(
            bedrock::CachePointBlock::builder()
                .r#type(bedrock::CachePointType::Default)
                .build()
                .map_err(|e| anyhow!("Failed to build cache point for message: {}", e))?,
        ));
    }

    bedrock::Message::builder()
        .role(to_bedrock_role(&message.role))
        .set_content(Some(content_blocks))
        .build()
        .map_err(|err| anyhow!("Failed to construct Bedrock message: {}", err))
}

pub fn to_bedrock_message_content(content: &MessageContent) -> Result<bedrock::ContentBlock> {
    Ok(match content {
        MessageContent::Text(text) => bedrock::ContentBlock::Text(text.text.to_string()),
        MessageContent::ToolConfirmationRequest(_tool_confirmation_request) => {
            bedrock::ContentBlock::Text("".to_string())
        }
        MessageContent::ActionRequired(_action_required) => {
            bedrock::ContentBlock::Text("".to_string())
        }
        MessageContent::Image(image) => {
            bedrock::ContentBlock::Image(to_bedrock_image(&image.data, &image.mime_type)?)
        }
        MessageContent::Thinking(_) => {
            // Thinking blocks are not supported in Bedrock - skip
            bedrock::ContentBlock::Text("".to_string())
        }
        MessageContent::RedactedThinking(_) => {
            // Redacted thinking blocks are not supported in Bedrock - skip
            bedrock::ContentBlock::Text("".to_string())
        }
        MessageContent::SystemNotification(_) => {
            bail!("SystemNotification should not get passed to the provider")
        }
        MessageContent::ToolRequest(tool_req) => {
            let tool_use_id = tool_req.id.to_string();
            let tool_use = if let Ok(call) = tool_req.tool_call.as_ref() {
                bedrock::ToolUseBlock::builder()
                    .tool_use_id(tool_use_id)
                    .name(call.name.to_string())
                    .input(to_bedrock_json(&args_to_value(call.arguments.clone())))
                    .build()
            } else {
                bedrock::ToolUseBlock::builder()
                    .tool_use_id(tool_use_id)
                    .build()
            }?;
            bedrock::ContentBlock::ToolUse(tool_use)
        }
        MessageContent::FrontendToolRequest(tool_req) => {
            let tool_use_id = tool_req.id.to_string();
            let tool_use = if let Ok(call) = tool_req.tool_call.as_ref() {
                bedrock::ToolUseBlock::builder()
                    .tool_use_id(tool_use_id)
                    .name(call.name.to_string())
                    .input(to_bedrock_json(&args_to_value(call.arguments.clone())))
                    .build()
            } else {
                bedrock::ToolUseBlock::builder()
                    .tool_use_id(tool_use_id)
                    .build()
            }?;
            bedrock::ContentBlock::ToolUse(tool_use)
        }
        MessageContent::ToolResponse(tool_res) => {
            let content = match &tool_res.tool_result {
                Ok(result) => Some(
                    result
                        .content
                        .iter()
                        .map(|c| to_bedrock_tool_result_content_block(&tool_res.id, c.clone()))
                        .collect::<Result<_>>()?,
                ),
                Err(error) => {
                    // For errors, create a text content block with the error message
                    Some(vec![bedrock::ToolResultContentBlock::Text(format!(
                        "The tool call returned the following error:\n{}",
                        error
                    ))])
                }
            };
            bedrock::ContentBlock::ToolResult(
                bedrock::ToolResultBlock::builder()
                    .tool_use_id(tool_res.id.to_string())
                    .status(if tool_res.tool_result.is_ok() {
                        bedrock::ToolResultStatus::Success
                    } else {
                        bedrock::ToolResultStatus::Error
                    })
                    .set_content(content)
                    .build()?,
            )
        }
    })
}

/// Convert MCP Content to Bedrock ToolResultContentBlock
///
/// Supports text, images, and document resources. Images are supported
/// by Bedrock for Anthropic Claude 3 models.
pub fn to_bedrock_tool_result_content_block(
    tool_use_id: &str,
    content: Content,
) -> Result<bedrock::ToolResultContentBlock> {
    Ok(match content.raw {
        RawContent::Text(text) => bedrock::ToolResultContentBlock::Text(text.text),
        RawContent::Image(image) => {
            bedrock::ToolResultContentBlock::Image(to_bedrock_image(&image.data, &image.mime_type)?)
        }
        RawContent::ResourceLink(_link) => {
            bedrock::ToolResultContentBlock::Text("[Resource link]".to_string())
        }
        RawContent::Resource(resource) => match &resource.resource {
            ResourceContents::TextResourceContents { text, .. } => {
                match to_bedrock_document(tool_use_id, &resource.resource)? {
                    Some(doc) => bedrock::ToolResultContentBlock::Document(doc),
                    None => bedrock::ToolResultContentBlock::Text(text.to_string()),
                }
            }
            ResourceContents::BlobResourceContents { .. } => {
                bail!("Blob resource content is not supported by Bedrock provider yet")
            }
        },
        RawContent::Audio(..) => bail!("Audio is not supported by Bedrock provider"),
    })
}

pub fn to_bedrock_role(role: &Role) -> bedrock::ConversationRole {
    match role {
        Role::User => bedrock::ConversationRole::User,
        Role::Assistant => bedrock::ConversationRole::Assistant,
    }
}

pub fn to_bedrock_image(data: &str, mime_type: &str) -> Result<bedrock::ImageBlock> {
    // Extract format from MIME type
    let format = match mime_type {
        "image/png" => bedrock::ImageFormat::Png,
        "image/jpeg" | "image/jpg" => bedrock::ImageFormat::Jpeg,
        "image/gif" => bedrock::ImageFormat::Gif,
        "image/webp" => bedrock::ImageFormat::Webp,
        _ => bail!(
            "Unsupported image format: {}. Bedrock supports png, jpeg, gif, webp",
            mime_type
        ),
    };

    // Create image source with base64 data
    let source = bedrock::ImageSource::Bytes(aws_smithy_types::Blob::new(
        base64::prelude::BASE64_STANDARD
            .decode(data)
            .map_err(|e| anyhow!("Failed to decode base64 image data: {}", e))?,
    ));

    // Build the image block
    Ok(bedrock::ImageBlock::builder()
        .format(format)
        .source(source)
        .build()?)
}

pub fn to_bedrock_tool_config(tools: &[Tool]) -> Result<bedrock::ToolConfiguration> {
    Ok(bedrock::ToolConfiguration::builder()
        .set_tools(Some(
            tools.iter().map(to_bedrock_tool).collect::<Result<_>>()?,
        ))
        .build()?)
}

pub fn to_bedrock_tool(tool: &Tool) -> Result<bedrock::Tool> {
    let mut input_schema = tool.input_schema.as_ref().clone();

    // If the schema doesn't have a "type" field, add it
    // This is required by Bedrock
    if !input_schema.contains_key("type") {
        input_schema.insert("type".to_string(), Value::String("object".to_string()));
    }

    Ok(bedrock::Tool::ToolSpec(
        bedrock::ToolSpecification::builder()
            .name(tool.name.to_string())
            .description(
                tool.description
                    .as_ref()
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            )
            .input_schema(bedrock::ToolInputSchema::Json(to_bedrock_json(
                &Value::Object(input_schema),
            )))
            .build()?,
    ))
}

fn args_to_value(args: Option<serde_json::Map<String, Value>>) -> Value {
    match args {
        Some(map) => Value::Object(map),
        None => Value::Object(serde_json::Map::new()),
    }
}

pub fn to_bedrock_json(value: &Value) -> Document {
    match value {
        Value::Null => Document::Null,
        Value::Bool(bool) => Document::Bool(*bool),
        Value::Number(num) => {
            if let Some(n) = num.as_u64() {
                Document::Number(Number::PosInt(n))
            } else if let Some(n) = num.as_i64() {
                Document::Number(Number::NegInt(n))
            } else if let Some(n) = num.as_f64() {
                Document::Number(Number::Float(n))
            } else {
                unreachable!()
            }
        }
        Value::String(str) => Document::String(str.to_string()),
        Value::Array(arr) => Document::Array(arr.iter().map(to_bedrock_json).collect()),
        Value::Object(obj) => Document::Object(HashMap::from_iter(
            obj.into_iter()
                .map(|(key, val)| (key.to_string(), to_bedrock_json(val))),
        )),
    }
}

fn to_bedrock_document(
    tool_use_id: &str,
    content: &ResourceContents,
) -> Result<Option<bedrock::DocumentBlock>> {
    let (uri, text) = match content {
        ResourceContents::TextResourceContents { uri, text, .. } => (uri, text),
        ResourceContents::BlobResourceContents { .. } => {
            bail!("Blob resource content is not supported by Bedrock provider yet")
        }
    };

    let filename = Path::new(uri)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(uri);

    // Return None if the file type is not supported
    let (name, format) = match filename.split_once('.') {
        Some((name, "txt")) => (name, bedrock::DocumentFormat::Txt),
        Some((name, "csv")) => (name, bedrock::DocumentFormat::Csv),
        Some((name, "md")) => (name, bedrock::DocumentFormat::Md),
        _ => return Ok(None), // Not a supported document type
    };

    // Since we can't use the full path (due to character limit and also Bedrock does not accept `/` etc.),
    // and Bedrock wants document names to be unique, we're adding `tool_use_id` as a prefix to make
    // document names unique
    let name = format!("{tool_use_id}-{name}");

    Ok(Some(
        bedrock::DocumentBlock::builder()
            .format(format)
            .name(name)
            .source(bedrock::DocumentSource::Bytes(text.as_bytes().into()))
            .build()
            .map_err(|err| anyhow!("Failed to construct Bedrock document: {}", err))?,
    ))
}

pub fn from_bedrock_message(message: &bedrock::Message) -> Result<Message> {
    let role = from_bedrock_role(message.role())?;
    let content = message
        .content()
        .iter()
        .filter(|block| !matches!(block, bedrock::ContentBlock::CachePoint(_)))
        .map(from_bedrock_content_block)
        .collect::<Result<Vec<_>>>()?;
    let created = Utc::now().timestamp();

    Ok(Message::new(role, created, content))
}

pub fn from_bedrock_content_block(block: &bedrock::ContentBlock) -> Result<MessageContent> {
    Ok(match block {
        bedrock::ContentBlock::Text(text) => MessageContent::text(text),
        bedrock::ContentBlock::ToolUse(tool_use) => MessageContent::tool_request(
            tool_use.tool_use_id.to_string(),
            Ok(CallToolRequestParams::new(tool_use.name.clone())
                .with_arguments(object(from_bedrock_json(&tool_use.input.clone())?))),
        ),
        bedrock::ContentBlock::ToolResult(tool_res) => MessageContent::tool_response(
            tool_res.tool_use_id.to_string(),
            if tool_res.content.is_empty() {
                Err(ErrorData {
                    code: ErrorCode::INTERNAL_ERROR,
                    message: Cow::from("Empty content for tool use from Bedrock".to_string()),
                    data: None,
                })
            } else {
                tool_res
                    .content
                    .iter()
                    .map(from_bedrock_tool_result_content_block)
                    .collect::<ToolResult<Vec<_>>>()
                    .map(rmcp::model::CallToolResult::success)
            },
        ),
        bedrock::ContentBlock::CachePoint(_) => {
            // Filtered upstream in from_bedrock_message
            bail!("CachePoint blocks should have been filtered out during message processing")
        }
        _ => bail!("Unsupported content block type from Bedrock"),
    })
}

pub fn from_bedrock_tool_result_content_block(
    content: &bedrock::ToolResultContentBlock,
) -> ToolResult<Content> {
    Ok(match content {
        bedrock::ToolResultContentBlock::Text(text) => Content::text(text.to_string()),
        _ => {
            return Err(ErrorData {
                code: ErrorCode::INTERNAL_ERROR,
                message: Cow::from("Unsupported tool result from Bedrock".to_string()),
                data: None,
            })
        }
    })
}

pub fn from_bedrock_role(role: &bedrock::ConversationRole) -> Result<Role> {
    Ok(match role {
        bedrock::ConversationRole::User => Role::User,
        bedrock::ConversationRole::Assistant => Role::Assistant,
        _ => bail!("Unknown role from Bedrock"),
    })
}

pub fn from_bedrock_usage(usage: &bedrock::TokenUsage) -> Usage {
    Usage::new(
        Some(usage.input_tokens),
        Some(usage.output_tokens),
        Some(usage.total_tokens),
    )
}

pub fn from_bedrock_json(document: &Document) -> Result<Value> {
    Ok(match document {
        Document::Null => Value::Null,
        Document::Bool(bool) => Value::Bool(*bool),
        Document::Number(num) => match num {
            Number::PosInt(i) => Value::Number((*i).into()),
            Number::NegInt(i) => Value::Number((*i).into()),
            Number::Float(f) => Value::Number(
                serde_json::Number::from_f64(*f).ok_or(anyhow!("Expected a valid float"))?,
            ),
        },
        Document::String(str) => Value::String(str.clone()),
        Document::Array(arr) => {
            Value::Array(arr.iter().map(from_bedrock_json).collect::<Result<_>>()?)
        }
        Document::Object(obj) => Value::Object(
            obj.iter()
                .map(|(key, val)| Ok((key.clone(), from_bedrock_json(val)?)))
                .collect::<Result<_>>()?,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use goose_test_support::TEST_IMAGE_B64;
    use rmcp::model::{AnnotateAble, RawImageContent};

    #[test]
    fn test_to_bedrock_image_supported_formats() -> Result<()> {
        let supported_formats = [
            "image/png",
            "image/jpeg",
            "image/jpg",
            "image/gif",
            "image/webp",
        ];

        for mime_type in supported_formats {
            let image = RawImageContent {
                data: TEST_IMAGE_B64.to_string(),
                mime_type: mime_type.to_string(),
                meta: None,
            }
            .no_annotation();

            let result = to_bedrock_image(&image.data, &image.mime_type);
            assert!(result.is_ok(), "Failed to convert {} format", mime_type);
        }

        Ok(())
    }

    #[test]
    fn test_to_bedrock_image_unsupported_format() {
        let image = RawImageContent {
            data: TEST_IMAGE_B64.to_string(),
            mime_type: "image/bmp".to_string(),
            meta: None,
        }
        .no_annotation();

        let result = to_bedrock_image(&image.data, &image.mime_type);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unsupported image format: image/bmp"));
        assert!(error_msg.contains("Bedrock supports png, jpeg, gif, webp"));
    }

    #[test]
    fn test_to_bedrock_image_invalid_base64() {
        let image = RawImageContent {
            data: "invalid_base64_data!!!".to_string(),
            mime_type: "image/png".to_string(),
            meta: None,
        }
        .no_annotation();

        let result = to_bedrock_image(&image.data, &image.mime_type);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to decode base64 image data"));
    }

    #[test]
    fn test_to_bedrock_message_content_image() -> Result<()> {
        let image = RawImageContent {
            data: TEST_IMAGE_B64.to_string(),
            mime_type: "image/png".to_string(),
            meta: None,
        }
        .no_annotation();

        let message_content = MessageContent::Image(image);
        let result = to_bedrock_message_content(&message_content)?;

        // Verify we get an Image content block
        assert!(matches!(result, bedrock::ContentBlock::Image(_)));

        Ok(())
    }

    #[test]
    fn test_to_bedrock_tool_result_content_block_image() -> Result<()> {
        let content = Content::image(TEST_IMAGE_B64.to_string(), "image/png".to_string());
        let result = to_bedrock_tool_result_content_block("test_id", content)?;

        // Verify the wrapper correctly converts Content::Image to ToolResultContentBlock::Image
        assert!(matches!(result, bedrock::ToolResultContentBlock::Image(_)));

        Ok(())
    }

    #[test]
    fn test_to_bedrock_message_with_caching() -> Result<()> {
        use chrono::Utc;
        use rmcp::model::Role;

        // Multiple content blocks: cache point appended at end, order preserved
        let message = Message::new(
            Role::User,
            Utc::now().timestamp(),
            vec![
                MessageContent::text("First text"),
                MessageContent::text("Second text"),
            ],
        );
        let bedrock_message = to_bedrock_message_with_caching(&message, true)?;
        assert_eq!(bedrock_message.content.len(), 3);
        if let bedrock::ContentBlock::Text(text) = &bedrock_message.content[0] {
            assert_eq!(text, "First text");
        } else {
            panic!("Expected text content block");
        }
        if let bedrock::ContentBlock::Text(text) = &bedrock_message.content[1] {
            assert_eq!(text, "Second text");
        } else {
            panic!("Expected text content block");
        }
        assert!(matches!(
            bedrock_message.content[2],
            bedrock::ContentBlock::CachePoint(_)
        ));

        // Caching disabled: no cache point added
        let no_cache = to_bedrock_message_with_caching(&message, false)?;
        assert_eq!(no_cache.content.len(), 2);
        for block in &no_cache.content {
            assert!(!matches!(block, bedrock::ContentBlock::CachePoint(_)));
        }

        // Empty content: no cache point added even with caching enabled
        let empty = Message::new(Role::User, Utc::now().timestamp(), vec![]);
        let empty_msg = to_bedrock_message_with_caching(&empty, true)?;
        assert_eq!(empty_msg.content.len(), 0);

        Ok(())
    }

    #[test]
    fn test_from_bedrock_content_block_cache_point() {
        // Create a cache point block with the required type field
        let cache_point = bedrock::CachePointBlock::builder()
            .r#type(bedrock::CachePointType::Default)
            .build()
            .unwrap();
        let content_block = bedrock::ContentBlock::CachePoint(cache_point);

        // Verify that converting a cache point results in an error
        let result = from_bedrock_content_block(&content_block);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("CachePoint blocks should have been filtered out"));
    }

    #[test]
    fn test_from_bedrock_message_filters_cache_points() -> Result<()> {
        use rmcp::model::Role;

        // Create a Bedrock message with mixed content including CachePoint
        let cache_point = bedrock::CachePointBlock::builder()
            .r#type(bedrock::CachePointType::Default)
            .build()
            .unwrap();

        let bedrock_message = bedrock::Message::builder()
            .role(bedrock::ConversationRole::Assistant)
            .content(bedrock::ContentBlock::Text("First text".to_string()))
            .content(bedrock::ContentBlock::CachePoint(cache_point))
            .content(bedrock::ContentBlock::Text("Second text".to_string()))
            .build()
            .unwrap();

        // Convert from Bedrock format
        let message = from_bedrock_message(&bedrock_message)?;

        // Verify that CachePoint was filtered out and only text content remains
        assert_eq!(message.content.len(), 2);
        assert_eq!(message.role, Role::Assistant);

        if let MessageContent::Text(text) = &message.content[0] {
            assert_eq!(text.text, "First text");
        } else {
            panic!("Expected first text content");
        }

        if let MessageContent::Text(text) = &message.content[1] {
            assert_eq!(text.text, "Second text");
        } else {
            panic!("Expected second text content");
        }

        Ok(())
    }

    #[test]
    fn test_cache_points_with_tool_request_messages() -> Result<()> {
        use chrono::Utc;
        use rmcp::model::{CallToolRequestParams, Role};
        use serde_json::json;

        let message = Message::new(
            Role::Assistant,
            Utc::now().timestamp(),
            vec![
                MessageContent::text("I'll use a tool"),
                MessageContent::tool_request(
                    "tool_1".to_string(),
                    Ok(CallToolRequestParams::new("test_tool")
                        .with_arguments(object(json!({"param": "value"})))),
                ),
            ],
        );

        let bedrock_message = to_bedrock_message_with_caching(&message, true)?;

        // Verify cache point is added after all content blocks (text + tool request + cache point)
        assert_eq!(bedrock_message.content.len(), 3);
        assert!(matches!(
            bedrock_message.content[0],
            bedrock::ContentBlock::Text(_)
        ));
        assert!(matches!(
            bedrock_message.content[1],
            bedrock::ContentBlock::ToolUse(_)
        ));
        assert!(matches!(
            bedrock_message.content[2],
            bedrock::ContentBlock::CachePoint(_)
        ));

        Ok(())
    }

    #[test]
    fn test_cache_points_with_tool_response_messages() -> Result<()> {
        use chrono::Utc;
        use rmcp::model::{CallToolResult, Role};

        let message = Message::new(
            Role::User,
            Utc::now().timestamp(),
            vec![MessageContent::tool_response(
                "tool_1".to_string(),
                Ok(CallToolResult::success(vec![Content::text(
                    "Tool result text".to_string(),
                )])),
            )],
        );

        let bedrock_message = to_bedrock_message_with_caching(&message, true)?;

        // Verify cache point is added after tool response content
        assert_eq!(bedrock_message.content.len(), 2);
        assert!(matches!(
            bedrock_message.content[0],
            bedrock::ContentBlock::ToolResult(_)
        ));
        assert!(matches!(
            bedrock_message.content[1],
            bedrock::ContentBlock::CachePoint(_)
        ));

        Ok(())
    }

    #[test]
    fn test_cache_points_with_mixed_tool_content() -> Result<()> {
        use chrono::Utc;
        use rmcp::model::{CallToolRequestParams, Role};
        use serde_json::json;

        let message = Message::new(
            Role::Assistant,
            Utc::now().timestamp(),
            vec![
                MessageContent::text("Using tools"),
                MessageContent::tool_request(
                    "tool_1".to_string(),
                    Ok(CallToolRequestParams::new("tool_a")
                        .with_arguments(object(json!({"key": "val"})))),
                ),
                MessageContent::tool_request(
                    "tool_2".to_string(),
                    Ok(CallToolRequestParams::new("tool_b")
                        .with_arguments(object(json!({"key": "val"})))),
                ),
            ],
        );

        let bedrock_message = to_bedrock_message_with_caching(&message, true)?;

        // Verify cache point is added at the end after all tool requests
        assert_eq!(bedrock_message.content.len(), 4);
        assert!(matches!(
            bedrock_message.content[0],
            bedrock::ContentBlock::Text(_)
        ));
        assert!(matches!(
            bedrock_message.content[1],
            bedrock::ContentBlock::ToolUse(_)
        ));
        assert!(matches!(
            bedrock_message.content[2],
            bedrock::ContentBlock::ToolUse(_)
        ));
        assert!(matches!(
            bedrock_message.content[3],
            bedrock::ContentBlock::CachePoint(_)
        ));

        Ok(())
    }
}
