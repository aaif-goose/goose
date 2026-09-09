use base64::prelude::*;

use goose_provider_types::conversation::message::{Message, MessageContent};

#[derive(Debug)]
pub struct ExtractedImage {
    pub bytes: Vec<u8>,
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
