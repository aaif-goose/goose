use super::SystemAutomation;
use std::io::Result;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;

static INIT: Once = Once::new();

#[derive(Debug)]
pub enum DisplayServer {
    X11,
    Wayland,
    Unknown,
}

pub struct LinuxAutomation {
    display_server: DisplayServer,
}

impl Default for LinuxAutomation {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxAutomation {
    pub fn new() -> Self {
        let automation = LinuxAutomation {
            display_server: Self::detect_display_server(),
        };

        if automation.has_display() {
            INIT.call_once(|| {
                automation.initialize().unwrap_or_else(|e| {
                    eprintln!("Warning: Failed to initialize Linux automation: {}", e);
                });
            });
        }

        automation
    }

    pub fn has_display(&self) -> bool {
        !matches!(self.display_server, DisplayServer::Unknown)
    }

    fn detect_display_server() -> DisplayServer {
        if let Ok(wayland_display) = std::env::var("WAYLAND_DISPLAY") {
            if !wayland_display.is_empty() {
                return DisplayServer::Wayland;
            }
        }

        if let Ok(display) = std::env::var("DISPLAY") {
            if !display.is_empty() {
                return DisplayServer::X11;
            }
        }

        DisplayServer::Unknown
    }

    fn initialize(&self) -> Result<()> {
        // Check for common dependencies first
        self.check_common_dependencies()?;

        // Check display server specific dependencies
        match self.display_server {
            DisplayServer::X11 => self.check_x11_dependencies()?,
            DisplayServer::Wayland => self.check_wayland_dependencies()?,
            DisplayServer::Unknown => {
                return Err(std::io::Error::other("Unable to detect display server"));
            }
        }

        Ok(())
    }

    fn check_common_dependencies(&self) -> Result<()> {
        let common_deps = ["bash", "python3"];
        self.check_dependencies(&common_deps)
    }

    fn check_x11_dependencies(&self) -> Result<()> {
        let x11_deps = ["xdotool", "wmctrl", "xclip", "xwininfo"];
        self.check_dependencies(&x11_deps)
    }

    fn check_wayland_dependencies(&self) -> Result<()> {
        let wayland_deps = ["wtype", "wl-copy", "wl-paste"];
        self.check_dependencies(&wayland_deps)
    }

    fn check_dependencies(&self, deps: &[&str]) -> Result<()> {
        for dep in deps {
            if !Command::new("which").arg(dep).output()?.status.success() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Required dependency '{}' not found", dep),
                ));
            }
        }
        Ok(())
    }

    fn execute_input_command(&self, cmd: &str) -> Result<String> {
        match self.display_server {
            DisplayServer::X11 => self.execute_x11_command(cmd),
            DisplayServer::Wayland => self.execute_wayland_command(cmd),
            DisplayServer::Unknown => Err(std::io::Error::other("Unknown display server")),
        }
    }

    fn execute_x11_command(&self, cmd: &str) -> Result<String> {
        if cmd.starts_with("click") {
            Command::new("xdotool").arg("click").arg("1").output()?;
            Ok(String::new())
        } else if let Some(text) = cmd.strip_prefix("type ") {
            Command::new("xdotool").arg("type").arg(text).output()?;
            Ok(String::new())
        } else if let Some(key) = cmd.strip_prefix("key ") {
            Command::new("xdotool").arg("key").arg(key).output()?;
            Ok(String::new())
        } else if let Some(window) = cmd.strip_prefix("activate ") {
            Command::new("wmctrl").arg("-a").arg(window).output()?;
            Ok(String::new())
        } else if cmd == "get clipboard" {
            let output = Command::new("xclip")
                .arg("-o")
                .arg("-selection")
                .arg("clipboard")
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else if let Some(text) = cmd.strip_prefix("set clipboard ") {
            let mut child = Command::new("xclip")
                .arg("-selection")
                .arg("clipboard")
                .stdin(std::process::Stdio::piped())
                .spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()?;
            Ok(String::new())
        } else {
            Ok(String::new())
        }
    }

    fn execute_wayland_command(&self, cmd: &str) -> Result<String> {
        if let Some(text) = cmd.strip_prefix("type ") {
            Command::new("wtype").arg(text).output()?;
            Ok(String::new())
        } else if let Some(key) = cmd.strip_prefix("key ") {
            Command::new("wtype").arg(key).output()?;
            Ok(String::new())
        } else if cmd == "get clipboard" {
            let output = Command::new("wl-paste").output()?;
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else if let Some(text) = cmd.strip_prefix("set clipboard ") {
            let mut child = Command::new("wl-copy")
                .stdin(std::process::Stdio::piped())
                .spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()?;
            Ok(String::new())
        } else {
            // Some commands might not be available in Wayland
            Ok(String::new())
        }
    }

    fn create_python_script(&self, _commands: &[&str]) -> String {
        String::from(
            r#"#!/usr/bin/env python3
import subprocess
import sys

def run_command(cmd):
    try:
        result = subprocess.run(cmd.split(), shell=False, capture_output=True, text=True)
        return result.stdout
    except Exception as e:
        print(f"Error executing {cmd}: {e}", file=sys.stderr)
        return ""

for cmd in sys.argv[1:]:
    run_command(cmd)
"#,
        )
    }
}

impl SystemAutomation for LinuxAutomation {
    fn execute_system_script(&self, script: &str) -> Result<String> {
        // Parse the script into individual commands
        let commands: Vec<_> = script
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();

        // For complex automation sequences, use Python as an intermediary
        if commands.len() > 1 {
            let python_script = self.create_python_script(&commands);
            let mut temp_path = self.get_temp_path();
            temp_path.push("automation_script.py");

            std::fs::write(&temp_path, python_script)?;

            #[cfg(unix)]
            std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755))?;

            #[cfg(not(unix))]
            {
                // On non-Unix systems, we don't set execute permissions
                // The script will be executed by the Python interpreter directly
            }

            let output = Command::new("python3")
                .arg(&temp_path)
                .args(commands.iter())
                .output()?;

            std::fs::remove_file(temp_path)?;

            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            } else {
                Err(std::io::Error::other(
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                ))
            }
        } else if let Some(cmd) = commands.first() {
            // For single commands, execute directly
            self.execute_input_command(cmd)
        } else {
            Ok(String::new())
        }
    }

    fn get_shell_command(&self) -> (&'static str, &'static str) {
        ("bash", "-c")
    }

    fn get_temp_path(&self) -> PathBuf {
        std::env::temp_dir()
    }

    fn has_display(&self) -> bool {
        self.has_display()
    }
}
