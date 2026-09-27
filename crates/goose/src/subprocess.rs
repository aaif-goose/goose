use rmcp::transport::TokioChildProcess;
use std::io;
#[cfg(target_os = "linux")]
use std::sync::{mpsc, OnceLock};
use tokio::process::ChildStderr;
use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW_FLAG: u32 = 0x08000000;

#[cfg(target_os = "linux")]
const PR_SET_CHILD_SUBREAPER: libc::c_int = 36;
#[cfg(target_os = "linux")]
const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
#[cfg(target_os = "linux")]
const CLONE_NEWNS: libc::c_int = 0x00020000;
#[cfg(target_os = "linux")]
const CLONE_NEWUTS: libc::c_int = 0x04000000;
#[cfg(target_os = "linux")]
const CLONE_NEWIPC: libc::c_int = 0x08000000;
#[cfg(target_os = "linux")]
const CLONE_NEWNET: libc::c_int = 0x40000000;

#[cfg(target_os = "linux")]
pub const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
#[cfg(target_os = "linux")]
pub const LANDLOCK_CREATE_RULESET_VERSION: libc::c_uint = 1 << 0;

/// Process fencing mode for isolating subprocesses and shell execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessFenceMode {
    #[default]
    None,
    Standard,
    Strict,
}

/// Configuration options for subprocess fencing and containment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessFenceConfig {
    pub mode: ProcessFenceMode,
    pub isolate_process_group: bool,
    pub parent_death_signal: bool,
    pub death_signal_kill: bool,
    pub child_subreaper: bool,
    pub isolate_namespaces: bool,
    pub isolate_network: bool,
    pub landlock_enabled: bool,
    pub custom_runtime: Option<String>,
}

impl Default for ProcessFenceConfig {
    fn default() -> Self {
        Self {
            mode: ProcessFenceMode::None,
            isolate_process_group: true,
            parent_death_signal: true,
            death_signal_kill: false,
            child_subreaper: false,
            isolate_namespaces: false,
            isolate_network: false,
            landlock_enabled: false,
            custom_runtime: None,
        }
    }
}

impl ProcessFenceConfig {
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let mode_str = lookup("GOOSE_PROCESS_FENCE")
            .or_else(|| lookup("GOOSE_SANDBOX"))
            .unwrap_or_default()
            .to_ascii_lowercase();

        let mode = match mode_str.as_str() {
            "strict" | "2" => ProcessFenceMode::Strict,
            "standard" | "basic" | "1" | "true" => ProcessFenceMode::Standard,
            _ => ProcessFenceMode::None,
        };

        let isolate_process_group = true;
        let parent_death_signal = true;
        let death_signal_kill = matches!(mode, ProcessFenceMode::Strict)
            || lookup("GOOSE_FENCE_PDEATHSIG")
                .or_else(|| lookup("GOOSE_PDEATHSIG"))
                .map_or(false, |v| {
                    v.eq_ignore_ascii_case("kill") || v.eq_ignore_ascii_case("sigkill") || v == "9"
                });

        let child_subreaper = matches!(mode, ProcessFenceMode::Standard | ProcessFenceMode::Strict)
            || lookup("GOOSE_FENCE_SUBREAPER")
                .map_or(false, |v| v == "1" || v.eq_ignore_ascii_case("true"));

        let isolate_namespaces = matches!(mode, ProcessFenceMode::Strict)
            || lookup("GOOSE_FENCE_NAMESPACES")
                .map_or(false, |v| v == "1" || v.eq_ignore_ascii_case("true"));

        let isolate_network = lookup("GOOSE_FENCE_ISOLATE_NET")
            .map_or(false, |v| v == "1" || v.eq_ignore_ascii_case("true"));

        let landlock_enabled =
            matches!(mode, ProcessFenceMode::Standard | ProcessFenceMode::Strict)
                || lookup("GOOSE_FENCE_LANDLOCK")
                    .map_or(false, |v| v == "1" || v.eq_ignore_ascii_case("true"));

        let custom_runtime = lookup("GOOSE_FENCE_RUNTIME")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        Self {
            mode,
            isolate_process_group,
            parent_death_signal,
            death_signal_kill,
            child_subreaper,
            isolate_namespaces,
            isolate_network,
            landlock_enabled,
            custom_runtime,
        }
    }
}

/// Returns whether process fencing or sandboxing is enabled via environment.
pub fn is_process_fencing_enabled() -> bool {
    is_process_fencing_enabled_with(|k| std::env::var(k).ok())
}

pub fn is_process_fencing_enabled_with<F>(lookup: F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    let config = ProcessFenceConfig::from_lookup(&lookup);
    config.mode != ProcessFenceMode::None || lookup("GOOSE_FENCE_RUNTIME").is_some()
}

/// Check Landlock LSM ABI availability on Linux.
#[cfg(target_os = "linux")]
pub fn probe_landlock_abi() -> Option<u32> {
    let rc = unsafe {
        libc::syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            std::ptr::null::<()>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    if rc >= 1 {
        Some(rc as u32)
    } else {
        None
    }
}

#[cfg(not(target_os = "linux"))]
pub fn probe_landlock_abi() -> Option<u32> {
    None
}

#[cfg(target_os = "linux")]
fn configure_linux_fencing(command: &mut Command, config: &ProcessFenceConfig) {
    let parent_pid = unsafe { libc::getpid() };
    let fence = config.clone();

    unsafe {
        command.pre_exec(move || {
            if fence.parent_death_signal {
                let sig = if fence.death_signal_kill {
                    libc::SIGKILL
                } else {
                    libc::SIGTERM
                };
                if libc::prctl(libc::PR_SET_PDEATHSIG, sig) != 0 {
                    return Err(std::io::Error::last_os_error());
                }

                if libc::getppid() != parent_pid {
                    return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
                }
            }

            if fence.child_subreaper {
                let _ = libc::prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0);
            }

            let mut unshare_flags = 0;
            if fence.isolate_namespaces {
                unshare_flags |= CLONE_NEWNS | CLONE_NEWIPC | CLONE_NEWUTS;
            }
            if fence.isolate_network {
                unshare_flags |= CLONE_NEWNET;
            }

            if unshare_flags != 0 {
                let _ = libc::unshare(unshare_flags);
            }

            if fence.mode != ProcessFenceMode::None || fence.landlock_enabled {
                if libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    if fence.mode == ProcessFenceMode::Strict {
                        return Err(std::io::Error::last_os_error());
                    }
                }

                if fence.landlock_enabled || fence.mode == ProcessFenceMode::Strict {
                    let abi = libc::syscall(
                        SYS_LANDLOCK_CREATE_RULESET,
                        std::ptr::null::<()>(),
                        0usize,
                        LANDLOCK_CREATE_RULESET_VERSION,
                    );
                    if abi < 1 && fence.mode == ProcessFenceMode::Strict {
                        return Err(std::io::Error::from_raw_os_error(libc::EPERM));
                    }
                }
            }

            Ok(())
        });
    }
}

#[cfg(target_os = "linux")]
fn configure_linux_std_fencing(command: &mut std::process::Command, config: &ProcessFenceConfig) {
    use std::os::unix::process::CommandExt;
    let parent_pid = unsafe { libc::getpid() };
    let fence = config.clone();

    unsafe {
        command.pre_exec(move || {
            if fence.parent_death_signal {
                let sig = if fence.death_signal_kill {
                    libc::SIGKILL
                } else {
                    libc::SIGTERM
                };
                if libc::prctl(libc::PR_SET_PDEATHSIG, sig) != 0 {
                    return Err(std::io::Error::last_os_error());
                }

                if libc::getppid() != parent_pid {
                    return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
                }
            }

            if fence.child_subreaper {
                let _ = libc::prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0);
            }

            let mut unshare_flags = 0;
            if fence.isolate_namespaces {
                unshare_flags |= CLONE_NEWNS | CLONE_NEWIPC | CLONE_NEWUTS;
            }
            if fence.isolate_network {
                unshare_flags |= CLONE_NEWNET;
            }

            if unshare_flags != 0 {
                let _ = libc::unshare(unshare_flags);
            }

            if fence.mode != ProcessFenceMode::None || fence.landlock_enabled {
                if libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    if fence.mode == ProcessFenceMode::Strict {
                        return Err(std::io::Error::last_os_error());
                    }
                }

                if fence.landlock_enabled || fence.mode == ProcessFenceMode::Strict {
                    let abi = libc::syscall(
                        SYS_LANDLOCK_CREATE_RULESET,
                        std::ptr::null::<()>(),
                        0usize,
                        LANDLOCK_CREATE_RULESET_VERSION,
                    );
                    if abi < 1 && fence.mode == ProcessFenceMode::Strict {
                        return Err(std::io::Error::from_raw_os_error(libc::EPERM));
                    }
                }
            }

            Ok(())
        });
    }
}

pub trait SubprocessExt {
    fn set_no_window(&mut self) -> &mut Self;
    fn apply_process_fencing(&mut self) -> &mut Self;
    fn apply_fence_config(&mut self, config: &ProcessFenceConfig) -> &mut Self;
}

/// Creates a Git command that rejects implicit bare repositories and cannot run a
/// repository-configured fsmonitor hook.
pub fn git_command() -> std::process::Command {
    let mut command = std::process::Command::new("git");
    command.args([
        "-c",
        "safe.bareRepository=explicit",
        "-c",
        "core.fsmonitor=false",
    ]);
    command
}

impl SubprocessExt for Command {
    fn set_no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            self.creation_flags(CREATE_NO_WINDOW_FLAG);
        }
        self
    }

    fn apply_process_fencing(&mut self) -> &mut Self {
        let config = ProcessFenceConfig::from_env();
        self.apply_fence_config(&config)
    }

    fn apply_fence_config(&mut self, config: &ProcessFenceConfig) -> &mut Self {
        #[cfg(unix)]
        if config.isolate_process_group {
            self.process_group(0);
        }
        self.set_no_window();
        #[cfg(target_os = "linux")]
        configure_linux_fencing(self, config);
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

    fn apply_process_fencing(&mut self) -> &mut Self {
        let config = ProcessFenceConfig::from_env();
        self.apply_fence_config(&config)
    }

    fn apply_fence_config(&mut self, config: &ProcessFenceConfig) -> &mut Self {
        #[cfg(unix)]
        if config.isolate_process_group {
            use std::os::unix::process::CommandExt;
            self.process_group(0);
        }
        self.set_no_window();
        #[cfg(target_os = "linux")]
        configure_linux_std_fencing(self, config);
        self
    }
}

#[allow(dead_code)]
fn configure_common_subprocess(command: &mut Command) {
    command.apply_fence_config(&ProcessFenceConfig::default());
}

#[allow(unused_variables)]
pub fn configure_subprocess(command: &mut Command) {
    command.apply_process_fencing();
}

pub fn configure_fenced_subprocess(command: &mut Command, config: &ProcessFenceConfig) {
    command.apply_fence_config(config);
}

pub fn configure_std_subprocess(command: &mut std::process::Command) {
    command.apply_process_fencing();
}

#[cfg(target_os = "linux")]
struct LongLivedSpawnRequest {
    command: Command,
    runtime: tokio::runtime::Handle,
    response: tokio::sync::oneshot::Sender<io::Result<(TokioChildProcess, Option<ChildStderr>)>>,
}

#[cfg(target_os = "linux")]
fn long_lived_spawn_sender() -> io::Result<mpsc::Sender<LongLivedSpawnRequest>> {
    static SENDER: OnceLock<io::Result<mpsc::Sender<LongLivedSpawnRequest>>> = OnceLock::new();

    match SENDER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<LongLivedSpawnRequest>();
        std::thread::Builder::new()
            .name("goose-extension-spawner".to_owned())
            .spawn(move || {
                while let Ok(mut request) = receiver.recv() {
                    let _runtime_guard = request.runtime.enter();
                    configure_subprocess(&mut request.command);
                    let result = TokioChildProcess::builder(request.command)
                        .stderr(std::process::Stdio::piped())
                        .spawn();
                    let _ = request.response.send(result);
                }
            })
            .map(|_| sender)
    }) {
        Ok(sender) => Ok(sender.clone()),
        Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
    }
}

/// Spawn a long-lived MCP subprocess without tying Linux parent-death cleanup
/// to the Tokio worker that happened to request it.
pub async fn spawn_long_lived_mcp_subprocess(
    command: Command,
) -> io::Result<(TokioChildProcess, Option<ChildStderr>)> {
    #[cfg(target_os = "linux")]
    {
        let runtime = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        long_lived_spawn_sender()?
            .send(LongLivedSpawnRequest {
                command,
                runtime,
                response: response_tx,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "extension spawner exited"))?;
        response_rx
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "extension spawner exited"))?
    }

    #[cfg(not(target_os = "linux"))]
    {
        let mut command = command;
        configure_subprocess(&mut command);
        TokioChildProcess::builder(command)
            .stderr(std::process::Stdio::piped())
            .spawn()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_process_fence_config_defaults() {
        let config = ProcessFenceConfig::default();
        assert_eq!(config.mode, ProcessFenceMode::None);
        assert!(config.isolate_process_group);
        assert!(config.parent_death_signal);
        assert!(!config.death_signal_kill);
        assert!(!config.child_subreaper);
        assert!(!config.isolate_namespaces);
        assert!(!config.isolate_network);
        assert!(!config.landlock_enabled);
        assert!(config.custom_runtime.is_none());
    }

    #[test]
    fn test_process_fence_mode_parsing() {
        let mut env = HashMap::new();
        env.insert("GOOSE_PROCESS_FENCE", "strict".to_string());
        let config = ProcessFenceConfig::from_lookup(|k| env.get(k).cloned());
        assert_eq!(config.mode, ProcessFenceMode::Strict);
        assert!(config.death_signal_kill);
        assert!(config.child_subreaper);
        assert!(config.isolate_namespaces);
        assert!(config.landlock_enabled);
        assert!(is_process_fencing_enabled_with(|k| env.get(k).cloned()));

        env.insert("GOOSE_PROCESS_FENCE", "standard".to_string());
        let config = ProcessFenceConfig::from_lookup(|k| env.get(k).cloned());
        assert_eq!(config.mode, ProcessFenceMode::Standard);
        assert!(!config.death_signal_kill);
        assert!(config.child_subreaper);
        assert!(!config.isolate_namespaces);
        assert!(config.landlock_enabled);
        assert!(is_process_fencing_enabled_with(|k| env.get(k).cloned()));

        env.clear();
        let config = ProcessFenceConfig::from_lookup(|k| env.get(k).cloned());
        assert_eq!(config.mode, ProcessFenceMode::None);
        assert!(!config.death_signal_kill);
        assert!(!is_process_fencing_enabled_with(|k| env.get(k).cloned()));
    }

    #[test]
    fn test_process_fence_custom_env_flags() {
        let mut env = HashMap::new();
        env.insert("GOOSE_PROCESS_FENCE", "none".to_string());
        env.insert("GOOSE_FENCE_PDEATHSIG", "kill".to_string());
        env.insert("GOOSE_FENCE_SUBREAPER", "true".to_string());
        env.insert("GOOSE_FENCE_NAMESPACES", "1".to_string());
        env.insert("GOOSE_FENCE_ISOLATE_NET", "1".to_string());
        env.insert("GOOSE_FENCE_LANDLOCK", "true".to_string());
        env.insert("GOOSE_FENCE_RUNTIME", "vetto".to_string());

        let config = ProcessFenceConfig::from_lookup(|k| env.get(k).cloned());
        assert_eq!(config.mode, ProcessFenceMode::None);
        assert!(config.death_signal_kill);
        assert!(config.child_subreaper);
        assert!(config.isolate_namespaces);
        assert!(config.isolate_network);
        assert!(config.landlock_enabled);
        assert_eq!(config.custom_runtime.as_deref(), Some("vetto"));
        assert!(is_process_fencing_enabled_with(|k| env.get(k).cloned()));
    }

    #[test]
    fn test_apply_process_fencing_to_commands() {
        let mut cmd = Command::new("echo");
        cmd.apply_process_fencing();

        let mut std_cmd = std::process::Command::new("echo");
        std_cmd.apply_process_fencing();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_probe_landlock_abi_call() {
        let _abi = probe_landlock_abi();
    }
}
