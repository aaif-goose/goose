use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_cron_scheduler::{job::JobId, Job, JobScheduler as TokioJobScheduler};
use tokio_util::sync::CancellationToken;

use crate::agents::AgentEvent;
use crate::agents::{Agent, SessionConfig};
use crate::config::paths::Paths;
use crate::config::{resolve_extensions_for_new_session, Config};
use crate::conversation::message::Message;
use crate::conversation::Conversation;
#[cfg(feature = "telemetry")]
use crate::posthog;
use crate::providers::create;
use crate::recipe::Recipe;
use crate::scheduler_trait::SchedulerTrait;
use crate::session::session_manager::SessionType;
use crate::session::{Session, SessionManager};

type RunningTasksMap = HashMap<String, CancellationToken>;
type JobsMap = HashMap<String, (JobId, ScheduledJob)>;

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

#[derive(Clone, Serialize, Deserialize, Debug, utoipa::ToSchema)]
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
}

async fn persist_jobs(
    storage_path: &Path,
    jobs: &Arc<Mutex<JobsMap>>,
) -> Result<(), SchedulerError> {
    let jobs_guard = jobs.lock().await;
    let list: Vec<ScheduledJob> = jobs_guard.values().map(|(_, j)| j.clone()).collect();
    if let Some(parent) = storage_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(&list)?;
    fs::write(storage_path, data)?;
    Ok(())
}

pub struct Scheduler {
    tokio_scheduler: TokioJobScheduler,
    jobs: Arc<Mutex<JobsMap>>,
    storage_path: PathBuf,
    running_tasks: Arc<Mutex<RunningTasksMap>>,
    session_manager: Arc<SessionManager>,
}

impl Scheduler {
    pub async fn new(
        storage_path: PathBuf,
        session_manager: Arc<SessionManager>,
    ) -> Result<Arc<Self>, SchedulerError> {
        let internal_scheduler = TokioJobScheduler::new()
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        let jobs = Arc::new(Mutex::new(HashMap::new()));
        let running_tasks = Arc::new(Mutex::new(HashMap::new()));

        let arc_self = Arc::new(Self {
            tokio_scheduler: internal_scheduler,
            jobs,
            storage_path,
            running_tasks,
            session_manager,
        });

        arc_self.load_jobs_from_storage().await;
        arc_self
            .tokio_scheduler
            .start()
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        Ok(arc_self)
    }

    fn create_cron_task(&self, job: ScheduledJob) -> Result<Job, SchedulerError> {
        let job_for_task = job.clone();
        let jobs_arc = self.jobs.clone();
        let storage_path = self.storage_path.clone();
        let running_tasks_arc = self.running_tasks.clone();

        let cron_parts: Vec<&str> = job.cron.split_whitespace().collect();
        let cron = match cron_parts.len() {
            5 => {
                tracing::warn!(
                    "Job '{}' has legacy 5-field cron '{}', converting to 6-field",
                    job.id,
                    job.cron
                );
                format!("0 {}", job.cron)
            }
            6 => job.cron.clone(),
            _ => {
                return Err(SchedulerError::CronParseError(format!(
                    "Invalid cron expression '{}': expected 5 or 6 fields, got {}",
                    job.cron,
                    cron_parts.len()
                )))
            }
        };

        let local_tz = Local::now().timezone();

        Job::new_async_tz(&cron, local_tz, move |_uuid, _l| {
            tracing::info!("Cron task triggered for job '{}'", job_for_task.id);
            let task_job_id = job_for_task.id.clone();
            let current_jobs_arc = jobs_arc.clone();
            let local_storage_path = storage_path.clone();
            let job_to_execute = job_for_task.clone();
            let running_tasks = running_tasks_arc.clone();

            Box::pin(async move {
                let should_execute = {
                    let jobs_guard = current_jobs_arc.lock().await;
                    jobs_guard
                        .get(&task_job_id)
                        .map(|(_, j)| !j.paused)
                        .unwrap_or(false)
                };

                if !should_execute {
                    return;
                }

                let current_time = Utc::now();
                {
                    let mut jobs_guard = current_jobs_arc.lock().await;
                    if let Some((_, job)) = jobs_guard.get_mut(&task_job_id) {
                        job.last_run = Some(current_time);
                        job.currently_running = true;
                        job.process_start_time = Some(current_time);
                    }
                }

                if let Err(e) = persist_jobs(&local_storage_path, &current_jobs_arc).await {
                    tracing::error!("Failed to persist job status: {}", e);
                }

                let cancel_token = CancellationToken::new();
                {
                    let mut tasks = running_tasks.lock().await;
                    tasks.insert(task_job_id.clone(), cancel_token.clone());
                }

                let result = execute_job(
                    job_to_execute,
                    current_jobs_arc.clone(),
                    task_job_id.clone(),
                    cancel_token.clone(),
                )
                .await;

                {
                    let mut tasks = running_tasks.lock().await;
                    tasks.remove(&task_job_id);
                }

                {
                    let mut jobs_guard = current_jobs_arc.lock().await;
                    if let Some((_, job)) = jobs_guard.get_mut(&task_job_id) {
                        job.currently_running = false;
                        job.current_session_id = None;
                        job.process_start_time = None;
                    }
                }

                if let Err(e) = persist_jobs(&local_storage_path, &current_jobs_arc).await {
                    tracing::error!("Failed to persist job completion: {}", e);
                }

                match result {
                    Ok(_) => tracing::info!("Job '{}' completed", task_job_id),
                    Err(ref e) => {
                        tracing::error!("Job '{}' failed: {}", task_job_id, e);
                        #[cfg(feature = "telemetry")]
                        crate::posthog::emit_error("scheduler_job_failed", &e.to_string());
                    }
                }
            })
        })
        .map_err(|e| SchedulerError::CronParseError(e.to_string()))
    }

    pub async fn add_scheduled_job(
        &self,
        original_job_spec: ScheduledJob,
        make_copy: bool,
    ) -> Result<(), SchedulerError> {
        {
            let jobs_guard = self.jobs.lock().await;
            if jobs_guard.contains_key(&original_job_spec.id) {
                return Err(SchedulerError::JobIdExists(original_job_spec.id.clone()));
            }
        }

        let mut stored_job = original_job_spec;
        if make_copy {
            let original_recipe_path = Path::new(&stored_job.source);
            if !original_recipe_path.is_file() {
                return Err(SchedulerError::RecipeLoadError(format!(
                    "Recipe file not found: {}",
                    stored_job.source
                )));
            }

            let scheduled_recipes_dir = get_default_scheduled_recipes_dir()?;
            let original_extension = original_recipe_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("yaml");

            let destination_filename = format!("{}.{}", stored_job.id, original_extension);
            let destination_recipe_path = scheduled_recipes_dir.join(destination_filename);

            fs::copy(original_recipe_path, &destination_recipe_path)?;
            stored_job.source = destination_recipe_path.to_string_lossy().into_owned();
            stored_job.current_session_id = None;
            stored_job.process_start_time = None;
        }

        let cron_task = self.create_cron_task(stored_job.clone())?;

        let job_uuid = self
            .tokio_scheduler
            .add(cron_task)
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        {
            let mut jobs_guard = self.jobs.lock().await;
            jobs_guard.insert(stored_job.id.clone(), (job_uuid, stored_job));
        }

        persist_jobs(&self.storage_path, &self.jobs).await?;
        Ok(())
    }

    pub async fn schedule_recipe(
        &self,
        recipe_path: PathBuf,
        cron_schedule: Option<String>,
    ) -> Result<(), SchedulerError> {
        let recipe_path_str = recipe_path.to_string_lossy().to_string();

        let existing_job_id = {
            let jobs_guard = self.jobs.lock().await;
            jobs_guard
                .iter()
                .find(|(_, (_, job))| job.source == recipe_path_str)
                .map(|(id, _)| id.clone())
        };

        match cron_schedule {
            Some(cron) => {
                if let Some(job_id) = existing_job_id {
                    self.update_schedule(&job_id, cron).await
                } else {
                    let job_id = self.generate_unique_job_id(&recipe_path).await;
                    let job = ScheduledJob {
                        id: job_id,
                        source: recipe_path_str,
                        cron,
                        last_run: None,
                        currently_running: false,
                        paused: false,
                        current_session_id: None,
                        process_start_time: None,
                    };
                    self.add_scheduled_job(job, false).await
                }
            }
            None => {
                if let Some(job_id) = existing_job_id {
                    self.remove_scheduled_job(&job_id, false).await
                } else {
                    Ok(())
                }
            }
        }
    }

    async fn generate_unique_job_id(&self, path: &Path) -> String {
        let base_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let jobs_guard = self.jobs.lock().await;
        let mut id = base_id.clone();
        let mut counter = 1;

        while jobs_guard.contains_key(&id) {
            id = format!("{}_{}", base_id, counter);
            counter += 1;
        }

        id
    }

    async fn load_jobs_from_storage(self: &Arc<Self>) {
        if !self.storage_path.exists() {
            return;
        }
        let data = match fs::read_to_string(&self.storage_path) {
            Ok(data) => data,
            Err(e) => {
                tracing::error!(
                    "Failed to read {}: {}. Starting with empty schedule list.",
                    self.storage_path.display(),
                    e
                );
                return;
            }
        };
        if data.trim().is_empty() {
            return;
        }

        let list: Vec<ScheduledJob> = match serde_json::from_str(&data) {
            Ok(jobs) => jobs,
            Err(e) => {
                tracing::error!(
                    "Failed to parse {}: {}. Starting with empty schedule list.",
                    self.storage_path.display(),
                    e
                );
                return;
            }
        };

        for job_to_load in list {
            if !Path::new(&job_to_load.source).exists() {
                tracing::warn!(
                    "Recipe file {} not found, skipping job '{}'",
                    job_to_load.source,
                    job_to_load.id
                );
                continue;
            }

            let cron_task = match self.create_cron_task(job_to_load.clone()) {
                Ok(task) => task,
                Err(e) => {
                    tracing::error!(
                        "Failed to create cron task for job '{}': {}. Skipping.",
                        job_to_load.id,
                        e
                    );
                    continue;
                }
            };

            let job_uuid = match self.tokio_scheduler.add(cron_task).await {
                Ok(uuid) => uuid,
                Err(e) => {
                    tracing::error!(
                        "Failed to add job '{}' to scheduler: {}. Skipping.",
                        job_to_load.id,
                        e
                    );
                    continue;
                }
            };

            let mut jobs_guard = self.jobs.lock().await;
            jobs_guard.insert(job_to_load.id.clone(), (job_uuid, job_to_load));
        }
    }

    async fn sync_from_storage(&self) {
        if !self.storage_path.exists() {
            return;
        }
        let data = match fs::read_to_string(&self.storage_path) {
            Ok(d) => d,
            Err(_) => return,
        };
        if data.trim().is_empty() {
            return;
        }
        let disk_jobs: Vec<ScheduledJob> = match serde_json::from_str(&data) {
            Ok(jobs) => jobs,
            Err(_) => return,
        };

        let disk_ids: std::collections::HashSet<String> =
            disk_jobs.iter().map(|j| j.id.clone()).collect();

        let (jobs_to_add, jobs_to_remove): (Vec<ScheduledJob>, Vec<(String, JobId)>) = {
            let jobs_guard = self.jobs.lock().await;
            let to_add = disk_jobs
                .into_iter()
                .filter(|j| !jobs_guard.contains_key(&j.id))
                .collect();
            let to_remove = jobs_guard
                .iter()
                .filter(|(id, (_, j))| !disk_ids.contains(*id) && !j.currently_running)
                .map(|(id, (uuid, _))| (id.clone(), *uuid))
                .collect();
            (to_add, to_remove)
        };

        for job in jobs_to_add {
            if !Path::new(&job.source).exists() {
                tracing::warn!(
                    "Skipping sync of job '{}': recipe file not found at {}",
                    job.id,
                    job.source
                );
                continue;
            }
            let cron_task = match self.create_cron_task(job.clone()) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(
                        "Failed to create cron task for '{}' during sync: {}",
                        job.id,
                        e
                    );
                    continue;
                }
            };
            let uuid = match self.tokio_scheduler.add(cron_task).await {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!("Failed to register job '{}' during sync: {}", job.id, e);
                    continue;
                }
            };
            self.jobs.lock().await.insert(job.id.clone(), (uuid, job));
        }

        for (id, uuid) in jobs_to_remove {
            let _ = self.tokio_scheduler.remove(&uuid).await;
            self.jobs.lock().await.remove(&id);
        }
    }

    pub async fn list_scheduled_jobs(&self) -> Vec<ScheduledJob> {
        self.sync_from_storage().await;
        self.jobs
            .lock()
            .await
            .values()
            .map(|(_, j)| j.clone())
            .collect()
    }

    pub async fn remove_scheduled_job(
        &self,
        id: &str,
        remove_recipe: bool,
    ) -> Result<(), SchedulerError> {
        let (job_uuid, recipe_path) = {
            let mut jobs_guard = self.jobs.lock().await;
            match jobs_guard.remove(id) {
                Some((uuid, job)) => (uuid, job.source.clone()),
                None => return Err(SchedulerError::JobNotFound(id.to_string())),
            }
        };

        self.tokio_scheduler
            .remove(&job_uuid)
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        if remove_recipe {
            let path = Path::new(&recipe_path);
            if path.exists() {
                fs::remove_file(path)?;
            }
        }

        persist_jobs(&self.storage_path, &self.jobs).await?;
        Ok(())
    }

    pub async fn sessions(
        &self,
        sched_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, Session)>, SchedulerError> {
        let all_sessions = self
            .session_manager
            .list_sessions()
            .await
            .map_err(|e| SchedulerError::StorageError(io::Error::other(e)))?;

        let mut schedule_sessions: Vec<(String, Session)> = all_sessions
            .into_iter()
            .filter(|s| s.schedule_id.as_deref() == Some(sched_id))
            .map(|s| (s.id.clone(), s))
            .collect();

        schedule_sessions.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));
        schedule_sessions.truncate(limit);

        Ok(schedule_sessions)
    }

    pub async fn run_now(&self, sched_id: &str) -> Result<String, SchedulerError> {
        let job_to_run = {
            let mut jobs_guard = self.jobs.lock().await;
            match jobs_guard.get_mut(sched_id) {
                Some((_, job)) => {
                    if job.currently_running {
                        return Err(SchedulerError::AnyhowError(anyhow!(
                            "Job '{}' is already running",
                            sched_id
                        )));
                    }
                    job.currently_running = true;
                    job.process_start_time = Some(Utc::now());
                    job.clone()
                }
                None => return Err(SchedulerError::JobNotFound(sched_id.to_string())),
            }
        };

        persist_jobs(&self.storage_path, &self.jobs).await?;

        let cancel_token = CancellationToken::new();
        {
            let mut tasks = self.running_tasks.lock().await;
            tasks.insert(sched_id.to_string(), cancel_token.clone());
        }

        let result = execute_job(
            job_to_run,
            self.jobs.clone(),
            sched_id.to_string(),
            cancel_token.clone(),
        )
        .await;

        {
            let mut tasks = self.running_tasks.lock().await;
            tasks.remove(sched_id);
        }

        {
            let mut jobs_guard = self.jobs.lock().await;
            if let Some((_, job)) = jobs_guard.get_mut(sched_id) {
                job.currently_running = false;
                job.current_session_id = None;
                job.process_start_time = None;
                job.last_run = Some(Utc::now());
            }
        }

        persist_jobs(&self.storage_path, &self.jobs).await?;

        match result {
            Ok(session_id) => Ok(session_id),
            Err(e) => Err(SchedulerError::AnyhowError(anyhow!(
                "Job '{}' failed: {}",
                sched_id,
                e
            ))),
        }
    }

    pub async fn pause_schedule(&self, sched_id: &str) -> Result<(), SchedulerError> {
        {
            let mut jobs_guard = self.jobs.lock().await;
            match jobs_guard.get_mut(sched_id) {
                Some((_, job)) => {
                    if job.currently_running {
                        return Err(SchedulerError::AnyhowError(anyhow!(
                            "Cannot pause running schedule '{}'",
                            sched_id
                        )));
                    }
                    job.paused = true;
                }
                None => return Err(SchedulerError::JobNotFound(sched_id.to_string())),
            }
        }

        persist_jobs(&self.storage_path, &self.jobs).await
    }

    pub async fn unpause_schedule(&self, sched_id: &str) -> Result<(), SchedulerError> {
        {
            let mut jobs_guard = self.jobs.lock().await;
            match jobs_guard.get_mut(sched_id) {
                Some((_, job)) => job.paused = false,
                None => return Err(SchedulerError::JobNotFound(sched_id.to_string())),
            }
        }

        persist_jobs(&self.storage_path, &self.jobs).await
    }

    pub async fn update_schedule(
        &self,
        sched_id: &str,
        new_cron: String,
    ) -> Result<(), SchedulerError> {
        let (old_uuid, updated_job) = {
            let mut jobs_guard = self.jobs.lock().await;
            match jobs_guard.get_mut(sched_id) {
                Some((uuid, job)) => {
                    if job.currently_running {
                        return Err(SchedulerError::AnyhowError(anyhow!(
                            "Cannot update running schedule '{}'",
                            sched_id
                        )));
                    }
                    if new_cron == job.cron {
                        return Ok(());
                    }
                    job.cron = new_cron.clone();
                    (*uuid, job.clone())
                }
                None => return Err(SchedulerError::JobNotFound(sched_id.to_string())),
            }
        };

        self.tokio_scheduler
            .remove(&old_uuid)
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        let cron_task = self.create_cron_task(updated_job)?;
        let new_uuid = self
            .tokio_scheduler
            .add(cron_task)
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        {
            let mut jobs_guard = self.jobs.lock().await;
            if let Some((uuid, _)) = jobs_guard.get_mut(sched_id) {
                *uuid = new_uuid;
            }
        }

        persist_jobs(&self.storage_path, &self.jobs).await
    }

    pub async fn kill_running_job(&self, sched_id: &str) -> Result<(), SchedulerError> {
        {
            let jobs_guard = self.jobs.lock().await;
            match jobs_guard.get(sched_id) {
                Some((_, job)) if !job.currently_running => {
                    return Err(SchedulerError::AnyhowError(anyhow!(
                        "Schedule '{}' is not running",
                        sched_id
                    )));
                }
                None => return Err(SchedulerError::JobNotFound(sched_id.to_string())),
                _ => {}
            }
        }

        {
            let tasks = self.running_tasks.lock().await;
            if let Some(token) = tasks.get(sched_id) {
                token.cancel();
            }
        }

        Ok(())
    }

    pub async fn get_running_job_info(
        &self,
        sched_id: &str,
    ) -> Result<Option<(String, DateTime<Utc>)>, SchedulerError> {
        let jobs_guard = self.jobs.lock().await;
        match jobs_guard.get(sched_id) {
            Some((_, job)) if job.currently_running => {
                match (&job.current_session_id, &job.process_start_time) {
                    (Some(sid), Some(start)) => Ok(Some((sid.clone(), *start))),
                    _ => Ok(None),
                }
            }
            Some(_) => Ok(None),
            None => Err(SchedulerError::JobNotFound(sched_id.to_string())),
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn execute_job(
    job: ScheduledJob,
    jobs: Arc<Mutex<JobsMap>>,
    job_id: String,
    cancel_token: CancellationToken,
) -> Result<String> {
    if job.source.is_empty() {
        return Ok(job.id.to_string());
    }

    let recipe_path = Path::new(&job.source);
    let recipe_content = fs::read_to_string(recipe_path)?;

    let recipe: Recipe = {
        let extension = recipe_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("yaml")
            .to_lowercase();

        match extension.as_str() {
            "json" | "jsonl" => serde_json::from_str(&recipe_content)?,
            _ => serde_yaml::from_str(&recipe_content)?,
        }
    };

    let agent = Agent::new();

    let config = Config::global();
    let provider_name = config.get_goose_provider()?;
    let model_name = config.get_goose_model()?;
    let model_config =
        crate::model::ModelConfig::new(&model_name)?.with_canonical_limits(&provider_name);

    let session = agent
        .config
        .session_manager
        .create_session(
            std::env::current_dir()?,
            format!("Scheduled job: {}", job.id),
            SessionType::Scheduled,
            agent.config.goose_mode,
        )
        .await?;

    let extensions = resolve_extensions_for_new_session(recipe.extensions.as_deref(), None);
    for ext in &extensions {
        agent.add_extension(ext.clone(), &session.id).await?;
    }

    let agent_provider = create(&provider_name, model_config, extensions).await?;
    agent.update_provider(agent_provider, &session.id).await?;

    let mut jobs_guard = jobs.lock().await;
    if let Some((_, job_def)) = jobs_guard.get_mut(job_id.as_str()) {
        job_def.current_session_id = Some(session.id.clone());
    }
    drop(jobs_guard);

    let start_time = std::time::Instant::now();

    let recipe_display_name = recipe_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&job.id);
    let recipe_version = recipe.version.clone();

    tracing::info!(
        monotonic_counter.goose.session_starts = 1,
        session_type = "schedule",
        interface = "scheduler",
        interactive = false,
        "Scheduled session started"
    );

    tracing::info!(
        monotonic_counter.goose.recipe_runs = 1,
        recipe_name = %recipe_display_name,
        recipe_version = %recipe_version,
        session_type = "schedule",
        interface = "scheduler",
        "Recipe execution started"
    );

    #[cfg(feature = "telemetry")]
    tokio::spawn(async move {
        let mut props = HashMap::new();
        props.insert(
            "trigger".to_string(),
            serde_json::Value::String("automated".to_string()),
        );
        if let Err(e) = posthog::emit_event("schedule_job_started", props).await {
            tracing::debug!("Failed to send schedule telemetry: {}", e);
        }
    });

    let prompt_text = recipe
        .prompt
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            recipe
                .instructions
                .as_deref()
                .filter(|s| !s.trim().is_empty())
        })
        .ok_or_else(|| {
            anyhow!("Recipe must specify at least one of `instructions` or `prompt`.")
        })?;

    let user_message = Message::user().with_text(prompt_text);
    let mut conversation = Conversation::new_unvalidated(vec![user_message.clone()]);

    let session_config = SessionConfig {
        id: session.id.clone(),
        schedule_id: Some(job.id.clone()),
        max_turns: None,
        retry_config: None,
    };

    let stream = agent
        .reply(user_message, session_config, Some(cancel_token))
        .await?;

    use futures::StreamExt;
    let mut stream = std::pin::pin!(stream);

    let mut stream_error = false;
    while let Some(message_result) = stream.next().await {
        tokio::task::yield_now().await;

        match message_result {
            Ok(AgentEvent::Message(msg)) => {
                conversation.push(msg);
            }
            Ok(AgentEvent::HistoryReplaced(updated)) => {
                conversation = updated;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Error in agent stream: {}", e);
                stream_error = true;
                break;
            }
        }
    }

    agent
        .config
        .session_manager
        .update(&session.id)
        .schedule_id(Some(job.id.clone()))
        .recipe(Some(recipe))
        .apply()
        .await?;

    {
        let session_duration = start_time.elapsed();
        let exit_type = if stream_error { "error" } else { "normal" };
        let (total_tokens, message_count) = agent
            .config
            .session_manager
            .get_session(&session.id, false)
            .await
            .map(|s| (s.total_tokens.unwrap_or(0), s.message_count))
            .unwrap_or((0, 0));

        tracing::info!(
            monotonic_counter.goose.session_completions = 1,
            session_type = "schedule",
            interface = "scheduler",
            exit_type,
            duration_ms = session_duration.as_millis() as u64,
            total_tokens,
            message_count,
            "Session completed"
        );

        tracing::info!(
            monotonic_counter.goose.session_duration_ms = session_duration.as_millis() as u64,
            session_type = "schedule",
            interface = "scheduler",
            "Session duration"
        );

        if total_tokens > 0 {
            tracing::info!(
                monotonic_counter.goose.session_tokens = total_tokens,
                session_type = "schedule",
                interface = "scheduler",
                "Session tokens"
            );
        }
    }

    #[cfg(feature = "telemetry")]
    {
        let duration_secs = start_time.elapsed().as_secs();
        tokio::spawn(async move {
            let mut props = HashMap::new();
            props.insert(
                "trigger".to_string(),
                serde_json::Value::String("automated".to_string()),
            );
            props.insert(
                "status".to_string(),
                serde_json::Value::String("completed".to_string()),
            );
            props.insert(
                "duration_seconds".to_string(),
                serde_json::Value::Number(serde_json::Number::from(duration_secs)),
            );
            if let Err(e) = posthog::emit_event("schedule_job_completed", props).await {
                tracing::debug!("Failed to send schedule telemetry: {}", e);
            }
        });
    }

    Ok(session.id)
}

#[async_trait]
impl SchedulerTrait for Scheduler {
    async fn add_scheduled_job(
        &self,
        job: ScheduledJob,
        make_copy: bool,
    ) -> Result<(), SchedulerError> {
        self.add_scheduled_job(job, make_copy).await
    }

    async fn schedule_recipe(
        &self,
        recipe_path: PathBuf,
        cron_schedule: Option<String>,
    ) -> Result<(), SchedulerError> {
        self.schedule_recipe(recipe_path, cron_schedule).await
    }

    async fn list_scheduled_jobs(&self) -> Vec<ScheduledJob> {
        self.list_scheduled_jobs().await
    }

    async fn remove_scheduled_job(
        &self,
        id: &str,
        remove_recipe: bool,
    ) -> Result<(), SchedulerError> {
        self.remove_scheduled_job(id, remove_recipe).await
    }

    async fn pause_schedule(&self, id: &str) -> Result<(), SchedulerError> {
        self.pause_schedule(id).await
    }

    async fn unpause_schedule(&self, id: &str) -> Result<(), SchedulerError> {
        self.unpause_schedule(id).await
    }

    async fn run_now(&self, id: &str) -> Result<String, SchedulerError> {
        self.run_now(id).await
    }

    async fn sessions(
        &self,
        sched_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, Session)>, SchedulerError> {
        self.sessions(sched_id, limit).await
    }

    async fn update_schedule(
        &self,
        sched_id: &str,
        new_cron: String,
    ) -> Result<(), SchedulerError> {
        self.update_schedule(sched_id, new_cron).await
    }

    async fn kill_running_job(&self, sched_id: &str) -> Result<(), SchedulerError> {
        self.kill_running_job(sched_id).await
    }

    async fn get_running_job_info(
        &self,
        sched_id: &str,
    ) -> Result<Option<(String, DateTime<Utc>)>, SchedulerError> {
        self.get_running_job_info(sched_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::time::{sleep, Duration};

    fn create_test_recipe(dir: &Path, name: &str) -> PathBuf {
        let recipe_path = dir.join(format!("{}.yaml", name));
        fs::write(&recipe_path, "prompt: test\n").unwrap();
        recipe_path
    }

    #[tokio::test]
    async fn test_job_runs_on_schedule() {
        let _guard = env_lock::lock_env([
            ("GOOSE_PROVIDER", Some("openai")),
            ("GOOSE_MODEL", Some("gpt-4o")),
            ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
            ("OPENAI_CUSTOM_HEADERS", Some("")),
        ]);
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("schedule.json");
        let recipe_path = create_test_recipe(temp_dir.path(), "scheduled_job");
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let scheduler = Scheduler::new(storage_path, session_manager).await.unwrap();

        let job = ScheduledJob {
            id: "scheduled_job".to_string(),
            source: recipe_path.to_string_lossy().to_string(),
            cron: "* * * * * *".to_string(),
            last_run: None,
            currently_running: false,
            paused: false,
            current_session_id: None,
            process_start_time: None,
        };

        scheduler.add_scheduled_job(job, true).await.unwrap();
        sleep(Duration::from_millis(1500)).await;

        let jobs = scheduler.list_scheduled_jobs().await;
        assert!(jobs[0].last_run.is_some(), "Job should have run");
    }

    #[tokio::test]
    async fn test_paused_job_does_not_run() {
        let _guard = env_lock::lock_env([
            ("GOOSE_PROVIDER", Some("openai")),
            ("GOOSE_MODEL", Some("gpt-4o")),
            ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
            ("OPENAI_CUSTOM_HEADERS", Some("")),
        ]);
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("schedule.json");
        let recipe_path = create_test_recipe(temp_dir.path(), "paused_job");
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let scheduler = Scheduler::new(storage_path, session_manager).await.unwrap();

        let job = ScheduledJob {
            id: "paused_job".to_string(),
            source: recipe_path.to_string_lossy().to_string(),
            cron: "* * * * * *".to_string(),
            last_run: None,
            currently_running: false,
            paused: false,
            current_session_id: None,
            process_start_time: None,
        };

        scheduler.add_scheduled_job(job, true).await.unwrap();
        scheduler.pause_schedule("paused_job").await.unwrap();
        sleep(Duration::from_millis(1500)).await;

        let jobs = scheduler.list_scheduled_jobs().await;
        assert!(jobs[0].last_run.is_none(), "Paused job should not run");
    }

    #[tokio::test]
    async fn test_remove_scheduled_job_respects_recipe_removal_flag() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("schedule.json");
        let recipe_path = create_test_recipe(temp_dir.path(), "recipe_removal_flag_job");
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let scheduler = Scheduler::new(storage_path, session_manager).await.unwrap();

        let job = ScheduledJob {
            id: "recipe_removal_flag_job".to_string(),
            source: recipe_path.to_string_lossy().to_string(),
            cron: "0 0 0 1 1 *".to_string(),
            last_run: None,
            currently_running: false,
            paused: false,
            current_session_id: None,
            process_start_time: None,
        };

        scheduler
            .add_scheduled_job(job.clone(), false)
            .await
            .unwrap();
        scheduler
            .remove_scheduled_job("recipe_removal_flag_job", false)
            .await
            .unwrap();
        assert!(
            recipe_path.exists(),
            "Recipe should be kept when remove_recipe is false"
        );

        scheduler.add_scheduled_job(job, false).await.unwrap();
        scheduler
            .remove_scheduled_job("recipe_removal_flag_job", true)
            .await
            .unwrap();
        assert!(
            !recipe_path.exists(),
            "Recipe should be deleted when remove_recipe is true"
        );
    }

    #[tokio::test]
    async fn test_job_with_no_prompt_does_not_panic() {
        let _guard = env_lock::lock_env([
            ("GOOSE_PROVIDER", Some("openai")),
            ("GOOSE_MODEL", Some("gpt-4o")),
            ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
            ("OPENAI_CUSTOM_HEADERS", Some("")),
        ]);
        let temp_dir = tempdir().unwrap();
        let recipe_path = temp_dir.path().join("no_prompt.yaml");
        fs::write(
            &recipe_path,
            "title: missing\ndescription: no prompt or instructions\n",
        )
        .unwrap();

        let storage_path = temp_dir.path().join("schedule.json");
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let scheduler = Scheduler::new(storage_path, session_manager).await.unwrap();

        let job = ScheduledJob {
            id: "no_prompt_job".to_string(),
            source: recipe_path.to_string_lossy().to_string(),
            cron: "* * * * * *".to_string(),
            last_run: None,
            currently_running: false,
            paused: false,
            current_session_id: None,
            process_start_time: None,
        };

        // Schedule the job and let it run — should not panic
        scheduler.add_scheduled_job(job, true).await.unwrap();
        sleep(Duration::from_millis(1500)).await;

        // The job should have attempted to run (last_run set) but not crashed the scheduler
        let jobs = scheduler.list_scheduled_jobs().await;
        assert!(
            jobs[0].last_run.is_some(),
            "Job should have attempted to run without panicking"
        );
    }
}
