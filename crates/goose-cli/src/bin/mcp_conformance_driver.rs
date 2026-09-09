use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Map, Value};

fn script_for_scenario(scenario: Option<&str>) -> Value {
    let context: Map<String, Value> = std::env::var("MCP_CONFORMANCE_CONTEXT")
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();

    let mut script = match scenario {
        Some("tools_call") => json!({
            "steps": [{ "action": "callTool", "name": "add_numbers", "arguments": { "a": 2, "b": 3 } }],
        }),
        Some("elicitation-sep1034-client-defaults") => json!({
            "steps": [{ "action": "callTool", "name": "test_client_elicitation_defaults", "arguments": {} }],
            "elicitation": { "action": "acceptSchemaDefaults" },
        }),
        Some("auth/scope-step-up") => json!({
            "steps": [{ "action": "callTool", "name": "test-tool", "arguments": {} }],
        }),
        Some("sse-retry") => json!({
            "steps": [{ "action": "callTool", "name": "test_reconnection", "arguments": {} }],
        }),
        Some("auth/basic-cimd") => json!({
            "steps": [{ "action": "listTools" }],
            "oauth": { "clientMetadataUrl": "https://conformance-test.local/client-metadata.json" },
        }),
        Some("auth/pre-registration") => json!({
            "steps": [{ "action": "listTools" }],
            "oauth": { "clientId": context.get("client_id"), "clientSecret": context.get("client_secret") },
        }),
        Some("sep-2322-client-request-state") => json!({
            "steps": [
                { "action": "callTool", "name": "test_mrtr_echo_state", "arguments": {} },
                { "action": "callTool", "name": "test_mrtr_no_state", "arguments": {} },
                { "action": "callTool", "name": "test_mrtr_unrelated", "arguments": {} },
                { "action": "callTool", "name": "test_mrtr_no_result_type", "arguments": {} },
            ],
            "elicitation": { "action": "accept", "content": { "confirmed": true } },
        }),
        Some("http-custom-headers") => {
            let steps: Vec<Value> = context
                .get("toolCalls")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|call| {
                    let mut step = Map::new();
                    step.insert("action".to_string(), json!("callTool"));
                    if let Value::Object(call) = call {
                        step.extend(call);
                    }
                    Value::Object(step)
                })
                .collect();
            json!({ "steps": steps })
        }
        Some("http-invalid-tool-headers") => json!({
            "steps": [{ "action": "callTool", "name": "valid_tool", "arguments": {} }],
        }),
        Some("http-standard-headers") => json!({
            "steps": [
                { "action": "listTools" },
                { "action": "callTool", "name": "test_headers", "arguments": {} },
                { "action": "listPrompts" },
                { "action": "getPrompt", "name": "test_prompt", "arguments": {} },
                { "action": "listResources" },
                { "action": "readResource", "uri": "file:///path/to/file%20name.txt" },
            ],
        }),
        _ => json!({
            "steps": [
                { "action": "listTools" },
                { "action": "listPrompts" },
                { "action": "listResources" },
            ],
        }),
    };

    // Runner 0.1.16 does not set MCP_CONFORMANCE_PROTOCOL_VERSION; default to
    // the 2025-11-25 spec version those scenarios expect.
    let protocol_version = std::env::var("MCP_CONFORMANCE_PROTOCOL_VERSION")
        .unwrap_or_else(|_| "2025-11-25".to_string());
    {
        script["protocolVersion"] = json!(protocol_version);
    }
    script
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [target] = args.as_slice() else {
        eprintln!("usage: mcp_conformance_driver <server-url-or-stdio-command>");
        std::process::exit(2);
    };

    let scenario = std::env::var("MCP_CONFORMANCE_SCENARIO").ok();
    let script = script_for_scenario(scenario.as_deref());

    let goose = std::env::var("GOOSE_BIN").unwrap_or_else(|_| "target/debug/goose".to_string());
    let path_root = tempfile::Builder::new()
        .prefix("goose-mcp-conformance-")
        .tempdir()
        .unwrap_or_else(|err| {
            eprintln!("failed to create temporary GOOSE_PATH_ROOT: {err}");
            std::process::exit(1);
        });
    let mut child = Command::new(&goose)
        .args(["mcp-probe", target, "--script", "-"])
        .env("GOOSE_OAUTH_AUTOMATIC_CALLBACK", "1")
        .env("GOOSE_DISABLE_KEYRING", "1")
        .env("GOOSE_PATH_ROOT", path_root.path())
        .stdin(Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| {
            eprintln!("failed to spawn {goose}: {err}");
            std::process::exit(1);
        });

    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(script.to_string().as_bytes())
        .expect("write probe script to goose stdin");

    let status = child.wait().expect("wait for goose");
    std::process::exit(status.code().unwrap_or(1));
}
