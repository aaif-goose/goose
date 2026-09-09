use goose::acp::custom_notifications::custom_notification_schemas;
use goose::acp::server::{agent_request_schemas, GooseAcpAgent};
use schemars::SchemaGenerator;
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::PathBuf;

const STABLE_SCHEMA_TYPE_NAMES: &[&str] = &["EmptyResponse"];

fn main() {
    let mut generator = SchemaGenerator::default();
    let methods = GooseAcpAgent::custom_method_schemas(&mut generator);
    let notifications = custom_notification_schemas(&mut generator);
    let agent_requests = agent_request_schemas(&mut generator);

    // Types used by agent → client requests are answered by the client, so
    // they're tagged `x-side: "client"` rather than `"agent"`.
    let client_side_type_names: BTreeSet<String> = agent_requests
        .iter()
        .flat_map(|m| {
            m.params_type_name
                .iter()
                .chain(m.response_type_name.iter())
                .cloned()
        })
        .collect();

    // Collect $defs from the generator (all types referenced via subschema_for).
    let mut defs: Map<String, Value> = generator
        .take_definitions(true)
        .into_iter()
        .map(|(k, v)| (k, serde_json::to_value(v).unwrap_or(json!({}))))
        .collect();

    // Track which types map to which methods so we can detect shared types.
    let mut type_methods: HashMap<String, Vec<String>> = HashMap::new();
    for m in methods
        .iter()
        .chain(notifications.iter())
        .chain(agent_requests.iter())
    {
        let method = m.method.clone();
        if let Some(name) = &m.params_type_name {
            type_methods
                .entry(name.clone())
                .or_default()
                .push(method.clone());
        }
        if let Some(name) = &m.response_type_name {
            type_methods
                .entry(name.clone())
                .or_default()
                .push(method.clone());
        }
    }

    let unstable_type_names: BTreeSet<String> = type_methods
        .iter()
        .filter_map(|(name, methods_list)| {
            let is_unstable = methods_list.iter().all(|method| is_unstable_method(method));
            let has_stable_name = STABLE_SCHEMA_TYPE_NAMES.contains(&name.as_str());

            (is_unstable && !has_stable_name).then(|| name.clone())
        })
        .collect();

    for def in defs.values_mut() {
        rewrite_unstable_schema_refs(def, &unstable_type_names);
    }

    let mut renamed_defs = Map::new();
    for (name, def) in defs {
        let generated_name = generated_type_name(&name, &unstable_type_names);
        let previous = renamed_defs.insert(generated_name.clone(), def);
        assert!(
            previous.is_none(),
            "duplicate schema definition name after unstable suffix: {generated_name}"
        );
    }
    let mut defs = renamed_defs;

    // Replace `true` with `{}` throughout $defs. Both mean "accept any value" in
    // JSON Schema, but many TS codegen tools (e.g. @hey-api/openapi-ts Zod plugin)
    // silently drop properties whose schema is the bare `true` literal.
    //
    // Also strip "format": "uint64" / "int64" from integer types — these cause TS
    // codegen to emit BigInt validators, but JS/TS uses `number` for all integers.
    for def in defs.values_mut() {
        replace_true_schemas(def);
        strip_integer_formats(def);
    }

    add_mcp_server_transport_discriminants(&mut defs);

    // Annotate $defs entries with x-method/x-side. Only set x-method for types
    // used by exactly one method (shared types like EmptyResponse skip x-method).
    for (name, methods_list) in &type_methods {
        let generated_name = generated_type_name(name, &unstable_type_names);
        if let Some(def) = defs.get_mut(&generated_name) {
            if let Some(obj) = def.as_object_mut() {
                let side = if client_side_type_names.contains(name) {
                    "client"
                } else {
                    "agent"
                };
                obj.insert("x-side".into(), json!(side));
                if methods_list.len() == 1 {
                    obj.insert("x-method".into(), json!(methods_list[0]));
                }
            }
        }
    }

    // Build ExtRequest.params and ExtResponse.result anyOf arrays,
    // deduplicating response variants (e.g. EmptyResponse appears once).
    let mut request_variants: Vec<Value> = Vec::new();
    let mut response_variants: Vec<Value> = Vec::new();
    let mut notification_variants: Vec<Value> = Vec::new();
    let mut agent_request_variants: Vec<Value> = Vec::new();
    let mut agent_response_variants: Vec<Value> = Vec::new();
    let mut seen_response_types: BTreeSet<String> = BTreeSet::new();
    let mut seen_agent_response_types: BTreeSet<String> = BTreeSet::new();

    for m in &methods {
        if let Some(name) = &m.params_type_name {
            let generated_name = generated_type_name(name, &unstable_type_names);
            request_variants.push(json!({
                "allOf": [{ "$ref": format!("#/$defs/{generated_name}") }],
                "description": format!("Params for {}", m.method),
                "title": generated_name,
            }));
        }

        if let Some(name) = &m.response_type_name {
            let generated_name = generated_type_name(name, &unstable_type_names);
            if seen_response_types.insert(generated_name.clone()) {
                response_variants.push(json!({
                    "allOf": [{ "$ref": format!("#/$defs/{generated_name}") }],
                    "title": generated_name,
                }));
            }
        }
    }

    for n in &notifications {
        if let Some(name) = &n.params_type_name {
            let generated_name = generated_type_name(name, &unstable_type_names);
            notification_variants.push(json!({
                "allOf": [{ "$ref": format!("#/$defs/{generated_name}") }],
                "description": format!("Params for {}", n.method),
                "title": generated_name,
            }));
        }
    }

    for m in &agent_requests {
        if let Some(name) = &m.params_type_name {
            let generated_name = generated_type_name(name, &unstable_type_names);
            agent_request_variants.push(json!({
                "allOf": [{ "$ref": format!("#/$defs/{generated_name}") }],
                "description": format!("Params for {}", m.method),
                "title": generated_name,
            }));
        }

        if let Some(name) = &m.response_type_name {
            let generated_name = generated_type_name(name, &unstable_type_names);
            if seen_agent_response_types.insert(generated_name.clone()) {
                agent_response_variants.push(json!({
                    "allOf": [{ "$ref": format!("#/$defs/{generated_name}") }],
                    "title": generated_name,
                }));
            }
        }
    }

    // Build ExtRequest — mirrors AgentRequest structure.
    defs.insert(
        "ExtRequest".into(),
        json!({
            "properties": {
                "id": { "type": "string" },
                "method": { "type": "string" },
                "params": {
                    "anyOf": [
                        { "anyOf": request_variants },
                        { "description": "Untyped params", "type": ["object", "null"] },
                    ]
                }
            },
            "required": ["id", "method"],
            "type": "object",
            "x-docs-ignore": true,
        }),
    );

    // Build ExtResponse — mirrors AgentResponse structure.
    defs.insert(
        "ExtResponse".into(),
        json!({
            "anyOf": [
                {
                    "properties": {
                        "id": { "type": "string" },
                        "result": {
                            "anyOf": [
                                { "anyOf": response_variants },
                                { "description": "Untyped result" },
                            ]
                        }
                    },
                    "required": ["id"],
                    "title": "Success",
                    "type": "object",
                },
                {
                    "properties": {
                        "error": {
                            "type": "object",
                            "properties": {
                                "code": { "type": "integer" },
                                "message": { "type": "string" },
                                "data": {}
                            },
                            "required": ["code", "message"],
                        },
                        "id": { "type": "string" },
                    },
                    "required": ["id", "error"],
                    "title": "Error",
                    "type": "object",
                }
            ],
            "x-docs-ignore": true,
        }),
    );

    // Build ExtNotification — fire-and-forget message with no `id` and no response.
    defs.insert(
        "ExtNotification".into(),
        json!({
            "properties": {
                "method": { "type": "string" },
                "params": {
                    "anyOf": [
                        { "anyOf": notification_variants },
                        { "description": "Untyped params", "type": ["object", "null"] },
                    ]
                }
            },
            "required": ["method"],
            "type": "object",
            "x-docs-ignore": true,
        }),
    );

    // Build ExtAgentRequest — server-initiated request (agent → client),
    // structurally identical to ExtRequest but answered by the client.
    defs.insert(
        "ExtAgentRequest".into(),
        json!({
            "properties": {
                "id": { "type": "string" },
                "method": { "type": "string" },
                "params": {
                    "anyOf": [
                        { "anyOf": agent_request_variants },
                        { "description": "Untyped params", "type": ["object", "null"] },
                    ]
                }
            },
            "required": ["id", "method"],
            "type": "object",
            "x-docs-ignore": true,
        }),
    );

    // Build ExtAgentResponse — the client's reply (client → agent).
    defs.insert(
        "ExtAgentResponse".into(),
        json!({
            "anyOf": [
                {
                    "properties": {
                        "id": { "type": "string" },
                        "result": {
                            "anyOf": [
                                { "anyOf": agent_response_variants },
                                { "description": "Untyped result" },
                            ]
                        }
                    },
                    "required": ["id"],
                    "title": "Success",
                    "type": "object",
                },
                {
                    "properties": {
                        "error": {
                            "type": "object",
                            "properties": {
                                "code": { "type": "integer" },
                                "message": { "type": "string" },
                                "data": {}
                            },
                            "required": ["code", "message"],
                        },
                        "id": { "type": "string" },
                    },
                    "required": ["id", "error"],
                    "title": "Error",
                    "type": "object",
                }
            ],
            "x-docs-ignore": true,
        }),
    );

    // Assemble the root schema document.
    let root = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "GooseExtensions",
        "$defs": defs,
        "anyOf": [
            {
                "allOf": [{ "$ref": "#/$defs/ExtRequest" }],
                "description": "Extension request (client → agent)",
                "title": "Request",
            },
            {
                "allOf": [{ "$ref": "#/$defs/ExtResponse" }],
                "description": "Extension response (agent → client)",
                "title": "Response",
            },
            {
                "allOf": [{ "$ref": "#/$defs/ExtNotification" }],
                "description": "Extension notification (agent → client, fire-and-forget)",
                "title": "Notification",
            },
            {
                "allOf": [{ "$ref": "#/$defs/ExtAgentRequest" }],
                "description": "Extension agent request (agent → client)",
                "title": "AgentRequest",
            },
            {
                "allOf": [{ "$ref": "#/$defs/ExtAgentResponse" }],
                "description": "Extension agent response (client → agent)",
                "title": "AgentResponse",
            }
        ],
    });

    let json_str = serde_json::to_string_pretty(&root).expect("failed to serialize schema");

    let package_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let package_path = PathBuf::from(&package_dir);

    let schema_path = package_path.join("acp-schema.json");
    fs::write(&schema_path, format!("{json_str}\n")).expect("failed to write schema file");
    eprintln!("Generated ACP schema at {}", schema_path.display());

    // Build meta.json with method→type mappings (consumed by TS codegen).
    let method_entries: Vec<Value> = methods
        .iter()
        .map(|m| {
            json!({
                "method": &m.method,
                "requestType": m
                    .params_type_name
                    .as_ref()
                    .map(|name| generated_type_name(name, &unstable_type_names)),
                "responseType": m
                    .response_type_name
                    .as_ref()
                    .map(|name| generated_type_name(name, &unstable_type_names)),
            })
        })
        .collect();
    let notification_entries: Vec<Value> = notifications
        .iter()
        .map(|n| {
            json!({
                "method": &n.method,
                "paramsType": n
                    .params_type_name
                    .as_ref()
                    .map(|name| generated_type_name(name, &unstable_type_names)),
            })
        })
        .collect();
    let agent_request_entries: Vec<Value> = agent_requests
        .iter()
        .map(|m| {
            json!({
                "method": &m.method,
                "requestType": m
                    .params_type_name
                    .as_ref()
                    .map(|name| generated_type_name(name, &unstable_type_names)),
                "responseType": m
                    .response_type_name
                    .as_ref()
                    .map(|name| generated_type_name(name, &unstable_type_names)),
            })
        })
        .collect();
    let meta = json!({
        "methods": method_entries,
        "notifications": notification_entries,
        "agentRequests": agent_request_entries,
    });
    let meta_str = serde_json::to_string_pretty(&meta).expect("failed to serialize meta");
    let meta_path = package_path.join("acp-meta.json");
    fs::write(&meta_path, format!("{meta_str}\n")).expect("failed to write meta file");
    eprintln!("Generated ACP meta at {}", meta_path.display());

    println!("{json_str}");
}

fn is_unstable_method(method: &str) -> bool {
    method.contains("_goose/unstable")
}

fn generated_type_name(name: &str, unstable_type_names: &BTreeSet<String>) -> String {
    if unstable_type_names.contains(name) {
        format!("{name}_unstable")
    } else {
        name.to_string()
    }
}

fn rewrite_unstable_schema_refs(value: &mut Value, unstable_type_names: &BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get_mut("$ref") {
                if let Some(name) = reference.strip_prefix("#/$defs/") {
                    if unstable_type_names.contains(name) {
                        *reference = format!("#/$defs/{name}_unstable");
                    }
                }
            }
            for v in map.values_mut() {
                rewrite_unstable_schema_refs(v, unstable_type_names);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                rewrite_unstable_schema_refs(v, unstable_type_names);
            }
        }
        _ => {}
    }
}

fn add_mcp_server_transport_discriminants(defs: &mut Map<String, Value>) {
    add_object_discriminant(defs, "McpServerHttp", "http");
    add_object_discriminant(defs, "McpServerSse", "sse");
}

fn add_object_discriminant(defs: &mut Map<String, Value>, def_name: &str, tag: &str) {
    let def = defs
        .get_mut(def_name)
        .unwrap_or_else(|| panic!("missing {def_name} schema definition"));
    let obj = def
        .as_object_mut()
        .unwrap_or_else(|| panic!("{def_name} schema definition must be an object"));

    let properties = obj
        .entry("properties")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .unwrap_or_else(|| panic!("{def_name}.properties must be an object"));
    properties.insert(
        "type".into(),
        json!({
            "type": "string",
            "const": tag,
        }),
    );

    let required = obj
        .entry("required")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap_or_else(|| panic!("{def_name}.required must be an array"));
    if !required.iter().any(|item| item.as_str() == Some("type")) {
        required.insert(0, json!("type"));
    }
}

/// Recursively strip `"format"` from integer-typed schemas.
///
/// schemars emits `"format": "uint64"` / `"int64"` etc. for Rust integer types.
/// TS codegen tools interpret these as BigInt, but JS/TS uses `number` everywhere.
fn strip_integer_formats(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let is_integer = match map.get("type") {
                Some(Value::String(schema_type)) => schema_type == "integer",
                Some(Value::Array(schema_types)) => schema_types.iter().any(|schema_type| {
                    schema_type
                        .as_str()
                        .is_some_and(|schema_type| schema_type == "integer")
                }),
                _ => false,
            };
            if is_integer {
                map.remove("format");
            }
            for v in map.values_mut() {
                strip_integer_formats(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_integer_formats(v);
            }
        }
        _ => {}
    }
}

/// Recursively replace `true` with `{}` in a JSON value.
///
/// In JSON Schema, `true` and `{}` both mean "accept any value", but many
/// TypeScript codegen tools only handle the object form.
fn replace_true_schemas(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for v in map.values_mut() {
                if *v == Value::Bool(true) {
                    *v = json!({});
                } else {
                    replace_true_schemas(v);
                }
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                if *v == Value::Bool(true) {
                    *v = json!({});
                } else {
                    replace_true_schemas(v);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_http_and_sse_discriminants_without_tagging_stdio() {
        let mut defs = Map::from_iter([
            (
                "McpServerHttp".into(),
                json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    },
                    "required": ["name"]
                }),
            ),
            (
                "McpServerSse".into(),
                json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    },
                    "required": ["name"]
                }),
            ),
            (
                "McpServerStdio".into(),
                json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    },
                    "required": ["name"]
                }),
            ),
        ]);

        add_mcp_server_transport_discriminants(&mut defs);

        assert_eq!(
            defs["McpServerHttp"]["properties"]["type"],
            json!({ "type": "string", "const": "http" })
        );
        assert_eq!(
            defs["McpServerSse"]["properties"]["type"],
            json!({ "type": "string", "const": "sse" })
        );
        assert_eq!(defs["McpServerStdio"]["properties"].get("type"), None);
        assert!(defs["McpServerHttp"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("type")));
        assert!(defs["McpServerSse"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("type")));
    }

    #[test]
    fn strips_integer_formats_from_nullable_integer_schemas() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "timeout": {
                    "type": ["integer", "null"],
                    "format": "uint64",
                    "minimum": 0
                },
                "count": {
                    "type": "integer",
                    "format": "uint32",
                    "minimum": 0
                },
                "name": {
                    "type": "string",
                    "format": "custom"
                }
            }
        });

        strip_integer_formats(&mut schema);

        assert_eq!(
            schema["properties"]["timeout"].get("format"),
            None,
            "nullable integer formats should be stripped"
        );
        assert_eq!(
            schema["properties"]["count"].get("format"),
            None,
            "integer formats should be stripped"
        );
        assert_eq!(
            schema["properties"]["name"]["format"],
            json!("custom"),
            "non-integer formats should be preserved"
        );
    }
}
