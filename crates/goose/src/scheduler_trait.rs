use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

use crate::scheduler::{ScheduledJob, SchedulerError, ValidatedScheduleRecipe};
use crate::session::Session;

#[async_trait]
pub trait SchedulerTrait: Send + Sync {
    async fn add_scheduled_job(
        &self,
        job: ScheduledJob,
        copy_recipe: bool,
    ) -> Result<(), SchedulerError>;
    async fn add_scheduled_job_with_recipe(
        &self,
        job: ScheduledJob,
        validated_recipe: ValidatedScheduleRecipe,
    ) -> Result<(), SchedulerError>;
    async fn schedule_recipe(
        &self,
        recipe_path: PathBuf,
        cron_schedule: Option<String>,
    ) -> anyhow::Result<(), SchedulerError>;
    async fn list_scheduled_jobs(&self) -> Vec<ScheduledJob>;
    async fn remove_scheduled_job(
        &self,
        id: &str,
        remove_recipe: bool,
    ) -> Result<(), SchedulerError>;
    async fn pause_schedule(&self, id: &str) -> Result<(), SchedulerError>;
    async fn unpause_schedule(&self, id: &str) -> Result<(), SchedulerError>;
    async fn run_now(&self, id: &str) -> Result<String, SchedulerError>;
    async fn sessions(
        &self,
        sched_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, Session)>, SchedulerError>;
    async fn update_schedule(&self, sched_id: &str, new_cron: String)
        -> Result<(), SchedulerError>;
    async fn kill_running_job(&self, sched_id: &str) -> Result<(), SchedulerError>;
    async fn get_running_job_info(
        &self,
        sched_id: &str,
    ) -> Result<Option<(String, DateTime<Utc>)>, SchedulerError>;
}
