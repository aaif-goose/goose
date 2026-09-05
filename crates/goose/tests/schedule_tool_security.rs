use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use goose::agents::ScheduleTool;
use goose::scheduler::{ScheduledJob, SchedulerError, ValidatedScheduleRecipe};
use goose::scheduler_trait::SchedulerTrait;
use goose::session::{Session, SessionManager};
use tempfile::TempDir;

struct MockScheduler {
    jobs: tokio::sync::Mutex<Vec<ScheduledJob>>,
    validated_recipes: tokio::sync::Mutex<Vec<Vec<u8>>>,
}

impl MockScheduler {
    fn new() -> Self {
        Self {
            jobs: tokio::sync::Mutex::new(Vec::new()),
            validated_recipes: tokio::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl SchedulerTrait for MockScheduler {
    async fn add_scheduled_job(
        &self,
        job: ScheduledJob,
        _copy: bool,
    ) -> Result<(), SchedulerError> {
        self.jobs.lock().await.push(job);
        Ok(())
    }

    async fn add_scheduled_job_with_recipe(
        &self,
        job: ScheduledJob,
        validated_recipe: ValidatedScheduleRecipe,
    ) -> Result<(), SchedulerError> {
        self.jobs.lock().await.push(job);
        self.validated_recipes
            .lock()
            .await
            .push(validated_recipe.bytes().to_vec());
        Ok(())
    }

    async fn schedule_recipe(
        &self,
        _recipe_path: PathBuf,
        _cron_schedule: Option<String>,
    ) -> Result<(), SchedulerError> {
        Ok(())
    }

    async fn list_scheduled_jobs(&self) -> Vec<ScheduledJob> {
        self.jobs.lock().await.clone()
    }

    async fn remove_scheduled_job(&self, _id: &str, _remove: bool) -> Result<(), SchedulerError> {
        Ok(())
    }

    async fn pause_schedule(&self, _id: &str) -> Result<(), SchedulerError> {
        Ok(())
    }

    async fn unpause_schedule(&self, _id: &str) -> Result<(), SchedulerError> {
        Ok(())
    }

    async fn run_now(&self, _id: &str) -> Result<String, SchedulerError> {
        Ok("test-session".to_string())
    }

    async fn sessions(
        &self,
        _sched_id: &str,
        _limit: usize,
    ) -> Result<Vec<(String, Session)>, SchedulerError> {
        Ok(Vec::new())
    }

    async fn update_schedule(
        &self,
        _sched_id: &str,
        _new_cron: String,
    ) -> Result<(), SchedulerError> {
        Ok(())
    }

    async fn kill_running_job(&self, _sched_id: &str) -> Result<(), SchedulerError> {
        Ok(())
    }

    async fn get_running_job_info(
        &self,
        _sched_id: &str,
    ) -> Result<Option<(String, DateTime<Utc>)>, SchedulerError> {
        Ok(None)
    }
}

fn schedule_tool(temp_dir: &TempDir, scheduler: Arc<MockScheduler>) -> ScheduleTool {
    let data_dir = temp_dir.path().join("data");
    let session_manager = Arc::new(SessionManager::new(data_dir.clone()));
    ScheduleTool::new(scheduler, session_manager)
}

async fn create_schedule(schedule_tool: &ScheduleTool, recipe_path: &Path) -> Result<(), String> {
    schedule_tool
        .execute(serde_json::json!({
            "action": "create",
            "recipe_path": recipe_path,
            "cron_expression": "0 * * * *"
        }))
        .await
        .map(|_| ())
        .map_err(|error| error.message.to_string())
}

#[tokio::test]
async fn parse_errors_do_not_reflect_recipe_contents() {
    let temp_dir = TempDir::new().unwrap();
    let scheduler = Arc::new(MockScheduler::new());
    let tool = schedule_tool(&temp_dir, scheduler.clone());
    let cases = [
        ("invalid.yaml", "yaml-secret-242", "Invalid YAML recipe"),
        ("invalid.json", "\"json-secret-242\"", "Invalid JSON recipe"),
    ];

    for (name, secret, expected) in cases {
        let path = temp_dir.path().join(name);
        std::fs::write(&path, secret).unwrap();
        let message = create_schedule(&tool, &path).await.unwrap_err();
        assert_eq!(message, expected);
        assert!(!message.contains(secret));
    }

    assert!(scheduler.jobs.lock().await.is_empty());
}

#[tokio::test]
async fn reports_actionable_schema_errors_without_reflecting_values() {
    let temp_dir = TempDir::new().unwrap();
    let scheduler = Arc::new(MockScheduler::new());
    let tool = schedule_tool(&temp_dir, scheduler.clone());
    let cases = [
        (
            "missing-title.yaml",
            "description: Missing title\nprompt: Run safely\n".to_string(),
            "Invalid recipe: missing field `title`",
            "Missing title",
        ),
        (
            "invalid-input-type.yaml",
            "title: Test\ndescription: hi\nprompt: Run {{ item }}\nparameters:\n  - key: item\n    input_type: yaml-secret-242\n    requirement: required\n    description: hi\n"
                .to_string(),
            "Invalid recipe: parameters[0].input_type is invalid",
            "yaml-secret-242",
        ),
    ];

    for (name, content, expected, secret) in cases {
        let path = temp_dir.path().join(name);
        std::fs::write(&path, content).unwrap();

        let message = create_schedule(&tool, &path).await.unwrap_err();

        assert_eq!(message, expected);
        assert!(!message.contains(secret));
    }

    assert!(scheduler.jobs.lock().await.is_empty());
    assert!(scheduler.validated_recipes.lock().await.is_empty());
}

#[tokio::test]
async fn rejects_non_regular_recipe_path() {
    let temp_dir = TempDir::new().unwrap();
    let scheduler = Arc::new(MockScheduler::new());
    let tool = schedule_tool(&temp_dir, scheduler.clone());

    let message = create_schedule(&tool, temp_dir.path()).await.unwrap_err();

    assert_eq!(message, "Recipe path must reference a regular file");
    assert!(scheduler.jobs.lock().await.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_fifo_without_blocking() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::time::Duration;

    let temp_dir = TempDir::new().unwrap();
    let scheduler = Arc::new(MockScheduler::new());
    let tool = schedule_tool(&temp_dir, scheduler.clone());
    let path = temp_dir.path().join("recipe.yaml");
    let fifo_path = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: fifo_path is a valid, NUL-terminated path and mode contains only permission bits.
    assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);

    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let watchdog_path = path.clone();
    let watchdog = std::thread::spawn(move || {
        let timed_out = finished_rx.recv_timeout(Duration::from_secs(2)).is_err();
        if timed_out {
            let _ = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(watchdog_path);
        }
        timed_out
    });

    let message = create_schedule(&tool, &path).await.unwrap_err();
    let _ = finished_tx.send(());

    assert!(!watchdog.join().unwrap(), "FIFO validation blocked on open");
    assert_eq!(message, "Recipe path must reference a regular file");
    assert!(scheduler.jobs.lock().await.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn accepts_symlink_to_regular_recipe_with_canonical_provenance() {
    let temp_dir = TempDir::new().unwrap();
    let scheduler = Arc::new(MockScheduler::new());
    let tool = schedule_tool(&temp_dir, scheduler.clone());
    let target = temp_dir.path().join("target.yaml");
    let link = temp_dir.path().join("recipe-link.yaml");
    std::fs::write(
        &target,
        b"title: Valid recipe\ndescription: A small recipe\nprompt: Run safely\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    create_schedule(&tool, &link).await.unwrap();

    let canonical_target = target.canonicalize().unwrap();
    let jobs = scheduler.jobs.lock().await;
    assert_eq!(jobs[0].source, canonical_target.to_string_lossy());
    assert_eq!(
        jobs[0].recipe_base_dir.as_deref(),
        canonical_target.parent().and_then(Path::to_str)
    );
}

#[tokio::test]
async fn rejects_oversized_recipe() {
    let temp_dir = TempDir::new().unwrap();
    let scheduler = Arc::new(MockScheduler::new());
    let tool = schedule_tool(&temp_dir, scheduler.clone());
    let path = temp_dir.path().join("oversized.yaml");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(1_048_577)
        .unwrap();

    let message = create_schedule(&tool, &path).await.unwrap_err();

    assert_eq!(message, "Recipe file exceeds the 1048576 byte limit");
    assert!(scheduler.jobs.lock().await.is_empty());
}

#[tokio::test]
async fn accepts_valid_regular_recipe() {
    let temp_dir = TempDir::new().unwrap();
    let scheduler = Arc::new(MockScheduler::new());
    let tool = schedule_tool(&temp_dir, scheduler.clone());
    let path = temp_dir.path().join("valid.yaml");
    let recipe = b"title: Valid recipe\ndescription: A small recipe\nprompt: Run safely\n";
    std::fs::write(&path, recipe).unwrap();

    create_schedule(&tool, &path).await.unwrap();

    assert_eq!(scheduler.jobs.lock().await.len(), 1);
    assert_eq!(
        scheduler.validated_recipes.lock().await.as_slice(),
        &[recipe.to_vec()]
    );
    let canonical_path = path.canonicalize().unwrap();
    let jobs = scheduler.jobs.lock().await;
    assert_eq!(jobs[0].source, canonical_path.to_string_lossy());
    assert_eq!(
        jobs[0].recipe_base_dir.as_deref(),
        canonical_path.parent().and_then(Path::to_str)
    );
}
