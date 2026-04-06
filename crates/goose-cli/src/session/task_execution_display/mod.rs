use goose::agents::subagent_execution_tool::lib::TaskStatus;
use goose::agents::subagent_execution_tool::notification_events::{
    TaskExecutionNotificationEvent, TaskInfo,
};
use goose::utils::safe_truncate;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
mod tests;

pub const TASK_EXECUTION_NOTIFICATION_TYPE: &str = "task_execution";

static INITIAL_SHOWN: AtomicBool = AtomicBool::new(false);

fn format_result_data_for_display(result_data: &Value) -> String {
    match result_data {
        Value::String(s) => s.to_string(),
        Value::Object(obj) => {
            if let Some(partial_output) = obj.get("partial_output").and_then(|v| v.as_str()) {
                format!("Partial output: {}", partial_output)
            } else {
                serde_json::to_string_pretty(obj).unwrap_or_default()
            }
        }
        Value::Array(arr) => serde_json::to_string_pretty(arr).unwrap_or_default(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => "null".to_string(),
    }
}

fn process_output_for_display(output: &str) -> String {
    const MAX_OUTPUT_LINES: usize = 2;
    const OUTPUT_PREVIEW_LENGTH: usize = 100;

    let lines: Vec<&str> = output.lines().collect();
    let recent_lines = if lines.len() > MAX_OUTPUT_LINES {
        &lines[lines.len() - MAX_OUTPUT_LINES..]
    } else {
        &lines
    };

    let clean_output = recent_lines.join(" ... ");
    safe_truncate(&clean_output, OUTPUT_PREVIEW_LENGTH)
}

pub fn format_task_execution_notification(
    data: &Value,
) -> Option<(String, Option<String>, Option<String>)> {
    if let Ok(event) = serde_json::from_value::<TaskExecutionNotificationEvent>(data.clone()) {
        return Some(match event {
            TaskExecutionNotificationEvent::LineOutput { output, .. } => (
                format!("{}\n", output),
                None,
                Some(TASK_EXECUTION_NOTIFICATION_TYPE.to_string()),
            ),
            TaskExecutionNotificationEvent::TasksUpdate { .. } => {
                let formatted_display = format_tasks_update_from_event(&event);
                (
                    formatted_display,
                    None,
                    Some(TASK_EXECUTION_NOTIFICATION_TYPE.to_string()),
                )
            }
            TaskExecutionNotificationEvent::TasksComplete { .. } => {
                let formatted_summary = format_tasks_complete_from_event(&event);
                (
                    formatted_summary,
                    None,
                    Some(TASK_EXECUTION_NOTIFICATION_TYPE.to_string()),
                )
            }
        });
    }
    None
}

fn format_tasks_update_from_event(event: &TaskExecutionNotificationEvent) -> String {
    if let TaskExecutionNotificationEvent::TasksUpdate { stats, tasks } = event {
        let mut display = String::new();

        if !INITIAL_SHOWN.swap(true, Ordering::SeqCst) {
            display.push_str("🎯 Task Execution Dashboard\n");
            display.push_str("═══════════════════════════\n\n");
        }

        display.push_str(&format!(
            "📊 Progress: {} total | ⏳ {} pending | 🏃 {} running | ✅ {} completed | ❌ {} failed\n\n",
            stats.total, stats.pending, stats.running, stats.completed, stats.failed
        ));

        let mut sorted_tasks = tasks.clone();
        sorted_tasks.sort_by(|a, b| a.id.cmp(&b.id));

        for task in sorted_tasks {
            display.push_str(&format_task_display(&task));
        }

        display
    } else {
        String::new()
    }
}

fn format_tasks_complete_from_event(event: &TaskExecutionNotificationEvent) -> String {
    if let TaskExecutionNotificationEvent::TasksComplete {
        stats,
        failed_tasks,
    } = event
    {
        let mut summary = String::new();
        summary.push_str("Execution Complete!\n");
        summary.push_str("═══════════════════════\n");

        summary.push_str(&format!("Total Tasks: {}\n", stats.total));
        summary.push_str(&format!("✅ Completed: {}\n", stats.completed));
        summary.push_str(&format!("❌ Failed: {}\n", stats.failed));
        summary.push_str(&format!("📈 Success Rate: {:.1}%\n", stats.success_rate));

        if !failed_tasks.is_empty() {
            summary.push_str("\n❌ Failed Tasks:\n");
            for task in failed_tasks {
                summary.push_str(&format!("   • {}\n", task.name));
                if let Some(error) = &task.error {
                    summary.push_str(&format!("     Error: {}\n", error));
                }
            }
        }

        summary.push_str("\n📝 Generating summary...\n");
        summary
    } else {
        String::new()
    }
}

fn format_task_display(task: &TaskInfo) -> String {
    let mut task_display = String::new();

    let status_icon = match task.status {
        TaskStatus::Pending => "⏳",
        TaskStatus::Running => "🏃",
        TaskStatus::Completed => "✅",
        TaskStatus::Failed => "❌",
    };

    task_display.push_str(&format!(
        "{} {} ({})\n",
        status_icon, task.task_name, task.task_type
    ));

    if !task.task_metadata.is_empty() {
        task_display.push_str(&format!("   📋 Parameters: {}\n", task.task_metadata));
    }

    if let Some(duration_secs) = task.duration_secs {
        task_display.push_str(&format!("   ⏱️  {:.1}s\n", duration_secs));
    }

    if matches!(task.status, TaskStatus::Running) && !task.current_output.trim().is_empty() {
        let processed_output = process_output_for_display(&task.current_output);
        if !processed_output.is_empty() {
            task_display.push_str(&format!("   💬 {}\n", processed_output));
        }
    }

    if matches!(task.status, TaskStatus::Completed) {
        if let Some(result_data) = &task.result_data {
            let result_preview = format_result_data_for_display(result_data);
            if !result_preview.is_empty() {
                task_display.push_str(&format!("   📄 {}\n", result_preview));
            }
        }
    }

    if matches!(task.status, TaskStatus::Failed) {
        if let Some(error) = &task.error {
            let error_preview = safe_truncate(error, 80);
            task_display.push_str(&format!("   ⚠️  {}\n", error_preview.replace('\n', " ")));
        }
    }

    task_display.push('\n');
    task_display
}
