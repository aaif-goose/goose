use goose_apple_foundation_models::tool_schema;
use serde_json::{json, Value};

#[test]
fn nested_schemas_keep_order_required_fields_and_constraints() {
    let schema = json!({"type":"object", "required":["items"], "properties":{
        "items":{"type":"array", "items":{"type":"object", "properties":{
            "name":{"type":"string", "enum":["first", "second"]}
        }, "required":["name"]}}
    }});
    let result: Value = serde_json::from_str(&tool_schema(&schema, "Arguments").unwrap()).unwrap();
    assert_eq!(result["x-order"], json!(["items"]));
    assert_eq!(result["required"], json!(["items"]));
    assert_eq!(
        result["properties"]["items"]["items"]["x-order"],
        json!(["name"])
    );
    assert_eq!(
        result["properties"]["items"]["items"]["properties"]["name"]["enum"],
        json!(["first", "second"])
    );
}

#[test]
fn unsupported_schema_semantics_fail_instead_of_being_discarded() {
    assert!(tool_schema(&json!({"type":"object","additionalProperties":true}), "Map").is_err());
    assert!(tool_schema(&json!({"type":"object","allOf":[]}), "All").is_err());
    assert!(tool_schema(&json!({"type":"array","items":{"type":"string"}}), "Array").is_err());
}

#[test]
fn nullable_schemars_parameters_become_named_unions() {
    let schema = json!({"type":"object", "properties": {
        "line": {"type":["integer","null"],"minimum":1},
        "mode": {"type":"string","enum":["read","write"]}
    }});
    let result: Value = serde_json::from_str(&tool_schema(&schema, "Read").unwrap()).unwrap();
    assert_eq!(result["properties"]["line"]["title"], "Read_line");
    assert_eq!(result["properties"]["line"]["anyOf"][0]["type"], "integer");
    assert_eq!(result["properties"]["line"]["anyOf"][0]["minimum"], 1);
    assert_eq!(result["properties"]["line"]["anyOf"][1]["type"], "null");
    assert!(tool_schema(
        &json!({"type":"object","properties":{"s":{"type":"string","minLength":1}}}),
        "A"
    )
    .is_err());
}

#[test]
fn format_annotations_keep_hints_and_numeric_constraints_in_nullable_parameters() {
    let schema = json!({"type":"object", "properties": {
        "timeout_secs": {
            "type":["integer","null"], "format":"uint64", "minimum":0,
            "description":"Timeout in seconds"
        }
    }});
    let result: Value = serde_json::from_str(&tool_schema(&schema, "Shell").unwrap()).unwrap();
    let timeout = &result["properties"]["timeout_secs"]["anyOf"][0];
    assert_eq!(timeout["type"], "integer");
    assert_eq!(timeout["minimum"], 0);
    assert_eq!(
        timeout["description"],
        "Timeout in seconds\nFormat: uint64."
    );
    assert!(timeout.get("format").is_none());
}

#[test]
fn nested_string_formats_become_hints_without_changing_properties_named_format() {
    let schema = json!({"type":"object", "properties": {
        "format": {"type":"string", "enum":["json","text"]},
        "links": {"type":"array", "items":{"type":"string", "format":"uri"}},
        "time": {"anyOf":[{"type":"string", "format":"date-time"}, {"type":"null"}]}
    }});
    let result: Value = serde_json::from_str(&tool_schema(&schema, "Links").unwrap()).unwrap();
    assert_eq!(
        result["properties"]["format"]["enum"],
        json!(["json", "text"])
    );
    assert_eq!(
        result["properties"]["links"]["items"]["description"],
        "Format: uri."
    );
    assert_eq!(
        result["properties"]["time"]["anyOf"][0]["description"],
        "Format: date-time."
    );
    assert!(tool_schema(
        &json!({"type":"object", "properties":{"bad":{"type":"string","format":42}}}),
        "Bad"
    )
    .is_err());
}
