//! Regression: an unoffered tool call must come back as a tool request, not as text.
//!
//! Qwen-family servers emit `<tool_call><function=name>…</function></tool_call>`.
//! Some parsers only promote that block when `name` is in the request's `tools`
//! list, and otherwise leave the markup in assistant `content`. Goose then
//! renders the XML and ends the turn. TensorFold #256 / MiaAI-Lab recipe #40.
//!
//! This drives the same provider path the GDK exposes (`declarative_provider_from_json`
//! → `Provider::complete`), with a tools list that does not include the name the
//! prompt asks for.
//!
//! ```bash
//! GOOSE_TOOL_REGRESSION_URL=http://127.0.0.1:8888 \
//!   cargo run -p goose-sdk --example unoffered_tool_call --features uniffi
//! ```
//!
//! Optional: `GOOSE_TOOL_REGRESSION_MODEL` (default `qwen3.8-flash-next`),
//! `GOOSE_TOOL_REGRESSION_KEY` (Bearer token when the gateway requires one).
//! Exit 0 when the call is structured. Exit 1 when the markup leaks into text,
//! or when the reply has neither a tool request nor the markup.

use goose_sdk::bindings::{
    declarative_provider_from_json, MessageContent, MessageRole, ProviderMessage,
    ProviderModelConfig, ProviderTool,
};
use serde_json::json;

const OFFERED_TOOL: &str = "shell";
const UNOFFERED_TOOL: &str = "read_file";
const EXPECTED_PATH: &str = "/tmp/probe.txt";

fn leak(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("<tool_call") || lower.contains("<function=")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::var("GOOSE_TOOL_REGRESSION_URL")
        .map_err(|_| "set GOOSE_TOOL_REGRESSION_URL to the OpenAI-compatible base, without /v1")?;
    let model_name = std::env::var("GOOSE_TOOL_REGRESSION_MODEL")
        .unwrap_or_else(|_| "qwen3.8-flash-next".to_string());
    let api_key_env = if std::env::var("GOOSE_TOOL_REGRESSION_KEY").is_ok() {
        "GOOSE_TOOL_REGRESSION_KEY"
    } else {
        ""
    };

    let config = json!({
        "name": "tool-regression",
        "engine": "openai",
        "display_name": "unoffered tool-call regression",
        "api_key_env": api_key_env,
        "base_url": base,
        "models": [{ "name": model_name, "context_limit": 32768 }],
        "requires_auth": !api_key_env.is_empty(),
        "dynamic_models": false,
        "timeout_seconds": 180,
        "supports_streaming": true
    });

    let provider = declarative_provider_from_json(config.to_string())?;
    let model = ProviderModelConfig {
        model_name,
        context_limit: None,
        temperature: Some(0.0),
        max_tokens: Some(200),
        toolshim: false,
        toolshim_model: None,
        // The served Qwen template thinks by default. A small budget then
        // finishes inside the think block and never emits the call.
        request_params_json: Some(
            json!({ "chat_template_kwargs": { "enable_thinking": false } }).to_string(),
        ),
        provider_params_json: None,
        reasoning: Some(false),
        timeout_ms: Some(180_000),
        request_headers: None,
    };
    let tools = vec![ProviderTool {
        name: OFFERED_TOOL.to_string(),
        description: "Run a shell command".to_string(),
        input_schema_json: json!({
            "type": "object",
            "properties": { "command": { "type": "string" } },
            "required": ["command"]
        })
        .to_string(),
        annotations_json: None,
    }];

    let system = "You are a tool-using assistant. Use a tool for every request.".to_string();
    check_natural(&provider, &model, &system, &tools).await?;
    check_forced_markup(&provider, &model, &tools).await?;
    Ok(())
}

async fn check_natural(
    provider: &goose_sdk::bindings::Provider,
    model: &goose_sdk::bindings::ProviderModelConfig,
    system: &str,
    tools: &[ProviderTool],
) -> Result<(), Box<dyn std::error::Error>> {
    let messages = vec![ProviderMessage {
        role: MessageRole::User,
        content: vec![MessageContent::Text {
            text: format!(
                "Call the {UNOFFERED_TOOL} tool with path {EXPECTED_PATH}. \
                 Do not use {OFFERED_TOOL}. Do not explain."
            ),
        }],
    }];
    let (names, texts) = complete(provider, model, system, messages, tools).await?;
    refuse_leak("natural", &texts)?;
    if names.is_empty() {
        return fail(&format!(
            "natural: no tool request. Asked for {UNOFFERED_TOOL}, offered only {OFFERED_TOOL}."
        ));
    }
    if names.iter().any(|name| name == UNOFFERED_TOOL) {
        println!("PASS natural: {UNOFFERED_TOOL} came back as a tool request");
    } else {
        println!(
            "PASS natural: structured tool request {names:?} and no leaked markup \
             (model used an offered tool)"
        );
    }
    Ok(())
}

async fn check_forced_markup(
    provider: &goose_sdk::bindings::Provider,
    model: &goose_sdk::bindings::ProviderModelConfig,
    tools: &[ProviderTool],
) -> Result<(), Box<dyn std::error::Error>> {
    // Deterministic. The natural prompt may substitute `shell`. This one asks
    // for the exact unoffered payload, which is what the broken parser used to
    // copy into `content`.
    let markup = format!(
        "<tool_call>\n<function={UNOFFERED_TOOL}>\n<parameter=path>\n{EXPECTED_PATH}\n</parameter>\n</function>\n</tool_call>"
    );
    let messages = vec![ProviderMessage {
        role: MessageRole::User,
        content: vec![MessageContent::Text {
            text: format!(
                "Reply with exactly this tool call and nothing else. \
                 Do not change the function name. Do not call {OFFERED_TOOL}.\n{markup}"
            ),
        }],
    }];
    let (names, texts) = complete(
        provider,
        model,
        "Repeat the requested tool call.",
        messages,
        tools,
    )
    .await?;
    refuse_leak("forced", &texts)?;
    if !names.iter().any(|name| name == UNOFFERED_TOOL) {
        return fail(&format!(
            "forced: expected a tool request named {UNOFFERED_TOOL}, got {names:?}. \
             An unoffered but well-formed <tool_call> must be emitted as tool_calls, \
             not dropped and not left in content."
        ));
    }
    println!(
        "PASS forced: {UNOFFERED_TOOL} was promoted to a tool request even though \
         the request tools were only {OFFERED_TOOL}"
    );
    Ok(())
}

async fn complete(
    provider: &goose_sdk::bindings::Provider,
    model: &goose_sdk::bindings::ProviderModelConfig,
    system: &str,
    messages: Vec<ProviderMessage>,
    tools: &[ProviderTool],
) -> Result<(Vec<String>, Vec<String>), Box<dyn std::error::Error>> {
    let completion = provider
        .complete(model.clone(), system.to_string(), messages, tools.to_vec())
        .await?;
    let mut names = Vec::new();
    let mut texts = Vec::new();
    for block in completion.content {
        match block {
            MessageContent::ToolRequest {
                name,
                arguments_json,
                ..
            } => {
                println!("tool_request {name} {arguments_json}");
                names.push(name);
            }
            MessageContent::Text { text } => {
                println!("text {}", text.chars().take(240).collect::<String>());
                texts.push(text);
            }
            other => println!("other {other:?}"),
        }
    }
    Ok((names, texts))
}

fn refuse_leak(label: &str, texts: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if texts.iter().any(|text| leak(text)) {
        fail(&format!(
            "{label}: <tool_call> markup was returned as assistant text. \
             The parser must emit tool_calls for a well-formed call even when \
             the name was not in the request tools."
        ))
    } else {
        Ok(())
    }
}

fn fail(message: &str) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("FAIL: {message}");
    std::process::exit(1);
}
