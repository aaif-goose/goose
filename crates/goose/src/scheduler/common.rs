use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::paths::Paths;

pub(crate) const MAX_SCHEDULE_RECIPE_BYTES: u64 = 1024 * 1024;

#[cfg_attr(not(feature = "scheduler"), allow(dead_code))]
pub struct ValidatedScheduleRecipe {
    pub(super) bytes: Vec<u8>,
    pub(super) source: PathBuf,
}

#[cfg_attr(not(feature = "scheduler"), allow(dead_code))]
impl ValidatedScheduleRecipe {
    pub(crate) fn new(bytes: Vec<u8>, source: PathBuf) -> Self {
        Self { bytes, source }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg_attr(not(feature = "scheduler"), allow(dead_code))]
pub(crate) fn open_regular_schedule_recipe(path: &Path) -> io::Result<File> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Recipe path must reference a regular file",
        ));
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Recipe path must reference a regular file",
        ));
    }
    Ok(file)
}

#[cfg_attr(not(feature = "scheduler"), allow(dead_code))]
pub(super) fn copy_bounded_schedule_recipe(
    source: &Path,
    destination: &Path,
) -> Result<(), SchedulerError> {
    let source = open_regular_schedule_recipe(source).map_err(|error| {
        SchedulerError::RecipeLoadError(format!("Cannot read recipe file: {error}"))
    })?;
    let metadata = source.metadata().map_err(|error| {
        SchedulerError::RecipeLoadError(format!("Cannot inspect recipe file: {error}"))
    })?;
    if !metadata.is_file() {
        return Err(SchedulerError::RecipeLoadError(
            "Recipe path must reference a regular file".to_string(),
        ));
    }
    if metadata.len() > MAX_SCHEDULE_RECIPE_BYTES {
        return Err(SchedulerError::RecipeLoadError(format!(
            "Recipe file exceeds the {MAX_SCHEDULE_RECIPE_BYTES} byte limit"
        )));
    }

    let mut bytes = Vec::new();
    source
        .take(MAX_SCHEDULE_RECIPE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            SchedulerError::RecipeLoadError(format!("Cannot read recipe file: {error}"))
        })?;
    if bytes.len() as u64 > MAX_SCHEDULE_RECIPE_BYTES {
        return Err(SchedulerError::RecipeLoadError(format!(
            "Recipe file exceeds the {MAX_SCHEDULE_RECIPE_BYTES} byte limit"
        )));
    }

    write_schedule_recipe_bytes(destination, &bytes)
}

#[cfg_attr(not(feature = "scheduler"), allow(dead_code))]
pub(super) fn write_schedule_recipe_bytes(
    destination: &Path,
    bytes: &[u8],
) -> Result<(), SchedulerError> {
    if bytes.len() as u64 > MAX_SCHEDULE_RECIPE_BYTES {
        return Err(SchedulerError::RecipeLoadError(format!(
            "Recipe file exceeds the {MAX_SCHEDULE_RECIPE_BYTES} byte limit"
        )));
    }

    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut file = options.open(destination)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.set_len(0)?;
        file.write_all(bytes)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(destination);
        return Err(SchedulerError::StorageError(error));
    }

    Ok(())
}

pub fn get_default_scheduler_storage_path() -> Result<PathBuf, io::Error> {
    let data_dir = Paths::data_dir();
    fs::create_dir_all(&data_dir)?;
    Ok(data_dir.join("schedule.json"))
}

pub fn get_default_scheduled_recipes_dir() -> Result<PathBuf, SchedulerError> {
    let data_dir = Paths::data_dir();
    let recipes_dir = data_dir.join("scheduled_recipes");
    fs::create_dir_all(&recipes_dir).map_err(SchedulerError::StorageError)?;
    Ok(recipes_dir)
}

#[derive(Debug)]
pub enum SchedulerError {
    JobIdExists(String),
    JobNotFound(String),
    StorageError(io::Error),
    RecipeLoadError(String),
    AgentSetupError(String),
    PersistError(String),
    CronParseError(String),
    SchedulerInternalError(String),
    AnyhowError(anyhow::Error),
}

impl std::fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchedulerError::JobIdExists(id) => write!(f, "Job ID '{}' already exists.", id),
            SchedulerError::JobNotFound(id) => write!(f, "Job ID '{}' not found.", id),
            SchedulerError::StorageError(e) => write!(f, "Storage error: {}", e),
            SchedulerError::RecipeLoadError(e) => write!(f, "Recipe load error: {}", e),
            SchedulerError::AgentSetupError(e) => write!(f, "Agent setup error: {}", e),
            SchedulerError::PersistError(e) => write!(f, "Failed to persist schedules: {}", e),
            SchedulerError::CronParseError(e) => write!(f, "Invalid cron string: {}", e),
            SchedulerError::SchedulerInternalError(e) => {
                write!(f, "Scheduler internal error: {}", e)
            }
            SchedulerError::AnyhowError(e) => write!(f, "Scheduler operation failed: {}", e),
        }
    }
}

impl std::error::Error for SchedulerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SchedulerError::StorageError(e) => Some(e),
            SchedulerError::AnyhowError(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<io::Error> for SchedulerError {
    fn from(err: io::Error) -> Self {
        SchedulerError::StorageError(err)
    }
}

impl From<serde_json::Error> for SchedulerError {
    fn from(err: serde_json::Error) -> Self {
        SchedulerError::PersistError(err.to_string())
    }
}

impl From<anyhow::Error> for SchedulerError {
    fn from(err: anyhow::Error) -> Self {
        SchedulerError::AnyhowError(err)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ScheduledJob {
    pub id: String,
    pub source: String,
    pub cron: String,
    pub last_run: Option<DateTime<Utc>>,
    #[serde(default)]
    pub currently_running: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub current_session_id: Option<String>,
    #[serde(default)]
    pub process_start_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub parameters: Vec<(String, String)>,
    /// Original directory of the recipe file before it was copied to scheduled_recipes/.
    /// Preserved so that relative paths (sub-recipes, template includes) resolve correctly
    /// against the source tree rather than the scheduler's internal storage directory.
    #[serde(default)]
    pub recipe_base_dir: Option<String>,
}
