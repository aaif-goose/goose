use crate::config::Config;
use chrono::Utc;
use rmcp::model::{CallToolResult, Content, ErrorData};
use std::fs::File;
use std::io::Write;

pub const DEFAULT_LARGE_TEXT_THRESHOLD: usize = 200_000;

fn large_text_threshold() -> usize {
    Config::global()
        .get_param::<usize>("GOOSE_MAX_TOOL_RESPONSE_SIZE")
        .unwrap_or(DEFAULT_LARGE_TEXT_THRESHOLD)
}

pub fn process_tool_response(
    response: Result<CallToolResult, ErrorData>,
) -> Result<CallToolResult, ErrorData> {
    let threshold = large_text_threshold();
    match response {
        Ok(mut result) => {
            let mut processed_contents = Vec::new();

            for content in result.content {
                match content.as_text() {
                    Some(text_content) => {
                        if text_content.text.chars().count() > threshold {
                            match write_large_text_to_file(&text_content.text) {
                                Ok(file_path) => {
                                    let message = format!(
                                        "The response returned from the tool call was larger ({} characters) and is stored in the file which you can use other tools to examine or search in: {}",
                                        text_content.text.chars().count(),
                                        file_path
                                    );
                                    processed_contents.push(Content::text(message));
                                }
                                Err(e) => {
                                    let warning = format!(
                                        "Warning: Failed to write large response to file: {}. Showing full content instead.\n\n{}",
                                        e,
                                        text_content.text
                                    );
                                    processed_contents.push(Content::text(warning));
                                }
                            }
                        } else {
                            processed_contents.push(content);
                        }
                    }
                    None => {
                        processed_contents.push(content);
                    }
                }
            }

            result.content = processed_contents;
            Ok(result)
        }
        Err(e) => Err(e),
    }
}

fn write_large_text_to_file(content: &str) -> Result<String, std::io::Error> {
    let temp_dir = std::env::temp_dir().join("goose_mcp_responses");
    std::fs::create_dir_all(&temp_dir)?;

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S%.6f");
    let file_path = temp_dir.join(format!("mcp_response_{}.txt", timestamp));

    let mut file = File::create(&file_path)?;
    file.write_all(content.as_bytes())?;

    Ok(file_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{Content, ErrorCode, ErrorData};
    use std::borrow::Cow;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_small_text_response_passes_through() {
        let small_text = "This is a small text response";
        let content = Content::text(small_text.to_string());

        let response = Ok(CallToolResult::success(vec![content]));

        let processed = process_tool_response(response).unwrap();

        assert_eq!(processed.content.len(), 1);
        if let Some(text_content) = processed.content[0].as_text() {
            assert_eq!(text_content.text, small_text);
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_large_text_response_redirected_to_file() {
        let large_text = "a".repeat(DEFAULT_LARGE_TEXT_THRESHOLD + 1000);
        let content = Content::text(large_text.clone());

        let response = Ok(CallToolResult::success(vec![content]));

        let processed = process_tool_response(response).unwrap();

        assert_eq!(processed.content.len(), 1);
        if let Some(text_content) = processed.content[0].as_text() {
            assert!(text_content
                .text
                .contains("The response returned from the tool call was larger"));
            assert!(text_content.text.contains("characters"));

            if let Some(file_path) = text_content.text.split("stored in the file: ").nth(1) {
                let path = Path::new(file_path.trim());
                if path.exists() {
                    if let Ok(file_content) = fs::read_to_string(path) {
                        assert_eq!(file_content, large_text);
                    }

                    let _ = fs::remove_file(path); // Ignore errors on cleanup
                }
            }
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_image_content_passes_through() {
        let image_content = Content::image("base64data".to_string(), "image/png".to_string());

        let response = Ok(CallToolResult::success(vec![image_content]));

        let processed = process_tool_response(response).unwrap();

        assert_eq!(processed.content.len(), 1);
        if let Some(img) = processed.content[0].as_image() {
            assert_eq!(img.data, "base64data");
            assert_eq!(img.mime_type, "image/png");
        } else {
            panic!("Expected image content");
        }
    }

    #[test]
    fn test_mixed_content_handled_correctly() {
        let small_text = Content::text("Small text");
        let large_text = Content::text("a".repeat(DEFAULT_LARGE_TEXT_THRESHOLD + 1000));
        let image = Content::image("image_data".to_string(), "image/jpeg".to_string());

        let response = Ok(CallToolResult::success(vec![small_text, large_text, image]));

        let processed = process_tool_response(response).unwrap();

        assert_eq!(processed.content.len(), 3);

        if let Some(text_content) = processed.content[0].as_text() {
            assert_eq!(text_content.text, "Small text");
        } else {
            panic!("Expected text content");
        }

        if let Some(text_content) = processed.content[1].as_text() {
            assert!(text_content
                .text
                .contains("The response returned from the tool call was larger"));

            if let Some(file_path) = text_content.text.split("stored in the file: ").nth(1) {
                let path = Path::new(file_path.trim());
                if path.exists() {
                    let _ = fs::remove_file(path); // Ignore errors on cleanup
                }
            }
        } else {
            panic!("Expected text content");
        }

        if let Some(img) = processed.content[2].as_image() {
            assert_eq!(img.data, "image_data");
            assert_eq!(img.mime_type, "image/jpeg");
        } else {
            panic!("Expected image content");
        }
    }

    #[test]
    fn test_error_response_passes_through() {
        let error = ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: Cow::from("Test error"),
            data: None,
        };
        let response: Result<CallToolResult, ErrorData> = Err(error);

        let processed = process_tool_response(response);

        assert!(processed.is_err());
        match processed {
            Err(err) => {
                assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
                assert_eq!(err.message, "Test error");
            }
            _ => panic!("Expected execution error"),
        }
    }
}
