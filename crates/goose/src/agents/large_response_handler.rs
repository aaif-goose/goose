use chrono::Utc;
use rmcp::model::{CallToolResult, Content, ErrorData};
use std::fs::File;
use std::io::Write;

const DEFAULT_LARGE_TEXT_THRESHOLD: usize = 200_000;
const RANGE_AWARE_TOOL_THRESHOLD: usize = 50_000;

fn threshold_for_tool(tool_name: &str) -> usize {
    // Tool names vary by provider (`shell`, `developer__shell`, `platform__developer__shell`,
    // `read`, `developer__read`, ...), so suffix-match. Tools that already self-truncate
    // or accept range parameters get a tighter safety net; opaque extension outputs keep
    // the default.
    if tool_name.ends_with("shell") || tool_name.ends_with("read") {
        RANGE_AWARE_TOOL_THRESHOLD
    } else {
        DEFAULT_LARGE_TEXT_THRESHOLD
    }
}

fn redirect_message(char_count: usize, path: &str) -> String {
    format!(
        "Tool output was {char_count} characters and is saved to {path}. To view portions, \
         use the `read` tool with `path: {path}` (and `line`/`limit` for ranges) if available, \
         or `sed -n 'A,Bp' {path}` via shell. Do NOT re-run the tool to see different slices."
    )
}

/// Process tool response and handle large text content
pub fn process_tool_response(
    response: Result<CallToolResult, ErrorData>,
    tool_name: &str,
) -> Result<CallToolResult, ErrorData> {
    let threshold = threshold_for_tool(tool_name);
    match response {
        Ok(mut result) => {
            let mut processed_contents = Vec::new();

            for content in result.content {
                match content.as_text() {
                    Some(text_content) => {
                        let char_count = text_content.text.chars().count();
                        if char_count > threshold {
                            match write_large_text_to_file(&text_content.text) {
                                Ok(file_path) => {
                                    processed_contents.push(Content::text(redirect_message(
                                        char_count, &file_path,
                                    )));
                                }
                                Err(e) => {
                                    let warning = format!(
                                        "Warning: Failed to write large response to file: {}. Showing full content instead.\n\n{}",
                                        e, text_content.text
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

/// Write large text content to a temporary file
fn write_large_text_to_file(content: &str) -> Result<String, std::io::Error> {
    // Create temp directory if it doesn't exist
    let temp_dir = std::env::temp_dir().join("goose_mcp_responses");
    std::fs::create_dir_all(&temp_dir)?;

    // Generate a unique filename with timestamp
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S%.6f");
    let filename = format!("mcp_response_{}.txt", timestamp);
    let file_path = temp_dir.join(&filename);

    // Write content to file
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

    fn extract_saved_path(message: &str) -> &str {
        // Redirect message shape: "... saved to {path}. To view portions, ..."
        // Saved filenames contain dots (microseconds + ".txt"), so split on the
        // sentinel that follows the path, not on the first '.'.
        let after = message
            .split("saved to ")
            .nth(1)
            .expect("redirect message contains saved path");
        after
            .split(". To view")
            .next()
            .expect("path is followed by '. To view'")
            .trim()
    }

    #[test]
    fn test_small_text_response_passes_through() {
        let small_text = "This is a small text response";
        let content = Content::text(small_text.to_string());

        let response = Ok(CallToolResult::success(vec![content]));
        let processed = process_tool_response(response, "some_extension__tool").unwrap();

        assert_eq!(processed.content.len(), 1);
        let text_content = processed.content[0]
            .as_text()
            .expect("expected text content");
        assert_eq!(text_content.text, small_text);
    }

    #[test]
    fn test_large_text_response_redirected_to_file() {
        let large_text = "a".repeat(DEFAULT_LARGE_TEXT_THRESHOLD + 1000);
        let content = Content::text(large_text.clone());

        let response = Ok(CallToolResult::success(vec![content]));
        let processed = process_tool_response(response, "some_extension__tool").unwrap();

        assert_eq!(processed.content.len(), 1);
        let text_content = processed.content[0]
            .as_text()
            .expect("expected text content");
        assert!(text_content.text.contains("Tool output was"));
        assert!(text_content.text.contains("characters"));

        let path = Path::new(extract_saved_path(&text_content.text));
        assert!(path.exists(), "redirect message names a file that exists");
        let file_content = fs::read_to_string(path).expect("file is readable");
        assert_eq!(file_content, large_text);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_image_content_passes_through() {
        let image_content = Content::image("base64data".to_string(), "image/png".to_string());

        let response = Ok(CallToolResult::success(vec![image_content]));
        let processed = process_tool_response(response, "some_extension__tool").unwrap();

        assert_eq!(processed.content.len(), 1);
        let img = processed.content[0].as_image().expect("expected image");
        assert_eq!(img.data, "base64data");
        assert_eq!(img.mime_type, "image/png");
    }

    #[test]
    fn test_mixed_content_handled_correctly() {
        let small_text = Content::text("Small text");
        let large_text = Content::text("a".repeat(DEFAULT_LARGE_TEXT_THRESHOLD + 1000));
        let image = Content::image("image_data".to_string(), "image/jpeg".to_string());

        let response = Ok(CallToolResult::success(vec![small_text, large_text, image]));
        let processed = process_tool_response(response, "some_extension__tool").unwrap();

        assert_eq!(processed.content.len(), 3);

        assert_eq!(
            processed.content[0].as_text().expect("expected text").text,
            "Small text"
        );

        let redirect = &processed.content[1].as_text().expect("expected text").text;
        assert!(redirect.contains("Tool output was"));
        let path = Path::new(extract_saved_path(redirect));
        assert!(path.exists());
        let _ = fs::remove_file(path);

        let img = processed.content[2].as_image().expect("expected image");
        assert_eq!(img.data, "image_data");
        assert_eq!(img.mime_type, "image/jpeg");
    }

    #[test]
    fn test_error_response_passes_through() {
        let error = ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: Cow::from("Test error"),
            data: None,
        };
        let response: Result<CallToolResult, ErrorData> = Err(error);
        let processed = process_tool_response(response, "shell");

        let err = processed.expect_err("expected error to pass through");
        assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(err.message, "Test error");
    }

    #[test]
    fn threshold_lower_for_shell_and_read_tools() {
        // 60K-char output: above shell/read threshold (50K) but below default (200K).
        let text = "a".repeat(RANGE_AWARE_TOOL_THRESHOLD + 10_000);
        let response = Ok(CallToolResult::success(vec![Content::text(text.clone())]));

        let processed_shell = process_tool_response(response, "developer__shell").unwrap();
        let shell_msg = &processed_shell.content[0]
            .as_text()
            .expect("expected text")
            .text;
        assert!(
            shell_msg.contains("Tool output was"),
            "shell exceeds tight threshold and should redirect"
        );
        let _ = fs::remove_file(extract_saved_path(shell_msg));

        let response = Ok(CallToolResult::success(vec![Content::text(text)]));
        let processed_other = process_tool_response(response, "some_extension__tool").unwrap();
        assert!(
            !processed_other.content[0]
                .as_text()
                .expect("expected text")
                .text
                .contains("Tool output was"),
            "non-range-aware tool stays under default threshold"
        );
    }

    #[test]
    fn redirect_message_mentions_read_tool_and_shell_fallback() {
        let text = "a".repeat(DEFAULT_LARGE_TEXT_THRESHOLD + 1);
        let response = Ok(CallToolResult::success(vec![Content::text(text)]));
        let processed = process_tool_response(response, "some_extension__tool").unwrap();
        let msg = &processed.content[0].as_text().expect("expected text").text;

        assert!(msg.contains("`read` tool"), "mentions read tool");
        assert!(msg.contains("`line`/`limit`"), "mentions range params");
        assert!(msg.contains("sed -n"), "mentions shell fallback");
        assert!(
            msg.contains("Do NOT re-run"),
            "anti-loop language is present"
        );

        let _ = fs::remove_file(extract_saved_path(msg));
    }
}
