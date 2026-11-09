#![cfg(feature = "mcp-interop")]

use goose_provider_types::{data, runtime::Shared};
use serde_json::json;

#[test]
fn tool_schema_annotations_and_metadata_round_trip() {
    let tool = data::Tool::new(
        "sensor",
        "Read the device sensor",
        Shared::new(
            serde_json::from_value(
                json!({"type":"object", "properties":{"pin":{"type":"integer"}}}),
            )
            .unwrap(),
        ),
    )
    .with_annotations(data::ToolAnnotations::new().read_only(true))
    .with_meta(data::MetaObject(
        serde_json::from_value(json!({"goose_extension":"device", "future":{"enabled":true}}))
            .unwrap(),
    ));
    let serialized = serde_json::to_value(&tool).unwrap();
    let mcp = rmcp::model::Tool::try_from(tool).unwrap();
    assert_eq!(serde_json::to_value(&mcp).unwrap(), serialized);
    assert_eq!(
        serde_json::to_value(data::Tool::try_from(mcp).unwrap()).unwrap(),
        serialized
    );
}

#[test]
fn multimodal_tool_result_preserves_content_and_metadata() {
    let result = data::CallToolResult::success(vec![
        data::ContentBlock::text("reading"),
        data::ContentBlock::image("aGVsbG8=", "image/png"),
        data::ContentBlock::embedded_text("sensor://local", "21 C"),
    ])
    .with_meta(Some(data::MetaObject(
        serde_json::from_value(json!({"provider":"custom", "timing":12})).unwrap(),
    )));
    let serialized = serde_json::to_value(&result).unwrap();
    let mcp = rmcp::model::CallToolResult::try_from(result).unwrap();
    assert_eq!(serde_json::to_value(&mcp).unwrap(), serialized);
    assert_eq!(
        serde_json::to_value(data::CallToolResult::try_from(mcp).unwrap()).unwrap(),
        serialized
    );
}

#[test]
fn transport_errors_keep_codes_details_and_data() {
    let mcp = rmcp::ErrorData::invalid_params("bad input", Some(json!({"pin":-1})));
    let serialized = serde_json::to_value(&mcp).unwrap();
    let portable = data::ErrorData::from(mcp);
    assert_eq!(portable.code, data::ErrorCode::INVALID_PARAMS);
    assert_eq!(
        serde_json::to_value(rmcp::ErrorData::from(portable)).unwrap(),
        serialized
    );
}
