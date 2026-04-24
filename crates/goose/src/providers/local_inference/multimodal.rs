use base64::prelude::*;
use serde_json::Value;

use crate::conversation::message::{Message, MessageContent};
use crate::providers::errors::ProviderError;

#[derive(Debug)]
pub struct ExtractedImage {
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MultimodalMessages {
    pub messages_json: String,
    pub images: Vec<ExtractedImage>,
}

/// Walk the OpenAI-format messages JSON array. For each content part with
/// `type: "image_url"`, decode the base64 data URL, store the raw bytes,
/// and replace the part with `{"type": "text", "text": "<marker>"}`.
///
/// Returns the modified JSON string and the extracted images in order.
#[allow(dead_code)]
pub fn extract_images_from_messages_json(
    messages_json: &str,
    marker: &str,
) -> Result<MultimodalMessages, ProviderError> {
    let mut messages: Vec<Value> = serde_json::from_str(messages_json).map_err(|e| {
        ProviderError::ExecutionError(format!("Failed to parse messages JSON: {e}"))
    })?;

    let mut images = Vec::new();

    for msg in messages.iter_mut() {
        let Some(content) = msg.get_mut("content").and_then(|c| c.as_array_mut()) else {
            continue;
        };

        for part in content.iter_mut() {
            if part.get("type").and_then(|t| t.as_str()) != Some("image_url") {
                continue;
            }

            let url = part
                .get("image_url")
                .and_then(|obj| obj.get("url"))
                .and_then(|u| u.as_str())
                .unwrap_or_default();

            if url.starts_with("http://") || url.starts_with("https://") {
                return Err(ProviderError::ExecutionError(
                    "Remote image URLs are not supported with local inference. \
                     Please attach the image directly."
                        .to_string(),
                ));
            }

            let base64_data = url.split_once(',').map_or(url, |(_, data)| data);

            let bytes = BASE64_STANDARD.decode(base64_data).map_err(|e| {
                ProviderError::ExecutionError(format!("Failed to decode base64 image: {e}"))
            })?;

            images.push(ExtractedImage { bytes });

            *part = serde_json::json!({
                "type": "text",
                "text": marker,
            });
        }
    }

    let messages_json = serde_json::to_string(&messages)
        .map_err(|e| ProviderError::ExecutionError(format!("Failed to serialize messages: {e}")))?;

    Ok(MultimodalMessages {
        messages_json,
        images,
    })
}

/// Scan messages for `MessageContent::Image` entries. Return the extracted image
/// bytes and a new message list with images replaced by text marker placeholders.
pub fn extract_images_from_messages(
    messages: &[Message],
    marker: &str,
) -> (Vec<ExtractedImage>, Vec<Message>) {
    let mut images = Vec::new();
    let mut new_messages = Vec::with_capacity(messages.len());

    for msg in messages {
        let mut new_content = Vec::with_capacity(msg.content.len());
        for content in &msg.content {
            match content {
                MessageContent::Image(img) => {
                    if let Ok(bytes) = BASE64_STANDARD.decode(&img.data) {
                        images.push(ExtractedImage { bytes });
                        new_content.push(MessageContent::text(marker));
                    } else {
                        new_content.push(MessageContent::text(
                            "[Image attached — failed to decode image data]",
                        ));
                    }
                }
                other => new_content.push(other.clone()),
            }
        }
        new_messages.push(Message {
            role: msg.role.clone(),
            content: new_content,
            ..msg.clone()
        });
    }

    (images, new_messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_test_messages_json(parts: Vec<Value>) -> String {
        serde_json::to_string(&vec![json!({
            "role": "user",
            "content": parts,
        })])
        .unwrap()
    }

    fn tiny_png_base64() -> String {
        // 1x1 red PNG
        let bytes: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x36, 0x28, 0x19,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        BASE64_STANDARD.encode(bytes)
    }

    #[test]
    fn test_extract_images_replaces_image_url_with_marker() {
        let b64 = tiny_png_base64();
        let json = make_test_messages_json(vec![json!({
            "type": "image_url",
            "image_url": {"url": format!("data:image/png;base64,{b64}")}
        })]);

        let result = extract_images_from_messages_json(&json, "<__media__>").unwrap();
        assert_eq!(result.images.len(), 1);
        assert!(!result.images[0].bytes.is_empty());

        let parsed: Vec<Value> = serde_json::from_str(&result.messages_json).unwrap();
        let content = parsed[0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "<__media__>");
    }

    #[test]
    fn test_extract_images_preserves_text_parts() {
        let json = make_test_messages_json(vec![json!({
            "type": "text",
            "text": "Hello world"
        })]);

        let result = extract_images_from_messages_json(&json, "<__media__>").unwrap();
        assert!(result.images.is_empty());

        let parsed: Vec<Value> = serde_json::from_str(&result.messages_json).unwrap();
        let content = parsed[0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Hello world");
    }

    #[test]
    fn test_extract_images_multiple_images() {
        let b64 = tiny_png_base64();
        let json = make_test_messages_json(vec![
            json!({"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{b64}")}}),
            json!({"type": "text", "text": "describe both"}),
            json!({"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{b64}")}}),
        ]);

        let result = extract_images_from_messages_json(&json, "<__media__>").unwrap();
        assert_eq!(result.images.len(), 2);

        let parsed: Vec<Value> = serde_json::from_str(&result.messages_json).unwrap();
        let content = parsed[0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "<__media__>");
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], "describe both");
        assert_eq!(content[2]["type"], "text");
        assert_eq!(content[2]["text"], "<__media__>");
    }

    #[test]
    fn test_extract_images_no_images() {
        let json = make_test_messages_json(vec![json!({
            "type": "text",
            "text": "just text"
        })]);

        let result = extract_images_from_messages_json(&json, "<__media__>").unwrap();
        assert!(result.images.is_empty());
        // JSON should be equivalent
        let original: Vec<Value> = serde_json::from_str(&json).unwrap();
        let result_parsed: Vec<Value> = serde_json::from_str(&result.messages_json).unwrap();
        assert_eq!(original, result_parsed);
    }

    #[test]
    fn test_extract_images_http_url_rejected() {
        let json = make_test_messages_json(vec![json!({
            "type": "image_url",
            "image_url": {"url": "https://example.com/image.png"}
        })]);

        let result = extract_images_from_messages_json(&json, "<__media__>");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Remote image URLs are not supported"));
    }

    #[test]
    fn test_extract_images_mixed_content() {
        let b64 = tiny_png_base64();
        // Two messages: first with text+image, second with just text
        let json = serde_json::to_string(&vec![
            json!({
                "role": "user",
                "content": [
                    {"type": "text", "text": "What is this?"},
                    {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{b64}")}},
                ]
            }),
            json!({
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "It looks like a red pixel."},
                ]
            }),
        ])
        .unwrap();

        let result = extract_images_from_messages_json(&json, "<__media__>").unwrap();
        assert_eq!(result.images.len(), 1);

        let parsed: Vec<Value> = serde_json::from_str(&result.messages_json).unwrap();
        // First message: text preserved, image replaced
        let content0 = parsed[0]["content"].as_array().unwrap();
        assert_eq!(content0[0]["text"], "What is this?");
        assert_eq!(content0[1]["text"], "<__media__>");
        // Second message unchanged
        let content1 = parsed[1]["content"].as_array().unwrap();
        assert_eq!(content1[0]["text"], "It looks like a red pixel.");
    }

    // --- Tests for extract_images_from_messages (Message-based) ---

    #[test]
    fn test_messages_extract_replaces_image_with_marker() {
        let b64 = tiny_png_base64();
        let messages = vec![Message::user().with_image(b64, "image/png")];

        let (images, new_msgs) = extract_images_from_messages(&messages, "<__media__>");
        assert_eq!(images.len(), 1);
        assert!(!images[0].bytes.is_empty());
        assert_eq!(new_msgs.len(), 1);
        assert_eq!(new_msgs[0].as_concat_text(), "<__media__>");
    }

    #[test]
    fn test_messages_extract_preserves_text() {
        let messages = vec![Message::user().with_text("Hello world")];

        let (images, new_msgs) = extract_images_from_messages(&messages, "<__media__>");
        assert!(images.is_empty());
        assert_eq!(new_msgs[0].as_concat_text(), "Hello world");
    }

    #[test]
    fn test_messages_extract_multiple_images() {
        let b64 = tiny_png_base64();
        let messages = vec![Message::user()
            .with_image(b64.clone(), "image/png")
            .with_text("describe both")
            .with_image(b64, "image/png")];

        let (images, new_msgs) = extract_images_from_messages(&messages, "<__media__>");
        assert_eq!(images.len(), 2);
        assert_eq!(new_msgs[0].content.len(), 3);
        assert_eq!(
            new_msgs[0].as_concat_text(),
            "<__media__>\ndescribe both\n<__media__>"
        );
    }

    #[test]
    fn test_messages_extract_no_images() {
        let messages = vec![Message::user().with_text("just text")];

        let (images, new_msgs) = extract_images_from_messages(&messages, "<__media__>");
        assert!(images.is_empty());
        assert_eq!(new_msgs[0].as_concat_text(), "just text");
    }

    #[test]
    fn test_messages_extract_invalid_base64() {
        let messages = vec![Message::user().with_image("not-valid-base64!!!", "image/png")];

        let (images, new_msgs) = extract_images_from_messages(&messages, "<__media__>");
        assert!(images.is_empty());
        assert!(new_msgs[0].as_concat_text().contains("failed to decode"));
    }

    #[test]
    fn test_messages_extract_mixed_content() {
        let b64 = tiny_png_base64();
        let messages = vec![
            Message::user()
                .with_text("What is this?")
                .with_image(b64, "image/png"),
            Message::assistant().with_text("It looks like a red pixel."),
        ];

        let (images, new_msgs) = extract_images_from_messages(&messages, "<__media__>");
        assert_eq!(images.len(), 1);
        assert_eq!(new_msgs.len(), 2);
        assert_eq!(new_msgs[0].as_concat_text(), "What is this?\n<__media__>");
        assert_eq!(new_msgs[1].as_concat_text(), "It looks like a red pixel.");
    }
}
