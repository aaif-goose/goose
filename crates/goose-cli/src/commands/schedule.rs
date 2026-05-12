use anyhow::{bail, Context, Result};
use goose::scheduler::{
    get_default_scheduled_recipes_dir, get_default_scheduler_storage_path, ScheduledJob, Scheduler,
    SchedulerError,
};
use goose::session::SessionManager;
use std::path::Path;
use std::sync::Arc;

fn validate_cron_expression(cron: &str) -> Result<()> {
    // Basic validation and helpful suggestions
    if cron.trim().is_empty() {
        bail!("Cron expression cannot be empty");
    }

    // Check for common mistakes and provide helpful suggestions
    let parts: Vec<&str> = cron.split_whitespace().collect();

    match parts.len() {
        5 => {
            // Standard 5-field cron (minute hour day month weekday)
            println!("✅ Using standard 5-field cron format: {}", cron);
        }
        6 => {
            // 6-field cron with seconds (second minute hour day month weekday)
            println!("✅ Using 6-field cron format with seconds: {}", cron);
        }
        1 if cron.starts_with('@') => {
            // Shorthand expressions like @hourly, @daily, etc.
            let valid_shorthands = [
                "@yearly",
                "@annually",
                "@monthly",
                "@weekly",
                "@daily",
                "@midnight",
                "@hourly",
            ];
            if valid_shorthands.contains(&cron) {
                println!("✅ Using cron shorthand: {}", cron);
            } else {
                println!(
                    "⚠️  Unknown cron shorthand '{}'. Valid options: {}",
                    cron,
                    valid_shorthands.join(", ")
                );
            }
        }
        _ => {
            println!("⚠️  Unusual cron format detected: '{}'", cron);
            println!("   Common formats:");
            println!("   - 5 fields: '0 * * * *' (minute hour day month weekday)");
            println!("   - 6 fields: '0 0 * * * *' (second minute hour day month weekday)");
            println!("   - Shorthand: '@hourly', '@daily', '@weekly', '@monthly'");
        }
    }

    // Provide examples for common scheduling needs
    if cron == "* * * * *" {
        println!("⚠️  This will run every minute! Did you mean:");
        println!("   - '0 * * * *' for every hour?");
        println!("   - '0 0 * * *' for every day?");
    }

    Ok(())
}

pub async fn handle_schedule_add(
    schedule_id: String,
    cron: String,
    recipe_source_arg: String, // This is expected to be a file path by the Scheduler
) -> Result<()> {
    println!(
        "[CLI Debug] Scheduling job ID: {}, Cron: {}, Recipe Source Path: {}",
        schedule_id, cron, recipe_source_arg
    );

    validate_cron_expression(&cron)?;

    // The Scheduler's add_scheduled_job will handle copying the recipe from recipe_source_arg
    // to its internal storage and validating the path.
    let job = ScheduledJob {
        id: schedule_id.clone(),
        source: recipe_source_arg.clone(), // Pass the original user-provided path
        cron,
        last_run: None,
        currently_running: false,
        paused: false,
        current_session_id: None,
        process_start_time: None,
    };

    let scheduler_storage_path =
        get_default_scheduler_storage_path().context("Failed to get scheduler storage path")?;
    let session_manager = Arc::new(SessionManager::instance());
    let scheduler = Scheduler::new(scheduler_storage_path, session_manager)
        .await
        .context("Failed to initialize scheduler")?;

    match scheduler.add_scheduled_job(job, true).await {
        Ok(_) => {
            // The scheduler has copied the recipe to its internal directory.
            // We can reconstruct the likely path for display if needed, or adjust success message.
            let scheduled_recipes_dir = get_default_scheduled_recipes_dir()
                .unwrap_or_else(|_| Path::new("./.goose_scheduled_recipes").to_path_buf()); // Fallback for display
            let extension = Path::new(&recipe_source_arg)
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("yaml");
            let final_recipe_path =
                scheduled_recipes_dir.join(format!("{}.{}", schedule_id, extension));

            println!(
                "Scheduled job '{}' added. Recipe expected at {:?}",
                schedule_id, final_recipe_path
            );
            Ok(())
        }
        Err(e) => {
            // No local file to clean up by the CLI in this revised flow.
            match e {
                SchedulerError::JobIdExists(job_id) => {
                    bail!("Error: Job with ID '{}' already exists.", job_id);
                }
                SchedulerError::RecipeLoadError(msg) => {
                    bail!(
                        "Error with recipe source: {}. Path: {}",
                        msg,
                        recipe_source_arg
                    );
                }
                _ => Err(anyhow::Error::new(e))
                    .context(format!("Failed to add job '{}' to scheduler", schedule_id)),
            }
        }
    }
}

pub async fn handle_schedule_list() -> Result<()> {
    let scheduler_storage_path =
        get_default_scheduler_storage_path().context("Failed to get scheduler storage path")?;
    let session_manager = Arc::new(SessionManager::instance());
    let scheduler = Scheduler::new(scheduler_storage_path, session_manager)
        .await
        .context("Failed to initialize scheduler")?;

    let jobs = scheduler.list_scheduled_jobs().await;
    if jobs.is_empty() {
        println!("No scheduled jobs found.");
    } else {
        println!("Scheduled Jobs:");
        for job in jobs {
            let status = if job.currently_running {
                "🟢 RUNNING"
            } else if job.paused {
                "⏸️  PAUSED"
            } else {
                "⏹️  IDLE"
            };

            println!(
                "- ID: {}\n  Status: {}\n  Cron: {}\n  Recipe Source (in store): {}\n  Last Run: {}",
                job.id,
                status,
                job.cron,
                job.source, // This source is now the path within scheduled_recipes_dir
                job.last_run
                    .map_or_else(|| "Never".to_string(), |dt| dt.to_rfc3339())
            );
        }
    }
    Ok(())
}

pub async fn handle_schedule_remove(schedule_id: String) -> Result<()> {
    let scheduler_storage_path =
        get_default_scheduler_storage_path().context("Failed to get scheduler storage path")?;
    let session_manager = Arc::new(SessionManager::instance());
    let scheduler = Scheduler::new(scheduler_storage_path, session_manager)
        .await
        .context("Failed to initialize scheduler")?;

    match scheduler.remove_scheduled_job(&schedule_id, false).await {
        Ok(_) => {
            println!(
                "Scheduled job '{}' removed. Associated recipe was kept.",
                schedule_id
            );
            Ok(())
        }
        Err(e) => match e {
            SchedulerError::JobNotFound(job_id) => {
                bail!("Error: Job with ID '{}' not found.", job_id);
            }
            _ => Err(anyhow::Error::new(e)).context(format!(
                "Failed to remove job '{}' from scheduler",
                schedule_id
            )),
        },
    }
}

pub async fn handle_schedule_sessions(schedule_id: String, limit: Option<usize>) -> Result<()> {
    let scheduler_storage_path =
        get_default_scheduler_storage_path().context("Failed to get scheduler storage path")?;
    let session_manager = Arc::new(SessionManager::instance());
    let scheduler = Scheduler::new(scheduler_storage_path, session_manager)
        .await
        .context("Failed to initialize scheduler")?;

    match scheduler.sessions(&schedule_id, limit.unwrap_or(50)).await {
        Ok(sessions) => {
            if sessions.is_empty() {
                println!("No sessions found for schedule ID '{}'.", schedule_id);
            } else {
                println!("Sessions for schedule ID '{}':", schedule_id);
                // sessions is now Vec<(String, SessionMetadata)>
                for (session_name, metadata) in sessions {
                    println!(
                        "  - Session ID: {}, Working Dir: {}, Description: \"{}\", Schedule ID: {:?}",
                        session_name, // Display the session_name as Session ID
                        metadata.working_dir.display(),
                        metadata.name,
                        metadata.schedule_id.as_deref().unwrap_or("N/A")
                    );
                }
            }
        }
        Err(e) => {
            bail!(
                "Failed to get sessions for schedule '{}': {:?}",
                schedule_id,
                e
            );
        }
    }
    Ok(())
}

pub async fn handle_schedule_run_now(schedule_id: String) -> Result<()> {
    let scheduler_storage_path =
        get_default_scheduler_storage_path().context("Failed to get scheduler storage path")?;
    let session_manager = Arc::new(SessionManager::instance());
    let scheduler = Scheduler::new(scheduler_storage_path, session_manager)
        .await
        .context("Failed to initialize scheduler")?;

    match scheduler.run_now(&schedule_id).await {
        Ok(session_id) => {
            println!(
                "Successfully triggered schedule '{}'. New session ID: {}",
                schedule_id, session_id
            );
        }
        Err(e) => match e {
            SchedulerError::JobNotFound(job_id) => {
                bail!("Error: Job with ID '{}' not found.", job_id);
            }
            _ => bail!("Failed to run schedule '{}' now: {:?}", schedule_id, e),
        },
    }
    Ok(())
}

pub async fn handle_schedule_services_status() -> Result<()> {
    println!("Service management has been removed as Temporal scheduler is no longer supported.");
    println!(
        "The built-in scheduler runs within the goose process and requires no external services."
    );
    Ok(())
}

pub async fn handle_schedule_services_stop() -> Result<()> {
    println!("Service management has been removed as Temporal scheduler is no longer supported.");
    println!(
        "The built-in scheduler runs within the goose process and requires no external services."
    );
    Ok(())
}

pub async fn handle_schedule_cron_help() -> Result<()> {
    println!("📅 Cron Expression Guide for goose Scheduler");
    println!("===========================================\\n");

    println!("🕐 HOURLY SCHEDULES (Most Common Request):");
    println!("  0 * * * *       - Every hour at minute 0 (e.g., 1:00, 2:00, 3:00...)");
    println!("  30 * * * *      - Every hour at minute 30 (e.g., 1:30, 2:30, 3:30...)");
    println!("  0 */2 * * *     - Every 2 hours at minute 0 (e.g., 2:00, 4:00, 6:00...)");
    println!("  0 */3 * * *     - Every 3 hours at minute 0 (e.g., 3:00, 6:00, 9:00...)");
    println!("  @hourly         - Every hour (same as \"0 * * * *\")\\n");

    println!("📅 DAILY SCHEDULES:");
    println!("  0 9 * * *       - Every day at 9:00 AM");
    println!("  30 14 * * *     - Every day at 2:30 PM");
    println!("  0 0 * * *       - Every day at midnight");
    println!("  @daily          - Every day at midnight\\n");

    println!("📆 WEEKLY SCHEDULES:");
    println!("  0 9 * * 1       - Every Monday at 9:00 AM");
    println!("  0 17 * * 5      - Every Friday at 5:00 PM");
    println!("  0 0 * * 0       - Every Sunday at midnight");
    println!("  @weekly         - Every Sunday at midnight\\n");

    println!("🗓️  MONTHLY SCHEDULES:");
    println!("  0 9 1 * *       - First day of every month at 9:00 AM");
    println!("  0 0 15 * *      - 15th of every month at midnight");
    println!("  @monthly        - First day of every month at midnight\\n");

    println!("📝 CRON FORMAT:");
    println!("  Standard 5-field: minute hour day month weekday");
    println!("  ┌───────────── minute (0 - 59)");
    println!("  │ ┌─────────── hour (0 - 23)");
    println!("  │ │ ┌───────── day of month (1 - 31)");
    println!("  │ │ │ ┌─────── month (1 - 12)");
    println!("  │ │ │ │ ┌───── day of week (0 - 7, Sunday = 0 or 7)");
    println!("  │ │ │ │ │");
    println!("  * * * * *\\n");

    println!("🔧 SPECIAL CHARACTERS:");
    println!("  *     - Any value (every minute, hour, day, etc.)");
    println!("  */n   - Every nth interval (*/5 = every 5 minutes)");
    println!("  n-m   - Range (1-5 = 1,2,3,4,5)");
    println!("  n,m   - List (1,3,5 = 1 or 3 or 5)\\n");

    println!("⚡ SHORTHAND EXPRESSIONS:");
    println!("  @yearly   - Once a year (0 0 1 1 *)");
    println!("  @monthly  - Once a month (0 0 1 * *)");
    println!("  @weekly   - Once a week (0 0 * * 0)");
    println!("  @daily    - Once a day (0 0 * * *)");
    println!("  @hourly   - Once an hour (0 * * * *)\\n");

    println!("💡 EXAMPLES:");
    println!(
        "  goose schedule add --schedule-id hourly-report --cron \"0 * * * *\" --recipe-source report.yaml"
    );
    println!(
        "  goose schedule add --schedule-id daily-backup --cron \"@daily\" --recipe-source backup.yaml"
    );
    println!("  goose schedule add --schedule-id weekly-summary --cron \"0 9 * * 1\" --recipe-source summary.yaml");

    Ok(())
}
