use crate::config::paths::Paths;
use anyhow::{anyhow, Context, Result};
use fs_err::File;
use goose_providers::request_log::{install_logger, RequestLogHandle, RequestLogger};
use serde_json::Value;
use std::error::Error;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::OnceLock;
use uuid::Uuid;

pub fn filter_extensions_from_system_prompt(system: &str) -> String {
    let Some(extensions_start) = system.find("# Extensions") else {
        return system.to_string();
    };

    let Some(after_extensions) = system.get(extensions_start + 1..) else {
        return system.to_string();
    };

    if let Some(next_section_pos) = after_extensions.find("\n# ") {
        let Some(before) = system.get(..extensions_start) else {
            return system.to_string();
        };
        let Some(after) = system.get(extensions_start + next_section_pos + 1..) else {
            return system.to_string();
        };
        format!("{}{}", before.trim_end(), after)
    } else {
        system
            .get(..extensions_start)
            .map(|s| s.trim_end().to_string())
            .unwrap_or_else(|| system.to_string())
    }
}

/// Extract the model name from a JSON object. Common with most providers to have this top level attribute.
pub fn get_model(data: &Value) -> String {
    if let Some(model) = data.get("model") {
        if let Some(model_str) = model.as_str() {
            model_str.to_string()
        } else {
            "Unknown".to_string()
        }
    } else {
        "Unknown".to_string()
    }
}

pub const LOGS_TO_KEEP: usize = 10;

static INIT_LOGGER: OnceLock<Result<()>> = OnceLock::new();

pub fn init_goose_request_log() -> Result<()> {
    INIT_LOGGER
        .get_or_init(|| Ok(install_logger(RequestLog::new(LOGS_TO_KEEP)?)?))
        .as_ref()
        .map_err(|e| anyhow::anyhow!("failed to set up logger: {}", e))?;
    Ok(())
}

pub struct RequestLog {
    logs_to_keep: usize,
}

impl RequestLog {
    pub fn new(logs_to_keep: usize) -> Result<Self> {
        LogDir::open()?;
        Ok(Self { logs_to_keep })
    }
}

fn request_log_name(index: usize) -> String {
    format!("llm_request.{index}.jsonl")
}

#[cfg(unix)]
struct LogDir {
    path: PathBuf,
    handle: std::fs::File,
}

#[cfg(unix)]
impl LogDir {
    fn open() -> Result<Self> {
        let dir = Self {
            path: Paths::in_state_dir("logs"),
            handle: crate::logging::open_private_log_dir()?,
        };
        dir.tighten_request_logs(crate::logging::current_uid())?;
        Ok(dir)
    }

    /// Narrows request logs left by older versions or interrupted runs to 0600, after
    /// checking that each one is a regular file owned by this user with no other links.
    fn tighten_request_logs(&self, owner: u32) -> Result<()> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        for entry in std::fs::read_dir(&self.path)? {
            let name = entry?.file_name();
            if !name
                .to_str()
                .is_some_and(|name| name.starts_with("llm_request.") && name.ends_with(".jsonl"))
            {
                continue;
            }
            let path = self.path.join(&name);
            let file = match self
                .open_at(&name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
                .or_else(|error| match error.raw_os_error() {
                    Some(libc::EACCES) => self.open_at(&name, libc::O_WRONLY | libc::O_NONBLOCK, 0),
                    _ => Err(error),
                }) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) if error.raw_os_error() == Some(libc::ELOOP) => {
                    anyhow::bail!("Request log {} is a symbolic link", path.display());
                }
                Err(error) if error.raw_os_error() == Some(libc::EACCES) => {
                    self.tighten_unopenable_request_log(&name, &path, owner)?;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let metadata = file.metadata()?;
            check_request_log(
                metadata.is_file(),
                metadata.uid(),
                metadata.nlink() == 1,
                &path,
                owner,
            )?;
            if metadata.permissions().mode() & 0o777 != 0o600 {
                file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            }
        }
        Ok(())
    }

    /// A request log with neither owner read nor write permission cannot be opened,
    /// so it is checked and changed by name, relative to the directory, without
    /// following a symlink.
    fn tighten_unopenable_request_log(
        &self,
        name: &std::ffi::OsStr,
        path: &std::path::Path,
        owner: u32,
    ) -> Result<()> {
        use std::os::fd::AsRawFd;

        let name = c_file_name(name)?;
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::fstatat(
                self.handle.as_raw_fd(),
                name.as_ptr(),
                &mut stat,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        check_request_log(
            (stat.st_mode & libc::S_IFMT) == libc::S_IFREG,
            stat.st_uid,
            stat.st_nlink == 1,
            path,
            owner,
        )?;
        crate::logging::chmod_no_follow(self.handle.as_raw_fd(), &name, 0o600)?;
        Ok(())
    }

    fn create_new(&self, name: &str) -> std::io::Result<std::fs::File> {
        use std::os::unix::fs::PermissionsExt;

        let file = self.open_at(
            name.as_ref(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        // The umask can remove owner bits from the creation mode.
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        Ok(file)
    }

    fn open_at(
        &self,
        name: &std::ffi::OsStr,
        flags: libc::c_int,
        mode: libc::c_uint,
    ) -> std::io::Result<std::fs::File> {
        use std::os::fd::{AsRawFd, FromRawFd};

        let name = c_file_name(name)?;
        let descriptor = unsafe {
            libc::openat(
                self.handle.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                mode,
            )
        };
        if descriptor < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { std::fs::File::from_raw_fd(descriptor) })
    }

    fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
        use std::os::fd::AsRawFd;

        let (from, to) = (c_file_name(from.as_ref())?, c_file_name(to.as_ref())?);
        let fd = self.handle.as_raw_fd();
        if unsafe { libc::renameat(fd, from.as_ptr(), fd, to.as_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn remove(&self, name: &str) -> std::io::Result<()> {
        use std::os::fd::AsRawFd;

        let name = c_file_name(name.as_ref())?;
        if unsafe { libc::unlinkat(self.handle.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(unix)]
fn c_file_name(name: &std::ffi::OsStr) -> std::io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;

    std::ffi::CString::new(name.as_bytes()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "request log name contains a NUL byte",
        )
    })
}

#[cfg(unix)]
fn check_request_log(
    is_file: bool,
    uid: u32,
    single_link: bool,
    path: &std::path::Path,
    owner: u32,
) -> Result<()> {
    if !is_file {
        anyhow::bail!("Request log {} is not a regular file", path.display());
    }
    if uid != owner {
        anyhow::bail!("Request log {} is owned by another user", path.display());
    }
    if !single_link {
        anyhow::bail!("Request log {} has other hard links", path.display());
    }
    Ok(())
}

#[cfg(not(unix))]
struct LogDir {
    path: PathBuf,
}

#[cfg(not(unix))]
impl LogDir {
    fn open() -> Result<Self> {
        let path = Paths::in_state_dir("logs");
        fs_err::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn create_new(&self, name: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.path.join(name))
    }

    fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
        std::fs::rename(self.path.join(from), self.path.join(to))
    }

    fn remove(&self, name: &str) -> std::io::Result<()> {
        std::fs::remove_file(self.path.join(name))
    }
}

struct FileLogHandle {
    writer: Option<BufWriter<File>>,
    dir: LogDir,
    temp_name: String,
    logs_to_keep: usize,
}

impl RequestLogger for RequestLog {
    fn start(&self) -> Result<Box<dyn RequestLogHandle>, Box<dyn Error + Send + Sync>> {
        let dir = LogDir::open()?;
        let temp_name = format!("llm_request.{}.jsonl", Uuid::new_v4());
        let file = dir.create_new(&temp_name)?;
        let writer = BufWriter::new(File::from_parts(file, dir.path.join(&temp_name)));

        Ok(Box::new(FileLogHandle {
            writer: Some(writer),
            dir,
            temp_name,
            logs_to_keep: self.logs_to_keep,
        }))
    }
}

impl RequestLogHandle for FileLogHandle {
    fn write(&mut self, s: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| anyhow!("logger is finished"))?;
        writeln!(writer, "{}", s)?;
        Ok(())
    }
}

impl FileLogHandle {
    fn finish(&mut self) -> Result<()> {
        if let Some(mut writer) = self.writer.take() {
            writer.flush()?;
            let temp_path = self.dir.path.join(&self.temp_name);

            if self.logs_to_keep == 0 {
                self.dir
                    .remove(&self.temp_name)
                    .with_context(|| format!("Failed to remove {}", temp_path.display()))?;
                return Ok(());
            }

            for i in (0..self.logs_to_keep.saturating_sub(1)).rev() {
                let _ = self
                    .dir
                    .rename(&request_log_name(i), &request_log_name(i + 1));
            }

            self.dir
                .rename(&self.temp_name, &request_log_name(0))
                .with_context(|| format!("Failed to rotate {}", temp_path.display()))?;
        }
        Ok(())
    }
}

impl Drop for FileLogHandle {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let _ = self.finish();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn request_log_owned_by_another_user_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(request_log_name(0));
        std::fs::write(&path, "request").unwrap();
        let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(&path).unwrap());
        let owner = crate::logging::current_uid();

        assert!(check_request_log(true, uid, true, &path, owner).is_ok());
        let error = check_request_log(true, uid, true, &path, owner + 1).unwrap_err();
        assert!(error.to_string().contains("owned by another user"));
    }
}
