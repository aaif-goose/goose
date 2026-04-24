use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};

struct AgentCommandDef {
    id: &'static str,
    binary_name: &'static str,
    install_command: Option<&'static str>,
    auth_command: Option<&'static str>,
    auth_status_command: Option<&'static str>,
}

const AGENT_COMMAND_DEFS: &[AgentCommandDef] = &[
    AgentCommandDef {
        id: "claude-acp",
        binary_name: "claude-agent-acp",
        install_command: Some(
            "npm install -g @anthropic-ai/claude-code @agentclientprotocol/claude-agent-acp",
        ),
        auth_command: Some("claude auth login"),
        auth_status_command: Some("claude auth status"),
    },
    AgentCommandDef {
        id: "codex-acp",
        binary_name: "codex-acp",
        install_command: Some("npm install -g @openai/codex @zed-industries/codex-acp"),
        auth_command: Some("codex login"),
        auth_status_command: Some("codex login status"),
    },
    AgentCommandDef {
        id: "copilot-acp",
        binary_name: "copilot",
        install_command: Some("npm install -g @github/copilot"),
        auth_command: Some("copilot login"),
        auth_status_command: None,
    },
    AgentCommandDef {
        id: "amp-acp",
        binary_name: "amp-acp",
        install_command: Some("npm install -g @sourcegraph/amp@latest amp-acp"),
        auth_command: Some("amp login"),
        auth_status_command: Some("amp usage"),
    },
    AgentCommandDef {
        id: "cursor-agent",
        binary_name: "cursor-agent",
        install_command: Some("curl -fsSL https://cursor.com/install | bash"),
        auth_command: Some("cursor-agent login"),
        auth_status_command: Some("cursor-agent status"),
    },
    AgentCommandDef {
        id: "pi-acp",
        binary_name: "pi-acp",
        install_command: None,
        auth_command: None,
        auth_status_command: None,
    },
];

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AgentSetupOutput {
    provider_id: String,
    line: String,
}

fn find_agent_command_def(provider_id: &str) -> Option<&'static AgentCommandDef> {
    AGENT_COMMAND_DEFS.iter().find(|def| def.id == provider_id)
}

fn get_agent_command_def(provider_id: &str) -> Result<&'static AgentCommandDef, String> {
    find_agent_command_def(provider_id)
        .ok_or_else(|| format!("Unknown agent provider '{provider_id}'"))
}

fn build_extended_path() -> String {
    let mut paths: Vec<PathBuf> = Vec::new();

    if let Ok(system_path) = std::env::var("PATH") {
        paths.extend(std::env::split_paths(&system_path).filter(|p| {
            !p.to_string_lossy().contains(".hermit") && !p.join("activate-hermit").exists()
        }));
    }

    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local/bin"));
        paths.push(home.join(".npm-global/bin"));
    }

    paths.push(PathBuf::from("/usr/local/bin"));

    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/opt/homebrew/bin"));
        paths.push(PathBuf::from("/opt/local/bin"));
    }

    if cfg!(windows) {
        if let Some(appdata) = dirs::data_dir() {
            paths.push(appdata.join("npm"));
        }
    }

    if let Some(home) = dirs::home_dir() {
        let nvm_dir = home.join(".nvm/versions/node");
        if nvm_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
                let mut versions: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .collect();
                versions.sort_by_key(|b| std::cmp::Reverse(b.file_name()));
                if let Some(latest) = versions.first() {
                    paths.push(latest.path().join("bin"));
                }
            }
        }

        let fnm_dir = home.join(".local/share/fnm/node-versions");
        if fnm_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&fnm_dir) {
                let mut versions: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .collect();
                versions.sort_by_key(|b| std::cmp::Reverse(b.file_name()));
                if let Some(latest) = versions.first() {
                    paths.push(latest.path().join("installation/bin"));
                }
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    paths.retain(|p| seen.insert(p.clone()));

    std::env::join_paths(paths)
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

#[tauri::command]
pub async fn check_agent_installed(provider_id: String) -> Result<bool, String> {
    let def = get_agent_command_def(&provider_id)?;
    let extended_path = build_extended_path();

    let (cmd, flag) = if cfg!(target_os = "windows") {
        ("where", "/Q")
    } else {
        ("which", "")
    };

    let mut command = std::process::Command::new(cmd);
    if !flag.is_empty() {
        command.arg(flag);
    }
    command.arg(def.binary_name);
    command.env("PATH", &extended_path);

    Ok(command
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false))
}

#[tauri::command]
pub async fn check_agent_auth(provider_id: String) -> Result<bool, String> {
    let def = get_agent_command_def(&provider_id)?;
    let Some(auth_status_command) = def.auth_status_command else {
        return Ok(false);
    };

    let extended_path = build_extended_path();

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

    std::process::Command::new(shell)
        .arg(flag)
        .arg(auth_status_command)
        .env("PATH", &extended_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map(|output| output.status.success())
        .map_err(|e| format!("Failed to check auth status: {e}"))
}

#[tauri::command]
pub async fn install_agent(app_handle: AppHandle, provider_id: String) -> Result<(), String> {
    let def = get_agent_command_def(&provider_id)?;
    let install_command = def
        .install_command
        .ok_or_else(|| format!("Agent provider '{provider_id}' does not support install"))?;
    run_shell_command(&app_handle, &provider_id, install_command).await
}

#[tauri::command]
pub async fn authenticate_agent(app_handle: AppHandle, provider_id: String) -> Result<(), String> {
    let def = get_agent_command_def(&provider_id)?;
    let auth_command = def
        .auth_command
        .ok_or_else(|| format!("Agent provider '{provider_id}' does not support auth"))?;
    run_shell_command(&app_handle, &provider_id, auth_command).await
}

fn strip_npm_config_env(cmd: &mut tokio::process::Command) {
    for (key, _) in std::env::vars() {
        if key.starts_with("npm_config") || key.starts_with("NPM_CONFIG") {
            cmd.env_remove(&key);
        }
    }
}

async fn run_shell_command(
    app_handle: &AppHandle,
    provider_id: &str,
    command: &str,
) -> Result<(), String> {
    let extended_path = build_extended_path();

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

    let mut child_cmd = tokio::process::Command::new(shell);
    child_cmd
        .arg(flag)
        .arg(command)
        .env("PATH", &extended_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    strip_npm_config_env(&mut child_cmd);

    let mut child = child_cmd
        .spawn()
        .map_err(|e| format!("Failed to start command: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let provider_id_owned = provider_id.to_string();
    let app_for_stdout = app_handle.clone();

    let stdout_task = tokio::spawn(async move {
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_for_stdout.emit(
                    "agent-setup:output",
                    AgentSetupOutput {
                        provider_id: provider_id_owned.clone(),
                        line,
                    },
                );
            }
        }
    });

    let provider_id_owned2 = provider_id.to_string();
    let app_for_stderr = app_handle.clone();

    let stderr_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_for_stderr.emit(
                    "agent-setup:output",
                    AgentSetupOutput {
                        provider_id: provider_id_owned2.clone(),
                        line,
                    },
                );
            }
        }
    });

    let _ = tokio::join!(stdout_task, stderr_task);

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for command: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(-1);
        Err(format!("Command exited with code {code}"))
    }
}
