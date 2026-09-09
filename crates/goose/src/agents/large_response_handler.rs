use crate::config::Config;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};
use std::io::Write;

const DEFAULT_LARGE_TEXT_THRESHOLD: usize = 200_000;

pub(crate) fn max_tool_response_size() -> usize {
    Config::global()
        .get_param::<usize>("GOOSE_MAX_TOOL_RESPONSE_SIZE")
        .unwrap_or(DEFAULT_LARGE_TEXT_THRESHOLD)
}

/// Process tool response and handle large text content
pub fn process_tool_response(
    response: Result<CallToolResult, ErrorData>,
) -> Result<CallToolResult, ErrorData> {
    let threshold = max_tool_response_size();
    match response {
        Ok(mut result) => {
            let mut processed_contents = Vec::new();

            for content in result.content {
                match content.as_text() {
                    Some(text_content) => {
                        // Check if text exceeds threshold
                        if text_content.text.chars().count() > threshold {
                            // Write to temp file
                            match write_large_text_to_file(&text_content.text) {
                                Ok(file_path) => {
                                    // Create a new text content with reference to the file
                                    let message = format!(
                                        "The response returned from the tool call was larger ({} characters) and is stored in the file which you can use other tools to examine or search in: {}",
                                        text_content.text.chars().count(),
                                        file_path
                                    );
                                    processed_contents.push(ContentBlock::text(message));
                                }
                                Err(e) => {
                                    // If file writing fails, include original content with warning
                                    let warning = format!(
                                        "Warning: Failed to write large response to file: {}. Showing full content instead.\n\n{}",
                                        e,
                                        text_content.text
                                    );
                                    processed_contents.push(ContentBlock::text(warning));
                                }
                            }
                        } else {
                            // Keep original content for smaller texts
                            processed_contents.push(content);
                        }
                    }
                    None => {
                        // Pass through other content types unchanged
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

/// Write large text content to a temporary file
fn write_large_text_to_file(content: &str) -> Result<String, std::io::Error> {
    let mut file = tempfile::Builder::new()
        .prefix("goose_mcp_response_")
        .suffix(".txt")
        .tempfile()?;
    file.write_all(content.as_bytes())?;
    let (_, file_path) = file.keep()?;

    Ok(file_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{ContentBlock, ErrorCode, ErrorData};
    use std::borrow::Cow;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_small_text_response_passes_through() {
        // Create a small text response
        let small_text = "This is a small text response";
        let content = ContentBlock::text(small_text.to_string());

        let response = Ok(CallToolResult::success(vec![content]));

        // Process the response
        let processed = process_tool_response(response).unwrap();

        // Verify the response is unchanged
        assert_eq!(processed.content.len(), 1);
        if let Some(text_content) = processed.content[0].as_text() {
            assert_eq!(text_content.text, small_text);
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_large_text_response_redirected_to_file() {
        // Create a text larger than the threshold
        let large_text = "a".repeat(DEFAULT_LARGE_TEXT_THRESHOLD + 1000);
        let content = ContentBlock::text(large_text.clone());

        let response = Ok(CallToolResult::success(vec![content]));

        // Process the response
        let processed = process_tool_response(response).unwrap();

        // Verify the response contains a message about the file
        assert_eq!(processed.content.len(), 1);
        if let Some(text_content) = processed.content[0].as_text() {
            assert!(text_content
                .text
                .contains("The response returned from the tool call was larger"));
            assert!(text_content.text.contains("characters"));

            // Extract the file path from the message
            let file_path = text_content
                .text
                .rsplit_once(": ")
                .expect("spill message should contain a file path")
                .1;
            let path = Path::new(file_path.trim());
            assert!(path.exists());
            assert_eq!(fs::read_to_string(path).unwrap(), large_text);
            fs::remove_file(path).unwrap();
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_image_content_passes_through() {
        // Create an image content
        let image_content = ContentBlock::image("base64data".to_string(), "image/png".to_string());

        let response = Ok(CallToolResult::success(vec![image_content]));

        // Process the response
        let processed = process_tool_response(response).unwrap();

        // Verify the response is unchanged
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
        // Create a response with mixed content types
        let small_text = ContentBlock::text("Small text");
        let large_text = ContentBlock::text("a".repeat(DEFAULT_LARGE_TEXT_THRESHOLD + 1000));
        let image = ContentBlock::image("image_data".to_string(), "image/jpeg".to_string());

        let response = Ok(CallToolResult::success(vec![small_text, large_text, image]));

        // Process the response
        let processed = process_tool_response(response).unwrap();

        // Verify each item is handled correctly
        assert_eq!(processed.content.len(), 3);

        // First item should be unchanged small text
        if let Some(text_content) = processed.content[0].as_text() {
            assert_eq!(text_content.text, "Small text");
        } else {
            panic!("Expected text content");
        }

        // Second item should be a message about the file
        if let Some(text_content) = processed.content[1].as_text() {
            assert!(text_content
                .text
                .contains("The response returned from the tool call was larger"));

            // Extract the file path and clean up
            let file_path = text_content
                .text
                .rsplit_once(": ")
                .expect("spill message should contain a file path")
                .1;
            fs::remove_file(Path::new(file_path.trim())).unwrap();
        } else {
            panic!("Expected text content");
        }

        // Third item should be unchanged image
        if let Some(img) = processed.content[2].as_image() {
            assert_eq!(img.data, "image_data");
            assert_eq!(img.mime_type, "image/jpeg");
        } else {
            panic!("Expected image content");
        }
    }

    #[test]
    fn test_error_response_passes_through() {
        // Create an error response
        let error = ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: Cow::from("Test error"),
            data: None,
        };
        let response: Result<CallToolResult, ErrorData> = Err(error);

        // Process the response
        let processed = process_tool_response(response);

        // Verify the error is passed through unchanged
        assert!(processed.is_err());
        match processed {
            Err(err) => {
                assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
                assert_eq!(err.message, "Test error");
            }
            _ => panic!("Expected execution error"),
        }
    }

    #[test]
    fn test_large_response_files_have_unique_paths() {
        let first_path = write_large_text_to_file("first response").unwrap();
        let second_path = write_large_text_to_file("second response").unwrap();
        let paths_are_distinct = first_path != second_path;
        let first_content = fs::read_to_string(&first_path).unwrap();
        let second_content = fs::read_to_string(&second_path).unwrap();

        fs::remove_file(&first_path).unwrap();
        if paths_are_distinct {
            fs::remove_file(&second_path).unwrap();
        }

        assert!(paths_are_distinct);
        assert_eq!(first_content, "first response");
        assert_eq!(second_content, "second response");
    }

    #[cfg(unix)]
    #[test]
    fn test_large_response_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let file_path = write_large_text_to_file("sensitive response").unwrap();
        let metadata = fs::metadata(&file_path).unwrap();
        let mode = metadata.permissions().mode();
        let content = fs::read_to_string(&file_path).unwrap();
        fs::remove_file(&file_path).unwrap();

        assert_eq!(mode & 0o077, 0);
        assert_eq!(content, "sensitive response");
    }
}
