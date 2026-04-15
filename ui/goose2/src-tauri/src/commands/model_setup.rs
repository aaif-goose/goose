use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::services::acp::resolve_goose_binary;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ModelSetupOutput {
    provider_id: String,
    line: String,
}

#[tauri::command]
pub async fn authenticate_model_provider(
    app_handle: AppHandle,
    provider_id: String,
    provider_label: String,
) -> Result<(), String> {
    if cfg!(target_os = "windows") {
        return Err("Native Goose sign-in is not supported on Windows yet".to_string());
    }

    let goose_binary = resolve_goose_binary()?;
    let quoted_label = shell_quote(&provider_label);
    let quoted_binary = shell_quote(&goose_binary.to_string_lossy());

    let command = if cfg!(target_os = "linux") {
        format!(
            "printf '\\n%s\\n' {quoted_label} | script -qf /dev/null -c '{quoted_binary} configure'",
        )
    } else {
        format!("printf '\\n%s\\n' {quoted_label} | script -q /dev/null {quoted_binary} configure",)
    };

    run_shell_command(&app_handle, &provider_id, &command).await
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn strip_ansi(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut output = String::new();

    while index < bytes.len() {
        if bytes[index] == 0x1b {
            index += 1;
            if index < bytes.len() && bytes[index] == b'[' {
                index += 1;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            }
            continue;
        }

        if bytes[index].is_ascii_control() {
            index += 1;
            continue;
        }

        output.push(bytes[index] as char);
        index += 1;
    }

    output
}

fn normalize_output_line(line: &str) -> Option<String> {
    let cleaned = strip_ansi(line);
    let trimmed = cleaned
        .trim()
        .trim_start_matches(['│', '┌', '└', '◆', '◇', '●', '○'])
        .trim();

    if trimmed.is_empty()
        || trimmed == "Configure Providers"
        || trimmed == "What would you like to configure?"
        || trimmed == "Which model provider should we use?"
        || trimmed == "This will update your existing config files"
        || trimmed.starts_with("if you prefer, you can edit them directly at")
    {
        return None;
    }

    Some(trimmed.to_string())
}

fn is_relevant_output(line: &str) -> bool {
    line.starts_with("Configuring ")
        || line.starts_with("Please visit ")
        || line.starts_with("Open ")
        || line.starts_with("Opening ")
        || line.starts_with("Waiting ")
        || line.starts_with("Authentication")
        || line.starts_with("Authenticated")
        || line.starts_with("Saved ")
        || line.contains("oauth")
        || line.contains("OAuth")
        || line.contains("device code")
        || line.contains("login/device")
        || line.contains("browser")
}

async fn run_shell_command(
    app_handle: &AppHandle,
    provider_id: &str,
    command: &str,
) -> Result<(), String> {
    let shell = if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "sh"
    };
    let flag = if cfg!(target_os = "windows") {
        "/C"
    } else {
        "-c"
    };

    let _ = app_handle.emit(
        "model-setup:output",
        ModelSetupOutput {
            provider_id: provider_id.to_string(),
            line: "Starting Goose sign-in...".to_string(),
        },
    );

    let mut child = tokio::process::Command::new(shell)
        .arg(flag)
        .arg(command)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start sign-in flow: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let provider_id_stdout = provider_id.to_string();
    let app_stdout = app_handle.clone();

    let stdout_task = tokio::spawn(async move {
        let mut has_relevant_output = false;
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Some(line) = normalize_output_line(&line) else {
                    continue;
                };

                if !has_relevant_output && is_relevant_output(&line) {
                    has_relevant_output = true;
                }

                if has_relevant_output {
                    let _ = app_stdout.emit(
                        "model-setup:output",
                        ModelSetupOutput {
                            provider_id: provider_id_stdout.clone(),
                            line,
                        },
                    );
                }
            }
        }
    });

    let provider_id_stderr = provider_id.to_string();
    let app_stderr = app_handle.clone();

    let stderr_task = tokio::spawn(async move {
        let mut has_relevant_output = false;
        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Some(line) = normalize_output_line(&line) else {
                    continue;
                };

                if !has_relevant_output && is_relevant_output(&line) {
                    has_relevant_output = true;
                }

                if has_relevant_output {
                    let _ = app_stderr.emit(
                        "model-setup:output",
                        ModelSetupOutput {
                            provider_id: provider_id_stderr.clone(),
                            line,
                        },
                    );
                }
            }
        }
    });

    let _ = tokio::join!(stdout_task, stderr_task);

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for sign-in flow: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(-1);
        Err(format!("Goose sign-in exited with code {code}"))
    }
}
