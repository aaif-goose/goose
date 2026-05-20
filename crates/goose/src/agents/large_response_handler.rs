use crate::config::paths::Paths;
use chrono::Utc;
use rmcp::model::{CallToolResult, Content, ErrorData};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

const LARGE_TEXT_THRESHOLD: usize = 200_000;

/// Process tool response and handle large text content
pub fn process_tool_response(
    response: Result<CallToolResult, ErrorData>,
) -> Result<CallToolResult, ErrorData> {
    match response {
        Ok(mut result) => {
            let mut processed_contents = Vec::new();

            for content in result.content {
                match content.as_text() {
                    Some(text_content) => {
                        let text_len = text_content.text.chars().count();
                        if text_len > LARGE_TEXT_THRESHOLD {
                            match write_large_text_to_file(&text_content.text) {
                                Ok(file_path) => {
                                    let message = format!(
                                        "The response returned from the tool call was larger ({} characters) and is stored in the file: {}\nYou can use other tools to examine or search it.",
                                        text_len,
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
                    None => processed_contents.push(content),
                }
            }

            result.content = processed_contents;
            Ok(result)
        }
        Err(e) => Err(e),
    }
}

fn write_large_text_to_file(content: &str) -> Result<String, io::Error> {
    let spill_dir = response_spill_dir()?;
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S%.6f").to_string();
    let mut file = tempfile::Builder::new()
        .prefix(&format!("mcp_response_{timestamp}_"))
        .suffix(".txt")
        .tempfile_in(&spill_dir)?;

    file.write_all(content.as_bytes())?;
    let (_file, file_path) = file.keep().map_err(|err| err.error)?;

    Ok(file_path.to_string_lossy().to_string())
}

fn response_spill_dir() -> Result<PathBuf, io::Error> {
    let spill_dir = Paths::in_data_dir("mcp_responses");
    ensure_private_response_spill_dir(&spill_dir)?;
    Ok(spill_dir)
}

#[cfg(unix)]
fn ensure_private_response_spill_dir(spill_dir: &Path) -> Result<(), io::Error> {
    match std::fs::symlink_metadata(spill_dir) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} is not a directory", spill_dir.display()),
                ));
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let mut builder = std::fs::DirBuilder::new();
            if let Some(parent) = spill_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            match builder.mode(0o700).create(spill_dir) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    return ensure_private_response_spill_dir(spill_dir);
                }
                Err(err) => return Err(err),
            }
        }
        Err(err) => return Err(err),
    };

    std::fs::set_permissions(spill_dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn ensure_private_response_spill_dir(spill_dir: &Path) -> Result<(), io::Error> {
    std::fs::create_dir_all(spill_dir)
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
        // Create a small text response
        let small_text = "This is a small text response";
        let content = Content::text(small_text.to_string());

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
        let large_text = "a".repeat(LARGE_TEXT_THRESHOLD + 1000);
        let content = Content::text(large_text.clone());

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

            let path = large_response_file_path(&text_content.text);
            let file_content = fs::read_to_string(path).unwrap();
            assert_eq!(file_content, large_text);
            let _ = fs::remove_file(path);
        } else {
            panic!("Expected text content");
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_large_text_response_file_permissions_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let large_text = "a".repeat(LARGE_TEXT_THRESHOLD + 1000);
        let response = Ok(CallToolResult::success(vec![Content::text(large_text)]));
        let processed = process_tool_response(response).unwrap();
        let text = processed.content[0].as_text().unwrap();
        let path = large_response_file_path(&text.text);
        let metadata = fs::metadata(path).unwrap();
        let parent_metadata = fs::metadata(path.parent().unwrap()).unwrap();

        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(parent_metadata.permissions().mode() & 0o777, 0o700);

        let _ = fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn test_response_spill_dir_rejects_symlink() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().unwrap();
        let target = parent.path().join("target");
        let link = parent.path().join("mcp_responses");
        fs::create_dir(&target).unwrap();
        symlink(&target, &link).unwrap();

        let err = ensure_private_response_spill_dir(&link).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn test_large_text_response_uses_data_dir_for_spill_file() {
        let temp_root = tempfile::tempdir().unwrap();
        let _guard =
            env_lock::lock_env([("GOOSE_PATH_ROOT", Some(temp_root.path().to_str().unwrap()))]);
        let large_text = "a".repeat(LARGE_TEXT_THRESHOLD + 1000);
        let processed =
            process_tool_response(Ok(CallToolResult::success(vec![Content::text(large_text)])))
                .unwrap();
        let text = processed.content[0].as_text().unwrap();
        let path = large_response_file_path(&text.text);

        assert!(path.starts_with(temp_root.path().join("data").join("mcp_responses")));

        let _ = fs::remove_file(path);
    }

    fn large_response_file_path(message: &str) -> &Path {
        let file_path = message
            .split("stored in the file: ")
            .nth(1)
            .expect("response should include file path")
            .lines()
            .next()
            .expect("file path should be on its own line");
        Path::new(file_path.trim())
    }

    #[test]
    fn test_image_content_passes_through() {
        // Create an image content
        let image_content = Content::image("base64data".to_string(), "image/png".to_string());

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
        let small_text = Content::text("Small text");
        let large_text = Content::text("a".repeat(LARGE_TEXT_THRESHOLD + 1000));
        let image = Content::image("image_data".to_string(), "image/jpeg".to_string());

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
            if let Some(file_path) = text_content.text.split("stored in the file: ").nth(1) {
                let path = Path::new(file_path.trim());
                if path.exists() {
                    let _ = fs::remove_file(path); // Ignore errors on cleanup
                }
            }
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
}
