use goose_acp::server::GooseAcpAgent;
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

    // Replace `true` with `{}` throughout $defs. Both mean "accept any value" in
    // JSON Schema, but many TS codegen tools (e.g. @hey-api/openapi-ts Zod plugin)
    // silently drop properties whose schema is the bare `true` literal.
    for def in defs.values_mut() {
        replace_true_schemas(def);
    }

    // Annotate $defs entries with x-method/x-side. Only set x-method for types
    // used by exactly one method (shared types like EmptyResponse skip x-method).
    for (name, methods_list) in &type_methods {
        if let Some(def) = defs.get_mut(name) {
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
            request_variants.push(json!({
                "allOf": [{ "$ref": format!("#/$defs/{name}") }],
                "description": format!("Params for {}", m.method),
                "title": name,
            }));
        }

        if let Some(name) = &m.response_type_name {
            if seen_response_types.insert(name.clone()) {
                response_variants.push(json!({
                    "allOf": [{ "$ref": format!("#/$defs/{name}") }],
                    "title": name,
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
                "requestType": m.params_type_name,
                "responseType": m.response_type_name,
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
