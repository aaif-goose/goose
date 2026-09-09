use crate::conversation::message::{Message, MessageContent};
use agent_client_protocol::schema::v1::{ContentBlock, ContentChunk, MessageId, Meta};
use serde::Serialize;

const OUTPUT_TOKEN_LIMIT_TEXT: &str =
    "Response stopped because the model reached its output-token limit.";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GooseMessageMeta<'a> {
    created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_id: Option<&'a str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    steer: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    output_token_limit_reached: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    fallback_content: bool,
}

fn goose_message_meta(
    message: &Message,
    steer: bool,
) -> serde_json::Map<String, serde_json::Value> {
    let message_meta = GooseMessageMeta {
        created: message.created,
        message_id: message.id.as_deref(),
        steer,
        output_token_limit_reached: message.metadata.output_token_limit_reached,
        fallback_content: has_output_token_limit_fallback_content(message),
    };

    match serde_json::to_value(message_meta) {
        Ok(serde_json::Value::Object(meta)) => meta,
        _ => serde_json::Map::new(),
    }
}

fn extend_message_meta(meta: &mut Meta, message: &Message, steer: bool) {
    let message_goose = goose_message_meta(message, steer);
    let goose_value = meta
        .entry("goose".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

    if let serde_json::Value::Object(goose) = goose_value {
        goose.extend(message_goose);
    } else {
        *goose_value = serde_json::Value::Object(message_goose);
    }
}

fn message_meta_with_steer(message: &Message, steer: bool) -> Meta {
    let mut meta = Meta::new();
    extend_message_meta(&mut meta, message, steer);
    meta
}

fn message_meta(message: &Message) -> Meta {
    message_meta_with_steer(message, message.metadata.steer)
}

pub(super) fn message_meta_without_steer(message: &Message) -> Meta {
    message_meta_with_steer(message, false)
}

pub(super) fn merge_message_meta(mut meta: Meta, message: &Message) -> Meta {
    extend_message_meta(&mut meta, message, message.metadata.steer);
    meta
}

pub(super) fn content_chunk_for_message(message: &Message, content: ContentBlock) -> ContentChunk {
    let mut chunk = ContentChunk::new(content).meta(message_meta(message));
    if let Some(message_id) = message.id.as_deref() {
        chunk = chunk.message_id(MessageId::new(message_id));
    }
    chunk
}

pub(super) fn populate_output_token_limit_content(message: &mut Message) {
    if message.role != rmcp::model::Role::Assistant
        || !message.content.is_empty()
        || !message.metadata.output_token_limit_reached
    {
        return;
    }

    message
        .content
        .push(MessageContent::text(OUTPUT_TOKEN_LIMIT_TEXT));
}

fn has_output_token_limit_fallback_content(message: &Message) -> bool {
    message.role == rmcp::model::Role::Assistant
        && message.metadata.output_token_limit_reached
        && matches!(
            message.content.as_slice(),
            [MessageContent::Text(text)] if text.text == OUTPUT_TOKEN_LIMIT_TEXT
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::TextContent;
    use rmcp::model::Role;

    #[test]
    fn message_meta_serializes_message_fields() {
        let message = Message::new(Role::Assistant, 1_700_000_000, vec![]).with_id("msg_live");
        assert_eq!(
            message_meta(&message).get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
                "messageId": "msg_live",
            })),
        );

        let steer_message = message.clone().with_steer();
        assert_eq!(
            message_meta(&steer_message).get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
                "messageId": "msg_live",
                "steer": true,
            })),
        );
        assert_eq!(
            message_meta_without_steer(&steer_message).get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
                "messageId": "msg_live",
            })),
        );

        let message_without_id = Message::new(Role::Assistant, 1_700_000_000, vec![]);
        assert_eq!(
            message_meta(&message_without_id).get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
            })),
        );

        let mut limited_message = message.clone();
        limited_message.metadata.output_token_limit_reached = true;
        assert_eq!(
            message_meta(&limited_message).get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
                "messageId": "msg_live",
                "outputTokenLimitReached": true,
            })),
        );

        populate_output_token_limit_content(&mut limited_message);
        assert_eq!(
            message_meta(&limited_message).get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
                "messageId": "msg_live",
                "outputTokenLimitReached": true,
                "fallbackContent": true,
            })),
        );
    }

    #[test]
    fn content_chunk_carries_message_id_and_metadata() {
        let mut message = Message::new(Role::Assistant, 1_700_000_000, vec![])
            .with_id("msg_live")
            .with_steer();
        message.metadata.output_token_limit_reached = true;
        populate_output_token_limit_content(&mut message);
        let chunk = content_chunk_for_message(
            &message,
            ContentBlock::Text(TextContent::new(OUTPUT_TOKEN_LIMIT_TEXT)),
        );

        assert_eq!(chunk.message_id, Some(MessageId::new("msg_live")));
        assert_eq!(
            chunk.meta.as_ref().and_then(|meta| meta.get("goose")),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
                "messageId": "msg_live",
                "outputTokenLimitReached": true,
                "fallbackContent": true,
                "steer": true,
            })),
        );
    }

    #[test]
    fn populate_output_token_limit_content_only_changes_empty_marked_assistant_messages() {
        let mut message = Message::new(Role::Assistant, 1_700_000_000, vec![])
            .with_id("msg_limited")
            .with_steer();
        message.metadata.output_token_limit_reached = true;

        populate_output_token_limit_content(&mut message);

        assert!(matches!(
            message.content.as_slice(),
            [MessageContent::Text(text)] if text.text == OUTPUT_TOKEN_LIMIT_TEXT
        ));
        assert!(has_output_token_limit_fallback_content(&message));

        let mut unmarked_message = Message::new(Role::Assistant, 1_700_000_000, vec![]);
        populate_output_token_limit_content(&mut unmarked_message);
        assert!(unmarked_message.content.is_empty());
        assert!(!has_output_token_limit_fallback_content(&unmarked_message));

        let mut marked_message_with_content =
            Message::new(Role::Assistant, 1_700_000_000, vec![]).with_text("partial response");
        marked_message_with_content
            .metadata
            .output_token_limit_reached = true;
        populate_output_token_limit_content(&mut marked_message_with_content);
        assert!(matches!(
            marked_message_with_content.content.as_slice(),
            [MessageContent::Text(text)] if text.text == "partial response"
        ));
        assert!(!has_output_token_limit_fallback_content(
            &marked_message_with_content
        ));

        let mut marked_user_message = Message::new(Role::User, 1_700_000_000, vec![]);
        marked_user_message.metadata.output_token_limit_reached = true;
        populate_output_token_limit_content(&mut marked_user_message);
        assert!(marked_user_message.content.is_empty());
        assert!(!has_output_token_limit_fallback_content(
            &marked_user_message
        ));
    }

    #[test]
    fn merge_message_meta_preserves_existing_metadata() {
        let mut message = Message::new(Role::Assistant, 1_700_000_000, vec![]).with_id("msg_1");
        message.metadata.output_token_limit_reached = true;
        let existing = serde_json::from_value(serde_json::json!({
            "goose": {
                "created": 1,
                "messageId": "old",
                "toolCall": {
                    "toolName": "weather__render",
                    "extensionName": "weather",
                },
                "mcpApp": {
                    "resourceUri": "ui://trusted/app",
                    "extensionName": "weather",
                    "toolName": "weather__render",
                },
            },
            "otherNamespace": {
                "preserve": true,
            },
        }))
        .unwrap();

        let merged = merge_message_meta(existing, &message);

        assert_eq!(
            merged.get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
                "messageId": "msg_1",
                "outputTokenLimitReached": true,
                "toolCall": {
                    "toolName": "weather__render",
                    "extensionName": "weather",
                },
                "mcpApp": {
                    "resourceUri": "ui://trusted/app",
                    "extensionName": "weather",
                    "toolName": "weather__render",
                },
            })),
        );
        assert_eq!(
            merged.get("otherNamespace"),
            Some(&serde_json::json!({
                "preserve": true,
            })),
        );
    }

    #[test]
    fn merge_message_meta_replaces_non_object_goose_metadata() {
        let message = Message::new(Role::Assistant, 1_700_000_000, vec![]).with_id("msg_1");
        let existing = serde_json::from_value(serde_json::json!({
            "goose": "invalid",
            "otherNamespace": {
                "preserve": true,
            },
        }))
        .unwrap();

        let merged = merge_message_meta(existing, &message);

        assert_eq!(
            merged.get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
                "messageId": "msg_1",
            })),
        );
        assert_eq!(
            merged.get("otherNamespace"),
            Some(&serde_json::json!({
                "preserve": true,
            })),
        );
    }
}
