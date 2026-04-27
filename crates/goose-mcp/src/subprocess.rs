use std::sync::OnceLock;
use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW_FLAG: u32 = 0x08000000;

pub trait SubprocessExt {
    fn set_no_window(&mut self) -> &mut Self;
}

impl SubprocessExt for Command {
    fn set_no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            self.creation_flags(CREATE_NO_WINDOW_FLAG);
        }
        self
    }
}

impl SubprocessExt for std::process::Command {
    fn set_no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(CREATE_NO_WINDOW_FLAG);
        }
        self
    }
}

/// Resolve the user's full PATH by running a login shell.
///
/// When goosed is launched from a desktop app (e.g. Electron), it may inherit
/// a minimal PATH like `/usr/bin:/bin`. This function spawns a login shell to
/// source the user's profile and recover the full PATH.
///
/// Ported from `crates/goose/src/agents/platform_extensions/developer/shell.rs`
/// where it was introduced in #5774 for the developer extension. This makes the
/// same fix available to all MCP extensions in goose-mcp.
#[cfg(not(windows))]
fn resolve_login_shell_path() -> Option<String> {
    use process_wrap::std::{CommandWrap, ProcessSession};
    use std::path::PathBuf;
    use std::process::Stdio;

    // Prefer the user's configured shell so we source the right profile files.
    // Fall back to /bin/bash (common default) then sh as last resort.
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| PathBuf::from(s).is_file())
        .unwrap_or_else(|| {
            if PathBuf::from("/bin/bash").is_file() {
                "/bin/bash".to_string()
            } else {
                "sh".to_string()
            }
        });

    let mut cmd = CommandWrap::from(std::process::Command::new(&shell));
    cmd.command_mut()
        .args(["-l", "-i", "-c", "echo $PATH"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    // Spawn in a new session so that interactive shell job-control setup
    // cannot steal the terminal foreground from the parent goose process.
    cmd.wrap(ProcessSession);

    let child = cmd.spawn().ok()?;
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }

    // Take the last non-empty line — interactive shells may emit
    // extra output from profile scripts before our echo.
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .filter(|path| !path.is_empty())
}

/// Returns the user's full login shell PATH, resolved once and cached.
///
/// Call this before spawning subprocesses to ensure they inherit the user's
/// full PATH rather than the restricted one from the desktop app launcher.
#[cfg(not(windows))]
pub fn user_login_path() -> Option<&'static str> {
    static CACHED: OnceLock<Option<String>> = OnceLock::new();
    CACHED.get_or_init(resolve_login_shell_path).as_deref()
}

/// Merge the login shell PATH with the current process PATH.
///
/// Prepends login shell entries so user tools are found first, while
/// preserving any runtime PATH additions (e.g. from direnv, nix, or
/// auto-install helpers like ensure_peekaboo).
#[cfg(not(windows))]
pub fn merged_path() -> Option<String> {
    let login = user_login_path()?;
    let current = std::env::var("PATH").unwrap_or_default();
    if current.is_empty() {
        return Some(login.to_string());
    }
    // Deduplicate: login shell entries first, then any current entries not already present.
    let login_entries: Vec<&str> = login.split(':').collect();
    let mut seen: std::collections::HashSet<&str> = login_entries.iter().copied().collect();
    let mut merged = login_entries;
    for entry in current.split(':') {
        if seen.insert(entry) {
            merged.push(entry);
        }
    }
    Some(merged.join(":"))
}
