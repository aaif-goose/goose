use crate::acp::tools::AcpAwareToolMeta;
use crate::agents::extension_manager::TRUSTED_TOOL_UPDATE_META_KEY;
use crate::conversation::message::{Message, ToolNameParts, ToolRequest, ToolResponse};
use crate::mcp_utils::ToolResult;
use agent_client_protocol::schema::v1::{
    BlobResourceContents, Content, ContentBlock, EmbeddedResource, EmbeddedResourceResource,
    ImageContent, Meta, TextContent, TextResourceContents, ToolCall, ToolCallContent, ToolCallId,
    ToolCallLocation, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use rmcp::model::{CallToolResult, ContentBlock as RmcpContentBlock, ResourceContents};

use super::super::message_meta::merge_message_meta;

pub(crate) fn format_tool_name(tool_name: &str) -> String {
    let parts = ToolNameParts::from(tool_name);
    if let Some(extension_name) = parts.extension_name {
        format!(
            "{}: {}",
            extension_name.replace('_', " "),
            parts.tool_name.replace('_', " ")
        )
    } else {
        parts.tool_name.replace('_', " ")
    }
}

fn default_tool_title(tool_name: &str, arguments: Option<&serde_json::Value>) -> String {
    let base = format_tool_name(tool_name);

    let detail = arguments.and_then(|args| {
        let obj = args.as_object()?;
        let keys = if matches!(tool_name, "developer__shell" | "shell") {
            [
                "command", "path", "file", "query", "url", "uri", "name", "pattern", "source",
            ]
        } else {
            [
                "path", "file", "command", "query", "url", "uri", "name", "pattern", "source",
            ]
        };
        for key in &keys {
            if let Some(v) = obj.get(*key) {
                let s = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if !s.is_empty() {
                    let mut lines = s.lines();
                    let first_line = lines.next().unwrap_or(&s);
                    if first_line.len() > 60 || lines.next().is_some() {
                        return Some(format!("{}…", crate::utils::safe_truncate(first_line, 57)));
                    }
                    return Some(first_line.to_string());
                }
            }
        }
        None
    });

    match detail {
        Some(d) => format!("{base} · {d}"),
        None => base,
    }
}

pub(crate) fn goose_tool_call_meta(tool_request: &ToolRequest) -> Option<Meta> {
    let tool_call = tool_request.tool_call.as_ref().ok()?;
    let tool_name = tool_call.name.to_string();
    let extension_name = tool_request
        .tool_name_parts()
        .and_then(|parts| parts.extension_name)
        .map(ToString::to_string);

    let mut tool_call_meta = serde_json::Map::new();
    tool_call_meta.insert("toolName".to_string(), serde_json::Value::String(tool_name));
    if let Some(extension_name) = extension_name {
        tool_call_meta.insert(
            "extensionName".to_string(),
            serde_json::Value::String(extension_name),
        );
    }

    let mut goose_meta = serde_json::Map::new();
    goose_meta.insert(
        "toolCall".to_string(),
        serde_json::Value::Object(tool_call_meta),
    );

    let mut meta = serde_json::Map::new();
    meta.insert("goose".to_string(), serde_json::Value::Object(goose_meta));
    Some(meta)
}

fn build_initial_tool_call(tool_request: &ToolRequest, include_generated_title: bool) -> ToolCall {
    let tool_name = match &tool_request.tool_call {
        Ok(tool_call) => tool_call.name.to_string(),
        Err(_) => "error".to_string(),
    };
    let args_value = tool_request
        .tool_call
        .as_ref()
        .ok()
        .and_then(|tc| tc.arguments.as_ref())
        .map(|a| serde_json::Value::Object(a.clone()));
    let default_tool_call_title = default_tool_title(&tool_name, args_value.as_ref());
    let goose_meta = goose_tool_call_meta(tool_request);

    let initial_title = if include_generated_title {
        tool_request
            .generated_title()
            .map(str::to_string)
            .unwrap_or(default_tool_call_title)
    } else {
        default_tool_call_title
    };

    let mut tool_call = ToolCall::new(ToolCallId::new(tool_request.id.clone()), initial_title)
        .status(ToolCallStatus::Pending);
    if let Some(args) = args_value {
        tool_call = tool_call.raw_input(args);
    }

    tool_call.meta(goose_meta)
}

pub(crate) fn build_initial_tool_call_with_message_meta(
    tool_request: &ToolRequest,
    message: &Message,
    include_generated_title: bool,
) -> ToolCall {
    let mut tool_call = build_initial_tool_call(tool_request, include_generated_title);
    let meta = tool_call.meta.take().unwrap_or_default();
    tool_call.meta(merge_message_meta(meta, message))
}

pub(crate) fn build_permission_tool_call_update(
    request_id: &str,
    tool_name: &str,
    arguments: serde_json::Map<String, serde_json::Value>,
    prompt: Option<String>,
) -> ToolCallUpdate {
    let arguments = serde_json::Value::Object(arguments);
    let mut fields = ToolCallUpdateFields::new()
        .title(default_tool_title(tool_name, Some(&arguments)))
        .kind(ToolKind::default())
        .status(ToolCallStatus::Pending)
        .raw_input(arguments);

    if let Some(prompt) = prompt {
        fields = fields.content(vec![ToolCallContent::Content(Content::new(
            ContentBlock::Text(TextContent::new(prompt)),
        ))]);
    }

    ToolCallUpdate::new(ToolCallId::new(request_id), fields)
}

fn json_u32(value: &serde_json::Value) -> Option<u32> {
    value.as_u64().and_then(|value| u32::try_from(value).ok())
}

fn extract_tool_locations_from_response(
    tool_response: &ToolResponse,
) -> Option<Vec<ToolCallLocation>> {
    let result = tool_response.tool_result.as_ref().ok()?;
    let meta = result.meta.as_ref()?;
    let locations_val = meta.get("tool_locations")?;
    let entries: Vec<serde_json::Value> = serde_json::from_value(locations_val.clone()).ok()?;
    let locations = entries
        .into_iter()
        .filter_map(|entry| {
            let path = entry.get("path")?.as_str()?;
            let line = entry.get("line").and_then(json_u32);
            Some(ToolCallLocation::new(path).line(line))
        })
        .collect::<Vec<_>>();
    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

fn extract_tool_locations_from_request(tool_request: &ToolRequest) -> Vec<ToolCallLocation> {
    let Some(parts) = tool_request.tool_name_parts() else {
        return Vec::new();
    };
    if parts.extension_name != Some("developer") {
        return Vec::new();
    }

    let Ok(tool_call) = &tool_request.tool_call else {
        return Vec::new();
    };
    let Some(path) = tool_call
        .arguments
        .as_ref()
        .and_then(|args| args.get("path"))
        .and_then(|path| path.as_str())
    else {
        return Vec::new();
    };

    let line = match parts.tool_name {
        "read" => tool_call
            .arguments
            .as_ref()
            .and_then(|arguments| arguments.get("line"))
            .and_then(json_u32),
        "write" | "edit" => Some(1),
        _ => return Vec::new(),
    };
    vec![ToolCallLocation::new(path).line(line)]
}

pub(crate) fn trusted_update_meta(tool_response: &ToolResponse) -> Option<Meta> {
    let tool_result = tool_response.tool_result.as_ref().ok()?;
    let goose_meta = tool_result
        .meta
        .as_ref()?
        .0
        .get(TRUSTED_TOOL_UPDATE_META_KEY)?
        .clone();
    let mut meta_map = serde_json::Map::new();
    meta_map.insert("goose".to_string(), goose_meta);
    Some(meta_map)
}

fn build_tool_call_content(tool_result: &ToolResult<CallToolResult>) -> Vec<ToolCallContent> {
    match tool_result {
        Ok(result) => result
            .content
            .iter()
            .filter_map(|content| match content {
                RmcpContentBlock::Text(val) => Some(ToolCallContent::Content(Content::new(
                    ContentBlock::Text(TextContent::new(val.text.clone())),
                ))),
                RmcpContentBlock::Image(val) => Some(ToolCallContent::Content(Content::new(
                    ContentBlock::Image(ImageContent::new(val.data.clone(), val.mime_type.clone())),
                ))),
                RmcpContentBlock::Resource(val) => {
                    let resource = match &val.resource {
                        ResourceContents::TextResourceContents {
                            mime_type,
                            text,
                            uri,
                            ..
                        } => EmbeddedResourceResource::TextResourceContents(
                            TextResourceContents::new(text.clone(), uri.clone())
                                .mime_type(mime_type.clone()),
                        ),
                        ResourceContents::BlobResourceContents {
                            mime_type,
                            blob,
                            uri,
                            ..
                        } => EmbeddedResourceResource::BlobResourceContents(
                            BlobResourceContents::new(blob.clone(), uri.clone())
                                .mime_type(mime_type.clone()),
                        ),
                        _ => return None,
                    };
                    Some(ToolCallContent::Content(Content::new(
                        ContentBlock::Resource(EmbeddedResource::new(resource)),
                    )))
                }
                RmcpContentBlock::Audio(_) | RmcpContentBlock::ResourceLink(_) => None,
                _ => None,
            })
            .collect(),
        Err(error) => vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
            TextContent::new(error.message.to_string()),
        )))],
    }
}

fn extract_tool_raw_output(tool_result: &ToolResult<CallToolResult>) -> Option<serde_json::Value> {
    tool_result
        .as_ref()
        .ok()
        .and_then(|result| result.structured_content.clone())
}

pub(crate) fn tool_call_update_fields_from_response(
    tool_response: &ToolResponse,
    tool_request: Option<&ToolRequest>,
    include_content_for_acp_aware_tools: bool,
) -> ToolCallUpdateFields {
    let is_failed = match &tool_response.tool_result {
        Ok(result) => result.is_error == Some(true),
        Err(_) => true,
    };
    let status = if is_failed {
        ToolCallStatus::Failed
    } else {
        ToolCallStatus::Completed
    };

    let mut fields = ToolCallUpdateFields::new().status(status);
    if let Some(raw_output) = extract_tool_raw_output(&tool_response.tool_result) {
        fields = fields.raw_output(raw_output);
    }
    let is_acp_aware = tool_response
        .tool_result
        .as_ref()
        .is_ok_and(|result| result.is_acp_aware());
    let include_content = include_content_for_acp_aware_tools || is_failed || !is_acp_aware;
    let include_locations = !is_acp_aware;

    if include_content {
        fields = fields.content(build_tool_call_content(&tool_response.tool_result));
    }

    if include_locations {
        let locations = extract_tool_locations_from_response(tool_response).unwrap_or_else(|| {
            tool_request
                .map(extract_tool_locations_from_request)
                .unwrap_or_default()
        });
        if !locations.is_empty() {
            fields = fields.locations(locations);
        }
    }

    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{CallToolRequestParams, ContentBlock as RmcpContent};
    use std::path::PathBuf;
    use test_case::test_case;

    mod format_tool_name {
        use super::*;

        #[test]
        fn with_extension() {
            assert_eq!(
                format_tool_name("platform__manage_extensions"),
                "platform: manage extensions"
            );
        }

        #[test]
        fn without_extension() {
            assert_eq!(format_tool_name("simple_tool"), "simple tool");
        }
    }

    mod default_tool_title {
        use super::*;

        #[test]
        fn no_args() {
            assert_eq!(
                default_tool_title("developer__shell", None),
                "developer: shell"
            );
        }

        #[test]
        fn with_command() {
            let args = serde_json::json!({"command": "cargo build"});
            assert_eq!(
                default_tool_title("developer__shell", Some(&args)),
                "developer: shell · cargo build"
            );
        }

        #[test]
        fn shell_command_takes_precedence_over_ignored_path() {
            let args = serde_json::json!({
                "path": "/tmp/harmless",
                "command": "rm -rf /tmp/important"
            });
            assert_eq!(
                default_tool_title("developer__shell", Some(&args)),
                "developer: shell · rm -rf /tmp/important"
            );
            assert_eq!(
                default_tool_title("shell", Some(&args)),
                "shell · rm -rf /tmp/important"
            );
        }

        #[test]
        fn multiline_shell_command_marks_omitted_lines() {
            let args = serde_json::json!({"command": "echo harmless\nrm -rf /tmp/important"});
            assert_eq!(
                default_tool_title("developer__shell", Some(&args)),
                "developer: shell · echo harmless…"
            );
        }

        #[test]
        fn long_value_is_truncated() {
            let long_path = "a".repeat(80);
            let args = serde_json::json!({"path": long_path});
            let result = default_tool_title("developer__read_file", Some(&args));
            assert!(result.ends_with('…'));
            assert!(result.len() < 90);
        }
    }

    mod build_initial_tool_call {
        use super::*;
        use crate::conversation::message::TOOL_META_TITLE_KEY;
        use rmcp::model::ErrorData;

        #[test]
        fn uses_default_title_and_preserves_request_data() {
            let arguments = json_object(vec![("path", serde_json::json!("/src/main.rs"))]);
            let request = ToolRequest {
                id: "req_1".to_string(),
                tool_call: Ok(CallToolRequestParams::new("edit").with_arguments(arguments.clone())),
                metadata: None,
                tool_meta: Some(serde_json::json!({"goose_extension": "developer"})),
            };

            let tool_call = build_initial_tool_call(&request, false);

            assert_eq!(tool_call.title, "edit · /src/main.rs");
            assert_eq!(tool_call.status, ToolCallStatus::Pending);
            assert_eq!(
                tool_call.raw_input,
                Some(serde_json::Value::Object(arguments))
            );
            assert_eq!(
                tool_call.meta.as_ref().and_then(|meta| meta.get("goose")),
                Some(&serde_json::json!({
                    "toolCall": {
                        "toolName": "edit",
                        "extensionName": "developer",
                    },
                }))
            );
        }

        #[test]
        fn uses_generated_title_when_enrichment_is_enabled() {
            let arguments = json_object(vec![("command", serde_json::json!("cargo test"))]);
            let request = ToolRequest {
                id: "req_1".to_string(),
                tool_call: Ok(
                    CallToolRequestParams::new("developer__shell").with_arguments(arguments)
                ),
                metadata: None,
                tool_meta: Some(serde_json::json!({
                    (TOOL_META_TITLE_KEY): "running focused tests",
                })),
            };

            let tool_call = build_initial_tool_call(&request, true);

            assert_eq!(tool_call.title, "running focused tests");
        }

        #[test]
        fn uses_default_title_when_enrichment_is_disabled() {
            let arguments = json_object(vec![("command", serde_json::json!("cargo test"))]);
            let request = ToolRequest {
                id: "req_1".to_string(),
                tool_call: Ok(
                    CallToolRequestParams::new("developer__shell").with_arguments(arguments)
                ),
                metadata: None,
                tool_meta: Some(serde_json::json!({
                    (TOOL_META_TITLE_KEY): "running focused tests",
                })),
            };

            let tool_call = build_initial_tool_call(&request, false);

            assert_eq!(tool_call.title, "developer: shell · cargo test");
        }

        #[test]
        fn handles_invalid_request() {
            let request = ToolRequest {
                id: "req_1".to_string(),
                tool_call: Err(ErrorData::invalid_request("invalid tool call", None)),
                metadata: None,
                tool_meta: None,
            };

            let tool_call = build_initial_tool_call(&request, false);

            assert_eq!(tool_call.title, "error");
            assert_eq!(tool_call.status, ToolCallStatus::Pending);
            assert_eq!(tool_call.raw_input, None);
            assert_eq!(tool_call.meta, None);
        }
    }

    mod build_initial_tool_call_with_message_meta {
        use super::*;
        use rmcp::model::Role;

        #[test]
        fn merges_message_and_tool_metadata() {
            let request = ToolRequest {
                id: "req_1".to_string(),
                tool_call: Ok(CallToolRequestParams::new("developer__shell")),
                metadata: None,
                tool_meta: None,
            };
            let message = Message::new(Role::Assistant, 1_700_000_000, vec![]).with_id("msg_live");

            let tool_call = build_initial_tool_call_with_message_meta(&request, &message, false);
            assert_eq!(
                tool_call.meta.as_ref().and_then(|meta| meta.get("goose")),
                Some(&serde_json::json!({
                    "created": 1_700_000_000,
                    "messageId": "msg_live",
                    "toolCall": {
                        "extensionName": "developer",
                        "toolName": "developer__shell",
                    },
                })),
            );

            let mut limited_message = message;
            limited_message.metadata.output_token_limit_reached = true;
            let limited_tool_call =
                build_initial_tool_call_with_message_meta(&request, &limited_message, false);
            assert_eq!(
                limited_tool_call
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.get("goose")),
                Some(&serde_json::json!({
                    "created": 1_700_000_000,
                    "messageId": "msg_live",
                    "outputTokenLimitReached": true,
                    "toolCall": {
                        "extensionName": "developer",
                        "toolName": "developer__shell",
                    },
                })),
            );
        }
    }

    mod build_permission_tool_call_update {
        use super::*;

        #[test]
        fn matches_initial_request_presentation() {
            let arguments = json_object(vec![("command", serde_json::json!("cargo test"))]);
            let request = ToolRequest {
                id: "req_1".to_string(),
                tool_call: Ok(CallToolRequestParams::new("developer__shell")
                    .with_arguments(arguments.clone())),
                metadata: None,
                tool_meta: None,
            };
            let initial = build_initial_tool_call(&request, false);

            let permission = build_permission_tool_call_update(
                &request.id,
                "developer__shell",
                arguments,
                Some("Allow this command?".to_string()),
            );

            assert_eq!(
                permission.fields.title.as_deref(),
                Some(initial.title.as_str())
            );
            assert_eq!(permission.fields.raw_input, initial.raw_input);
            assert_eq!(permission.fields.status, Some(ToolCallStatus::Pending));
            assert_eq!(
                first_tool_call_text(&permission.fields),
                Some("Allow this command?")
            );
        }
    }

    mod goose_tool_call_meta {
        use super::*;

        #[test]
        fn prefers_goose_extension_metadata_over_name_prefix() {
            let request = ToolRequest {
                id: "req_1".to_string(),
                tool_call: Ok(CallToolRequestParams::new("other__query-docs")),
                metadata: None,
                tool_meta: Some(serde_json::json!({"goose_extension": "context7"})),
            };

            let meta = goose_tool_call_meta(&request).expect("expected metadata");

            assert_eq!(
                meta.get("goose"),
                Some(&serde_json::json!({
                    "toolCall": {
                        "toolName": "other__query-docs",
                        "extensionName": "context7",
                    },
                })),
            );
        }
    }

    fn json_object(pairs: Vec<(&str, serde_json::Value)>) -> rmcp::model::JsonObject {
        pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    mod extract_tool_locations_from_request {
        use super::*;
        use test_case::test_case;

        fn request(name: &str, arguments: serde_json::Value) -> ToolRequest {
            ToolRequest {
                id: "req_1".to_string(),
                tool_call: Ok(CallToolRequestParams::new(name.to_string()).with_arguments(
                    arguments
                        .as_object()
                        .expect("test arguments should be an object")
                        .clone(),
                )),
                metadata: None,
                tool_meta: None,
            }
        }

        fn locations(request: &ToolRequest) -> Vec<(PathBuf, Option<u32>)> {
            super::extract_tool_locations_from_request(request)
                .into_iter()
                .map(|location| (location.path, location.line))
                .collect()
        }

        #[test]
        fn reads_requested_line() {
            let request = request(
                "developer__read",
                serde_json::json!({"path": "/tmp/f.txt", "line": 5}),
            );

            assert_eq!(
                locations(&request),
                vec![(PathBuf::from("/tmp/f.txt"), Some(5))]
            );
        }

        #[test_case(serde_json::json!({"path": "/tmp/f.txt"}); "missing line")]
        #[test_case(serde_json::json!({"path": "/tmp/f.txt", "line": "not_a_number"}); "invalid line")]
        fn reads_without_valid_line(arguments: serde_json::Value) {
            let request = request("developer__read", arguments);

            assert_eq!(
                locations(&request),
                vec![(PathBuf::from("/tmp/f.txt"), None)]
            );
        }

        #[test_case("developer__write"; "write")]
        #[test_case("developer__edit"; "edit")]
        fn writes_and_edits_start_at_line_one(name: &str) {
            let request = request(name, serde_json::json!({"path": "/tmp/f.txt"}));

            assert_eq!(
                locations(&request),
                vec![(PathBuf::from("/tmp/f.txt"), Some(1))]
            );
        }

        #[test]
        fn accepts_unprefixed_developer_tool_from_metadata() {
            let mut request = request("write", serde_json::json!({"path": "/tmp/f.txt"}));
            request.tool_meta = Some(serde_json::json!({"goose_extension": "developer"}));

            assert_eq!(
                locations(&request),
                vec![(PathBuf::from("/tmp/f.txt"), Some(1))]
            );
        }

        #[test_case("other__read"; "other extension read")]
        #[test_case("other__write"; "other extension write")]
        #[test_case("other__edit"; "other extension edit")]
        #[test_case("read"; "unqualified read")]
        #[test_case("write"; "unqualified write")]
        #[test_case("edit"; "unqualified edit")]
        #[test_case("developer__shell"; "non file developer tool")]
        fn rejects_tools_without_developer_ownership(name: &str) {
            let request = request(name, serde_json::json!({"path": "/tmp/f.txt"}));

            assert!(locations(&request).is_empty());
        }
    }

    fn response_with_meta(meta: Option<serde_json::Value>) -> ToolResponse {
        let mut result = CallToolResult::success(vec![RmcpContent::text("")]);
        result.meta = meta.map(|v| serde_json::from_value(v).unwrap());
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(result),
            metadata: None,
        }
    }

    #[test_case(
        response_with_meta(Some(serde_json::json!({"tool_locations": [{"path": "/tmp/f.txt"}]})))
        => Some(vec![(PathBuf::from("/tmp/f.txt"), None)])
        ; "meta with path no line"
    )]
    #[test_case(
        response_with_meta(None)
        => None
        ; "no meta"
    )]
    fn extracts_tool_locations_from_response(
        response: ToolResponse,
    ) -> Option<Vec<(PathBuf, Option<u32>)>> {
        extract_tool_locations_from_response(&response)
            .map(|locs| locs.into_iter().map(|loc| (loc.path, loc.line)).collect())
    }

    mod trusted_update_meta {
        use super::*;

        #[test]
        fn ignores_untrusted_goose_meta() {
            let response = response_with_meta(Some(serde_json::json!({
                "goose": {
                    "mcpApp": {
                        "resourceUri": "ui://spoofed/app",
                    },
                },
            })));

            assert_eq!(trusted_update_meta(&response), None);
        }

        #[test]
        fn uses_trusted_meta_only() {
            let response = response_with_meta(Some(serde_json::json!({
                "goose": {
                    "mcpApp": {
                        "resourceUri": "ui://spoofed/app",
                    },
                },
                TRUSTED_TOOL_UPDATE_META_KEY: {
                    "mcpApp": {
                        "resourceUri": "ui://trusted/app",
                        "extensionName": "weather",
                        "toolName": "weather__render",
                    },
                },
            })));

            let extracted = trusted_update_meta(&response).expect("expected trusted meta");
            assert_eq!(
                extracted.get("goose"),
                Some(&serde_json::json!({
                    "mcpApp": {
                        "resourceUri": "ui://trusted/app",
                        "extensionName": "weather",
                        "toolName": "weather__render",
                    },
                })),
            );
        }
    }

    fn response_from_tool_result(tool_result: ToolResult<CallToolResult>) -> ToolResponse {
        ToolResponse {
            id: "req_1".to_string(),
            tool_result,
            metadata: None,
        }
    }

    fn write_request(path: &str) -> ToolRequest {
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(
                CallToolRequestParams::new("write").with_arguments(json_object(vec![
                    ("path", serde_json::json!(path)),
                    ("content", serde_json::json!("updated")),
                ])),
            ),
            metadata: None,
            tool_meta: Some(serde_json::json!({"goose_extension": "developer"})),
        }
    }

    fn first_tool_call_text(fields: &ToolCallUpdateFields) -> Option<&str> {
        fields.content.as_ref()?.iter().find_map(|content| {
            let ToolCallContent::Content(content) = content else {
                return None;
            };
            let ContentBlock::Text(text) = &content.content else {
                return None;
            };
            Some(text.text.as_str())
        })
    }

    mod tool_call_update_fields_from_response {
        use super::*;

        #[test]
        fn includes_ordinary_success_details() {
            let raw_output = serde_json::json!({ "changed": true });
            let mut result = CallToolResult::success(vec![RmcpContent::text("write completed")]);
            result.structured_content = Some(raw_output.clone());
            let response = response_from_tool_result(Ok(result));
            let request = write_request("/tmp/request.txt");

            let fields = tool_call_update_fields_from_response(&response, Some(&request), false);

            assert_eq!(fields.status, Some(ToolCallStatus::Completed));
            assert_eq!(fields.raw_output, Some(raw_output));
            assert_eq!(first_tool_call_text(&fields), Some("write completed"));
            let locations = fields.locations.as_deref().expect("expected location");
            assert_eq!(locations.len(), 1);
            assert_eq!(locations[0].path, PathBuf::from("/tmp/request.txt"));
            assert_eq!(locations[0].line, Some(1));
        }

        #[test]
        fn includes_ordinary_error_content() {
            let response =
                response_from_tool_result(Ok(CallToolResult::error(vec![RmcpContent::text(
                    "write failed",
                )])));

            let fields = tool_call_update_fields_from_response(&response, None, false);

            assert_eq!(fields.status, Some(ToolCallStatus::Failed));
            assert_eq!(first_tool_call_text(&fields), Some("write failed"));
            assert!(fields.locations.is_none());
        }

        #[test]
        fn suppresses_acp_aware_success_details() {
            let raw_output = serde_json::json!({ "changed": true });
            let mut result = CallToolResult::success(vec![RmcpContent::text("write completed")]);
            result.structured_content = Some(raw_output.clone());
            let response = response_from_tool_result(Ok(result.with_acp_aware_meta()));
            let request = write_request("/tmp/request.txt");

            let fields = tool_call_update_fields_from_response(&response, Some(&request), false);

            assert_eq!(fields.status, Some(ToolCallStatus::Completed));
            assert_eq!(fields.raw_output, Some(raw_output));
            assert!(fields.content.is_none());
            assert!(fields.locations.is_none());
        }

        #[test]
        fn includes_acp_aware_success_content_when_requested() {
            let result = CallToolResult::success(vec![RmcpContent::text("write completed")])
                .with_acp_aware_meta();
            let response = response_from_tool_result(Ok(result));

            let fields = tool_call_update_fields_from_response(&response, None, true);

            assert_eq!(fields.status, Some(ToolCallStatus::Completed));
            assert_eq!(first_tool_call_text(&fields), Some("write completed"));
        }

        #[test]
        fn prefers_explicit_location() {
            let response = response_with_meta(Some(serde_json::json!({
                "tool_locations": [{ "path": "/tmp/response.txt", "line": 7 }]
            })));
            let request = write_request("/tmp/request.txt");

            let fields = tool_call_update_fields_from_response(&response, Some(&request), false);

            let locations = fields.locations.as_deref().expect("expected location");
            assert_eq!(locations.len(), 1);
            assert_eq!(locations[0].path, PathBuf::from("/tmp/response.txt"));
            assert_eq!(locations[0].line, Some(7));
        }

        #[test]
        fn includes_acp_aware_error_content() {
            let result = CallToolResult::error(vec![RmcpContent::text("write failed")])
                .with_acp_aware_meta();
            let response = response_from_tool_result(Ok(result));
            let request = write_request("/tmp/request.txt");

            let fields = tool_call_update_fields_from_response(&response, Some(&request), false);

            assert_eq!(fields.status, Some(ToolCallStatus::Failed));
            assert_eq!(first_tool_call_text(&fields), Some("write failed"));
            assert!(fields.locations.is_none());
        }

        #[test]
        fn includes_transport_error_content() {
            let response = response_from_tool_result(Err(rmcp::model::ErrorData::new(
                rmcp::model::ErrorCode::INTERNAL_ERROR,
                "transport failed",
                None,
            )));

            let fields = tool_call_update_fields_from_response(&response, None, false);

            assert_eq!(fields.status, Some(ToolCallStatus::Failed));
            assert_eq!(first_tool_call_text(&fields), Some("transport failed"));
        }
    }
}
