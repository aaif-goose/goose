use goose::acp::server::GooseAcpAgent;
use schemars::SchemaGenerator;
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let mut generator = SchemaGenerator::default();
    let methods = GooseAcpAgent::custom_method_schemas(&mut generator);

    // Collect $defs from the generator (all types referenced via subschema_for).
    let mut defs: Map<String, Value> = generator
        .take_definitions(true)
        .into_iter()
        .map(|(k, v)| (k, serde_json::to_value(v).unwrap_or(json!({}))))
        .collect();

    // Track which types map to which methods so we can detect shared types.
    let mut type_methods: HashMap<String, Vec<String>> = HashMap::new();
    for m in &methods {
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
            if methods_list.iter().all(|method| is_unstable_method(method)) {
                Some(name.clone())
            } else {
                None
            }
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

    // Annotate $defs entries with x-method/x-side. Only set x-method for types
    // used by exactly one method (shared types like EmptyResponse skip x-method).
    for (name, methods_list) in &type_methods {
        let generated_name = generated_type_name(name, &unstable_type_names);
        if let Some(def) = defs.get_mut(&generated_name) {
            if let Some(obj) = def.as_object_mut() {
                obj.insert("x-side".into(), json!("agent"));
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
    let mut seen_response_types: BTreeSet<String> = BTreeSet::new();

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
    let meta = json!({ "methods": method_entries });
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

/// Recursively strip `"format"` from integer-typed schemas.
///
/// schemars emits `"format": "uint64"` / `"int64"` etc. for Rust integer types.
/// TS codegen tools interpret these as BigInt, but JS/TS uses `number` everywhere.
fn strip_integer_formats(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let is_integer = map.get("type").and_then(|v| v.as_str()) == Some("integer");
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
