use agent_client_protocol::schema::v1::{
    Meta, ToolCallId, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
};
#[expect(deprecated)]
use rmcp::model::LoggingMessageNotificationParam;
use rmcp::model::{ProgressNotificationParam, ServerNotification};
use serde::Serialize;

use crate::agents::platform_extensions::developer::shell::{
    parse_shell_output_notification, ShellOutputNotificationParams,
};

#[expect(deprecated)]
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ToolNotification {
    Message {
        params: LoggingMessageNotificationParam,
    },
    Progress {
        params: ProgressNotificationParam,
    },
    PlatformEvent {
        params: serde_json::Value,
    },
    LiveOutput {
        params: ShellOutputNotificationParams,
    },
}

pub(super) fn tool_notification_update(
    tool_call_id: impl Into<ToolCallId>,
    notification: ServerNotification,
) -> Option<ToolCallUpdate> {
    let tool_notification = match notification {
        ServerNotification::LoggingMessageNotification(notification) => ToolNotification::Message {
            params: notification.params,
        },
        ServerNotification::ProgressNotification(notification) => ToolNotification::Progress {
            params: notification.params,
        },
        ServerNotification::CustomNotification(notification) => {
            if let Some(params) = parse_shell_output_notification(&notification) {
                ToolNotification::LiveOutput { params }
            } else if notification.method == "platform_event" {
                ToolNotification::PlatformEvent {
                    params: notification.params.unwrap_or(serde_json::Value::Null),
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };

    let mut meta = Meta::new();
    meta.insert(
        "toolNotification".to_string(),
        serde_json::to_value(tool_notification).ok()?,
    );

    Some(
        ToolCallUpdate::new(
            tool_call_id,
            ToolCallUpdateFields::new().status(ToolCallStatus::InProgress),
        )
        .meta(meta),
    )
}

#[cfg(test)]
mod tests {
    use super::tool_notification_update;
    use crate::agents::platform_extensions::developer::shell::DEVELOPER_SHELL_OUTPUT_NOTIFICATION_METHOD;
    use agent_client_protocol::schema::v1::SessionUpdate;
    use rmcp::model::{
        CancelledNotificationParam, CustomNotification, Notification, NumberOrString,
        ProgressNotificationParam, ProgressToken, ServerNotification,
    };
    #[expect(deprecated)]
    use rmcp::model::{LoggingLevel, LoggingMessageNotificationParam};
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    #[expect(deprecated)]
    fn maps_logging_message_notification_to_tool_update_meta() {
        let notification = ServerNotification::LoggingMessageNotification(Notification::new(
            LoggingMessageNotificationParam::new(
                LoggingLevel::Info,
                json!({
                    "type": "subagent_tool_request",
                    "subagent_id": "session_1",
                    "tool_call": {
                        "name": "developer__shell"
                    }
                }),
            )
            .with_logger("subagent:session_1"),
        ));

        let update = tool_notification_update("tool_1", notification).expect("expected update");
        let value = serde_json::to_value(SessionUpdate::ToolCallUpdate(update))
            .expect("update should serialize");

        assert_eq!(value["sessionUpdate"], "tool_call_update");
        assert_eq!(value["toolCallId"], "tool_1");
        assert_eq!(value["status"], "in_progress");
        assert_eq!(value["_meta"]["toolNotification"]["type"], "message");
        assert_eq!(
            value["_meta"]["toolNotification"]["params"]["level"],
            "info"
        );
        assert_eq!(
            value["_meta"]["toolNotification"]["params"]["logger"],
            "subagent:session_1"
        );
        assert_eq!(
            value["_meta"]["toolNotification"]["params"]["data"]["tool_call"]["name"],
            "developer__shell"
        );
    }

    #[test]
    fn maps_progress_notification_to_tool_update_meta() {
        let notification = ServerNotification::ProgressNotification(Notification::new(
            ProgressNotificationParam::new(
                ProgressToken(NumberOrString::String(Arc::from("scan-repo"))),
                3.0,
            )
            .with_total(10.0)
            .with_message("Scanned 3 of 10 directories"),
        ));

        let update = tool_notification_update("tool_1", notification).expect("expected update");
        let value = serde_json::to_value(SessionUpdate::ToolCallUpdate(update))
            .expect("update should serialize");

        assert_eq!(value["sessionUpdate"], "tool_call_update");
        assert_eq!(value["toolCallId"], "tool_1");
        assert_eq!(value["status"], "in_progress");
        assert_eq!(value["_meta"]["toolNotification"]["type"], "progress");
        assert_eq!(
            value["_meta"]["toolNotification"]["params"]["progressToken"],
            "scan-repo"
        );
        assert_eq!(
            value["_meta"]["toolNotification"]["params"]["progress"],
            3.0
        );
        assert_eq!(value["_meta"]["toolNotification"]["params"]["total"], 10.0);
        assert_eq!(
            value["_meta"]["toolNotification"]["params"]["message"],
            "Scanned 3 of 10 directories"
        );
    }

    #[test]
    fn ignores_non_tool_live_notification_variants() {
        let notification = ServerNotification::CancelledNotification(Notification::new(
            CancelledNotificationParam::new(
                Some(NumberOrString::String(Arc::from("request_1"))),
                None,
            ),
        ));

        assert!(tool_notification_update("tool_1", notification).is_none());
    }

    #[test]
    fn maps_platform_event_custom_notification_to_tool_update_meta() {
        let notification = ServerNotification::CustomNotification(CustomNotification::new(
            "platform_event",
            Some(json!({
                "extension": "apps",
                "event_type": "app_created",
                "app_name": "platform-event-repro"
            })),
        ));

        let update = tool_notification_update("tool_1", notification).expect("expected update");
        let value = serde_json::to_value(SessionUpdate::ToolCallUpdate(update))
            .expect("update should serialize");

        assert_eq!(value["sessionUpdate"], "tool_call_update");
        assert_eq!(value["toolCallId"], "tool_1");
        assert_eq!(value["status"], "in_progress");
        assert_eq!(value["_meta"]["toolNotification"]["type"], "platform_event");
        assert_eq!(
            value["_meta"]["toolNotification"]["params"]["extension"],
            "apps"
        );
        assert_eq!(
            value["_meta"]["toolNotification"]["params"]["event_type"],
            "app_created"
        );
        assert_eq!(
            value["_meta"]["toolNotification"]["params"]["app_name"],
            "platform-event-repro"
        );
    }

    #[test]
    fn maps_shell_output_custom_notification_to_live_output_meta() {
        let notification = ServerNotification::CustomNotification(CustomNotification::new(
            DEVELOPER_SHELL_OUTPUT_NOTIFICATION_METHOD,
            Some(json!({
                "sequence": 2,
                "chunks": [{
                    "stream": "stdout",
                    "output": "ready\n"
                }],
                "truncated": false
            })),
        ));

        let update = tool_notification_update("tool_1", notification).expect("expected update");
        let value = serde_json::to_value(SessionUpdate::ToolCallUpdate(update))
            .expect("update should serialize");

        assert_eq!(value["sessionUpdate"], "tool_call_update");
        assert_eq!(value["toolCallId"], "tool_1");
        assert_eq!(value["status"], "in_progress");
        assert_eq!(value["_meta"]["toolNotification"]["type"], "live_output");
        assert_eq!(
            value["_meta"]["toolNotification"]["params"],
            json!({
                "sequence": 2,
                "chunks": [{
                    "stream": "stdout",
                    "output": "ready\n"
                }],
                "truncated": false
            })
        );
    }

    #[test]
    fn ignores_non_platform_event_custom_notifications() {
        let notification = ServerNotification::CustomNotification(CustomNotification::new(
            "notifications/custom",
            Some(json!({ "extension": "apps" })),
        ));

        assert!(tool_notification_update("tool_1", notification).is_none());
    }
}
