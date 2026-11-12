use crate::utils::sanitize_unicode_tags;
use base64::Engine;
use rmcp::model::ResourceContents;

pub fn extract_text_from_resource(resource: &ResourceContents) -> String {
    match resource {
        ResourceContents::TextResourceContents { text, .. } => sanitize_unicode_tags(text),
        ResourceContents::BlobResourceContents {
            blob, mime_type, ..
        } => match base64::engine::general_purpose::STANDARD.decode(blob) {
            Ok(bytes) => {
                let byte_len = bytes.len();
                match String::from_utf8(bytes) {
                    Ok(text) => sanitize_unicode_tags(&text),
                    Err(_) => {
                        let mime = mime_type
                            .as_ref()
                            .map(|m| m.as_str())
                            .unwrap_or("application/octet-stream");
                        format!("[Binary content ({}) - {} bytes]", mime, byte_len)
                    }
                }
            }
            Err(_) => sanitize_unicode_tags(blob),
        },
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("Hello, World!", "Hello, World!" ; "simple text")]
    #[test_case("Hello from GitHub!", "Hello from GitHub!" ; "github content")]
    #[test_case("visible\u{E0041}\u{E0042}text", "visibletext" ; "unicode tags")]
    #[test_case("", "" ; "empty text")]
    fn test_extract_text_from_text_resource(input: &str, expected: &str) {
        let resource = ResourceContents::TextResourceContents {
            uri: "file:///test.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
            text: input.to_string(),
            meta: None,
        };
        assert_eq!(extract_text_from_resource(&resource), expected);
    }

    #[test_case("Hello from GitHub!", "Hello from GitHub!" ; "utf8 markdown")]
    #[test_case("Simple text", "Simple text" ; "utf8 plain")]
    #[test_case("visible\u{E0041}\u{E0042}text", "visibletext" ; "unicode tags")]
    fn test_extract_text_from_blob_utf8(input: &str, expected: &str) {
        let blob = base64::engine::general_purpose::STANDARD.encode(input.as_bytes());
        let resource = ResourceContents::BlobResourceContents {
            uri: "github://repo/file.md".to_string(),
            mime_type: Some("text/markdown".to_string()),
            blob,
            meta: None,
        };
        assert_eq!(extract_text_from_resource(&resource), expected);
    }

    #[test]
    fn test_extract_text_from_blob_binary() {
        let binary_data: Vec<u8> = vec![0xFF, 0xFE, 0x00, 0x01, 0x89, 0x50, 0x4E, 0x47];
        let blob = base64::engine::general_purpose::STANDARD.encode(&binary_data);

        let resource = ResourceContents::BlobResourceContents {
            uri: "file:///image.png".to_string(),
            mime_type: Some("image/png".to_string()),
            blob,
            meta: None,
        };

        assert_eq!(
            extract_text_from_resource(&resource),
            "[Binary content (image/png) - 8 bytes]"
        );
    }

    #[test]
    fn test_extract_text_from_blob_binary_no_mime_type() {
        let binary_data: Vec<u8> = vec![0xFF, 0xFE];
        let blob = base64::engine::general_purpose::STANDARD.encode(&binary_data);

        let resource = ResourceContents::BlobResourceContents {
            uri: "file:///unknown".to_string(),
            mime_type: None,
            blob,
            meta: None,
        };

        assert_eq!(
            extract_text_from_resource(&resource),
            "[Binary content (application/octet-stream) - 2 bytes]"
        );
    }

    #[test]
    fn test_extract_text_from_blob_invalid_base64() {
        let resource = ResourceContents::BlobResourceContents {
            uri: "file:///test.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
            blob: "not\u{E0041} valid base64!!!".to_string(),
            meta: None,
        };
        assert_eq!(extract_text_from_resource(&resource), "not valid base64!!!");
    }
}
