#![cfg(unix)]

use goose::agents::platform_extensions::developer::shell::{ShellOutput, ShellParams, ShellTool};
use rmcp::model::{CallToolResult, ContentBlock};

async fn run_with_capture_limit(limit: usize, command: &str) -> (CallToolResult, ShellOutput) {
    let limit = limit.to_string();
    let _env = env_lock::lock_env([("GOOSE_SHELL_CAPTURE_LIMIT_BYTES", Some(limit.as_str()))]);
    let result = ShellTool::new()
        .unwrap()
        .shell(ShellParams {
            command: command.to_string(),
            timeout_secs: None,
        })
        .await;
    let output = serde_json::from_value(result.structured_content.clone().unwrap()).unwrap();
    (result, output)
}

fn text(result: &CallToolResult) -> &str {
    match &result.content[0] {
        ContentBlock::Text(text) => &text.text,
        _ => panic!("expected text"),
    }
}

#[tokio::test]
async fn capture_limit_preserves_head_and_tail_of_mixed_streams() {
    let (result, output) = run_with_capture_limit(
        2000,
        "for i in $(seq 1 100); do \
            echo \"stdout-line-$i\"; \
            printf 'stderr-line-%d\\n' $i >&2; \
        done",
    )
    .await;

    assert_eq!(result.is_error, Some(false));
    assert_eq!(output.exit_code, Some(0));
    assert!(text(&result).contains("[Capture limit"));
    assert!(output.capture_dropped_bytes > 0);
    assert!(output.stdout.starts_with("stdout-line-1\n"));
    assert!(output.stdout.ends_with("stdout-line-100"));
    assert!(output.stderr.starts_with("stderr-line-1\n"));
    assert!(output.stderr.ends_with("stderr-line-100"));
    assert!(!output.stdout.contains("[Capture limit"));
    assert!(output.stdout.len() + output.stderr.len() <= 2000);
}

#[tokio::test]
async fn capture_limit_bounds_output_without_newlines() {
    let (result, output) = run_with_capture_limit(
        1000,
        "python3 -c \"import sys; sys.stdout.buffer.write(b'S' + b'x' * 999998 + b'E')\"",
    )
    .await;

    assert_eq!(result.is_error, Some(false));
    assert!(text(&result).contains("[Capture limit"));
    assert!(output.capture_dropped_bytes > 900_000);
    assert!(output.stdout.len() <= 1000);
    assert!(output.stdout.starts_with('S'));
    assert!(output.stdout.ends_with('E'));
    assert_eq!(
        output.stdout.matches('\n').count(),
        1,
        "only the head/tail gap may introduce a line break"
    );
}
