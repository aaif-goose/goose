use super::*;
use goose::agents::subagent_execution_tool::notification_events::{
    FailedTaskInfo, TaskCompletionStats, TaskExecutionStats,
};
use serde_json::json;

#[test]
fn test_process_output_for_display() {
    assert_eq!(process_output_for_display("hello world"), "hello world");
    assert_eq!(
        process_output_for_display("line1\nline2"),
        "line1 ... line2"
    );

    let input = "line1\nline2\nline3\nline4";
    let result = process_output_for_display(input);
    assert_eq!(result, "line3 ... line4");

    let long_line = "a".repeat(150);
    let result = process_output_for_display(&long_line);
    assert!(result.len() <= 100);
    assert!(result.ends_with("..."));

    assert_eq!(process_output_for_display(""), "");
}

#[test]
fn test_format_result_data_for_display() {
    assert_eq!(
        format_result_data_for_display(&json!("red text")),
        "red text"
    );

    assert_eq!(format_result_data_for_display(&json!(true)), "true");
    assert_eq!(format_result_data_for_display(&json!(false)), "false");
    assert_eq!(format_result_data_for_display(&json!(42)), "42");
    assert_eq!(format_result_data_for_display(&json!(3.41)), "3.41");
    assert_eq!(format_result_data_for_display(&json!(null)), "null");

    let partial_obj = json!({
        "partial_output": "some output",
        "other_field": "ignored"
    });
    assert_eq!(
        format_result_data_for_display(&partial_obj),
        "Partial output: some output"
    );

    let obj = json!({"key": "value", "num": 42});
    let result = format_result_data_for_display(&obj);
    assert!(result.contains("key"));
    assert!(result.contains("value"));

    let arr = json!([1, 2, 3]);
    let result = format_result_data_for_display(&arr);
    assert!(result.contains("1"));
    assert!(result.contains("2"));
    assert!(result.contains("3"));
}

#[test]
fn test_format_task_execution_notification_line_output() {
    let _event = TaskExecutionNotificationEvent::LineOutput {
        task_id: "task-1".to_string(),
        output: "Hello World".to_string(),
    };

    let data = json!({
        "subtype": "line_output",
        "task_id": "task-1",
        "output": "Hello World"
    });

    let result = format_task_execution_notification(&data);
    assert!(result.is_some());

    let (formatted, second, third) = result.unwrap();
    assert_eq!(formatted, "Hello World\n");
    assert_eq!(second, None);
    assert_eq!(third, Some("task_execution".to_string()));
}

#[test]
fn test_format_task_execution_notification_invalid_data() {
    let invalid_data = json!({
        "invalid": "structure"
    });

    let result = format_task_execution_notification(&invalid_data);
    assert_eq!(result, None);

    let incomplete_data = json!({
        "subtype": "line_output"
    });

    let result = format_task_execution_notification(&incomplete_data);
    assert_eq!(result, None);
}

#[test]
fn test_format_tasks_update_from_event() {
    INITIAL_SHOWN.store(false, Ordering::SeqCst);

    let stats = TaskExecutionStats::new(3, 1, 1, 1, 0);
    let tasks = vec![
        TaskInfo {
            id: "task-1".to_string(),
            status: TaskStatus::Running,
            duration_secs: Some(1.5),
            current_output: "Processing...".to_string(),
            task_type: "sub_recipe".to_string(),
            task_name: "test-task".to_string(),
            task_metadata: "param=value".to_string(),
            error: None,
            result_data: None,
        },
        TaskInfo {
            id: "task-2".to_string(),
            status: TaskStatus::Completed,
            duration_secs: Some(2.3),
            current_output: "".to_string(),
            task_type: "text_instruction".to_string(),
            task_name: "another-task".to_string(),
            task_metadata: "".to_string(),
            error: None,
            result_data: Some(json!({"result": "success"})),
        },
    ];

    let event = TaskExecutionNotificationEvent::TasksUpdate { stats, tasks };
    let result = format_tasks_update_from_event(&event);

    assert!(result.contains("🎯 Task Execution Dashboard"));
    assert!(result.contains("═══════════════════════════"));
    assert!(result.contains("📊 Progress: 3 total"));
    assert!(result.contains("⏳ 1 pending"));
    assert!(result.contains("🏃 1 running"));
    assert!(result.contains("✅ 1 completed"));
    assert!(result.contains("❌ 0 failed"));
    assert!(result.contains("🏃 test-task"));
    assert!(result.contains("✅ another-task"));
    assert!(result.contains("📋 Parameters: param=value"));
    assert!(result.contains("⏱️  1.5s"));
    assert!(result.contains("💬 Processing..."));

    let result2 = format_tasks_update_from_event(&event);
    assert!(!result2.contains("🎯 Task Execution Dashboard"));
    assert!(result2.contains("📊 Progress:"));
}

#[test]
fn test_format_tasks_complete_from_event() {
    let stats = TaskCompletionStats::new(5, 4, 1);
    let failed_tasks = vec![FailedTaskInfo {
        id: "task-3".to_string(),
        name: "failed-task".to_string(),
        error: Some("Connection timeout".to_string()),
    }];

    let event = TaskExecutionNotificationEvent::TasksComplete {
        stats,
        failed_tasks,
    };
    let result = format_tasks_complete_from_event(&event);

    assert!(result.contains("Execution Complete!"));
    assert!(result.contains("═══════════════════════"));
    assert!(result.contains("Total Tasks: 5"));
    assert!(result.contains("✅ Completed: 4"));
    assert!(result.contains("❌ Failed: 1"));
    assert!(result.contains("📈 Success Rate: 80.0%"));
    assert!(result.contains("❌ Failed Tasks:"));
    assert!(result.contains("• failed-task"));
    assert!(result.contains("Error: Connection timeout"));
    assert!(result.contains("📝 Generating summary..."));
}

#[test]
fn test_format_tasks_complete_from_event_no_failures() {
    let stats = TaskCompletionStats::new(3, 3, 0);
    let failed_tasks = vec![];

    let event = TaskExecutionNotificationEvent::TasksComplete {
        stats,
        failed_tasks,
    };
    let result = format_tasks_complete_from_event(&event);

    assert!(!result.contains("❌ Failed Tasks:"));
    assert!(result.contains("📈 Success Rate: 100.0%"));
    assert!(result.contains("❌ Failed: 0"));
}

#[test]
fn test_format_task_display_running() {
    let task = TaskInfo {
        id: "task-1".to_string(),
        status: TaskStatus::Running,
        duration_secs: Some(1.5),
        current_output: "Processing data...\nAlmost done...".to_string(),
        task_type: "sub_recipe".to_string(),
        task_name: "data-processor".to_string(),
        task_metadata: "input=file.txt,output=result.json".to_string(),
        error: None,
        result_data: None,
    };

    let result = format_task_display(&task);

    assert!(result.contains("🏃 data-processor (sub_recipe)"));
    assert!(result.contains("📋 Parameters: input=file.txt,output=result.json"));
    assert!(result.contains("⏱️  1.5s"));
    assert!(result.contains("💬 Processing data... ... Almost done..."));
}

#[test]
fn test_format_task_display_completed() {
    let task = TaskInfo {
        id: "task-2".to_string(),
        status: TaskStatus::Completed,
        duration_secs: Some(3.2),
        current_output: "".to_string(),
        task_type: "text_instruction".to_string(),
        task_name: "analyzer".to_string(),
        task_metadata: "".to_string(),
        error: None,
        result_data: Some(json!({"status": "success", "count": 42})),
    };

    let result = format_task_display(&task);

    assert!(result.contains("✅ analyzer (text_instruction)"));
    assert!(result.contains("⏱️  3.2s"));
    assert!(!result.contains("📋 Parameters"));
    assert!(result.contains("📄"));
}

#[test]
fn test_format_task_display_failed() {
    let task = TaskInfo {
        id: "task-3".to_string(),
        status: TaskStatus::Failed,
        duration_secs: None,
        current_output: "".to_string(),
        task_type: "sub_recipe".to_string(),
        task_name: "failing-task".to_string(),
        task_metadata: "".to_string(),
        error: Some(
            "Network connection failed after multiple retries. The server is unreachable."
                .to_string(),
        ),
        result_data: None,
    };

    let result = format_task_display(&task);

    assert!(result.contains("❌ failing-task (sub_recipe)"));
    assert!(!result.contains("⏱️"));
    assert!(result.contains("⚠️"));
    assert!(result.contains("Network connection failed after multiple retries"));
}

#[test]
fn test_format_task_display_pending() {
    let task = TaskInfo {
        id: "task-4".to_string(),
        status: TaskStatus::Pending,
        duration_secs: None,
        current_output: "".to_string(),
        task_type: "sub_recipe".to_string(),
        task_name: "waiting-task".to_string(),
        task_metadata: "priority=high".to_string(),
        error: None,
        result_data: None,
    };

    let result = format_task_display(&task);

    assert!(result.contains("⏳ waiting-task (sub_recipe)"));
    assert!(result.contains("📋 Parameters: priority=high"));
    assert!(!result.contains("⏱️"));
    assert!(!result.contains("💬"));
    assert!(!result.contains("📄"));
    assert!(!result.contains("⚠️"));
}

#[test]
fn test_format_task_display_empty_current_output() {
    let task = TaskInfo {
        id: "task-5".to_string(),
        status: TaskStatus::Running,
        duration_secs: Some(0.5),
        current_output: "   \n\t  \n   ".to_string(),
        task_type: "sub_recipe".to_string(),
        task_name: "quiet-task".to_string(),
        task_metadata: "".to_string(),
        error: None,
        result_data: None,
    };

    let result = format_task_display(&task);

    assert!(!result.contains("💬"));
}
