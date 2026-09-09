//! Across the consecutive requests of a session, the cache-relevant bytes a
//! provider has already seen must never change.
//!
//! Implicit caches require request N to be a verbatim item-prefix of request
//! N+1; explicit-breakpoint caches require the bytes up to the last
//! breakpoint to recur at the same positions, modulo the markers themselves.
//! Session states are shaped as the agent persists them and run through the
//! same projection and `fix_conversation` pass as the real request path.

use goose_provider_types::cache_semantics::{apply_chat_payload_breakpoints, CacheSemantics};
use goose_provider_types::conversation::message::{Message, MessageMetadata};
use goose_provider_types::conversation::{
    fix_conversation, merge_consecutive_messages_for_request, Conversation,
};
use goose_provider_types::formats::anthropic::{self, AnthropicFormatOptions};
use goose_provider_types::formats::{databricks, openai, openai_responses};
use goose_provider_types::images::ImageFormat;
use goose_provider_types::model::ModelConfig;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool};
use rmcp::object;
use serde_json::{json, Value};

const SYSTEM: &str = "You are a careful coding assistant.";

fn tools() -> Vec<Tool> {
    vec![Tool::new(
        "read_file",
        "Read a file from disk",
        object!({
            "type": "object",
            "properties": { "path": { "type": "string" } }
        }),
    )]
}

fn turn_block(turn: usize) -> String {
    format!(
        "<turn-context>\n\
         <current-time>2026-08-06 12:0{turn}:00 +02:00</current-time>\n\
         <working-directory>/Users/me/code/goose</working-directory>\n\
         <todo>step {turn} of the plan</todo>\n\
         </turn-context>"
    )
}

fn turn_context_message(turn: usize) -> Message {
    Message::user()
        .with_text(turn_block(turn))
        .with_metadata(MessageMetadata::agent_only().with_turn_context())
}

fn tool_exchange(id: &str, path: &str) -> [Message; 2] {
    [
        Message::assistant().with_tool_request(
            id,
            Ok(CallToolRequestParams::new("read_file").with_arguments(object!({ "path": path }))),
        ),
        Message::user().with_tool_response(
            id,
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "fn main() { run(); }",
            )])),
        ),
    ]
}

/// Two turns with one tool-loop iteration each; every state appends to the
/// previous one.
fn session_states() -> Vec<Vec<Message>> {
    let u1 = Message::user().with_text("What does the main entrypoint do?");
    let u2 = Message::user().with_text("Now add error handling to it.");
    let (tc1, tc2) = (turn_context_message(1), turn_context_message(2));
    let [a1, t1] = tool_exchange("call_1", "src/main.rs");
    let [a2, t2] = tool_exchange("call_2", "src/lib.rs");
    let a_text = Message::assistant().with_text("It calls `run()`.");

    vec![
        vec![u1.clone(), tc1.clone()],
        vec![u1.clone(), tc1.clone(), a1.clone(), t1.clone()],
        vec![
            u1.clone(),
            tc1.clone(),
            a1.clone(),
            t1.clone(),
            a_text.clone(),
            u2.clone(),
            tc2.clone(),
        ],
        vec![u1, tc1, a1, t1, a_text, u2, tc2, a2, t2],
    ]
}

/// The projection every request goes through before any formatter
/// (`stream_response_from_provider`).
fn provider_view(messages: &[Message]) -> Vec<Message> {
    let projected =
        Conversation::new_unvalidated(messages.iter().cloned()).agent_visible_messages();
    let (fixed, _) = fix_conversation(Conversation::new_unvalidated(projected));
    merge_consecutive_messages_for_request(fixed.messages().clone())
}

fn anthropic_request(messages: &[Message]) -> Value {
    anthropic::create_request(
        "anthropic",
        &ModelConfig::new("claude-sonnet-4-5"),
        SYSTEM,
        &provider_view(messages),
        &tools(),
        AnthropicFormatOptions::default(),
    )
    .unwrap()
}

fn openai_chat_messages(model: &str, messages: &[Message]) -> Vec<Value> {
    openai::create_request(
        &ModelConfig::new(model),
        SYSTEM,
        &provider_view(messages),
        &tools(),
        &ImageFormat::OpenAi,
        false,
    )
    .unwrap()["messages"]
        .as_array()
        .unwrap()
        .clone()
}

fn responses_input(messages: &[Message]) -> Vec<Value> {
    openai_responses::create_responses_request(
        &ModelConfig::new("gpt-5.2"),
        SYSTEM,
        &provider_view(messages),
        &tools(),
    )
    .unwrap()["input"]
        .as_array()
        .unwrap()
        .clone()
}

fn openrouter_style_request(messages: &[Message]) -> Value {
    let mut payload = openai::create_request(
        &ModelConfig::new("anthropic/claude-sonnet-4-5"),
        SYSTEM,
        &provider_view(messages),
        &tools(),
        &ImageFormat::OpenAi,
        false,
    )
    .unwrap();
    assert!(
        CacheSemantics::for_model("openrouter", "anthropic/claude-sonnet-4-5")
            .uses_explicit_breakpoints()
    );
    apply_chat_payload_breakpoints(&mut payload);
    payload
}

fn databricks_request(messages: &[Message]) -> Value {
    databricks::create_request(
        &ModelConfig::new("databricks-claude-sonnet-4"),
        SYSTEM,
        &provider_view(messages),
        &tools(),
        &ImageFormat::OpenAi,
    )
    .unwrap()
}

/// Strip breakpoint markers and normalize string content to block form:
/// upstream caches hash the normalized bytes, not the marker encoding.
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| k.as_str() != "cache_control")
                .map(|(k, v)| {
                    if k == "content" && v.is_string() {
                        (
                            k.clone(),
                            json!([{ "type": "text", "text": v.as_str().unwrap() }]),
                        )
                    } else {
                        (k.clone(), canonical(v))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

fn strict_violations(prev: &[Value], next: &[Value]) -> Vec<String> {
    if next.len() < prev.len() {
        return vec![format!(
            "request shrank from {} to {} items",
            prev.len(),
            next.len()
        )];
    }
    prev.iter()
        .zip(next)
        .enumerate()
        .filter(|(_, (p, n))| p != n)
        .map(|(i, (p, n))| format!("items diverge at index {i}:\n  sent: {p}\n  next: {n}"))
        .collect()
}

fn last_breakpoint(messages: &[Value]) -> Option<(usize, usize)> {
    let mut found = None;
    for (mi, message) in messages.iter().enumerate() {
        let blocks = match message["content"].as_array() {
            Some(blocks) => blocks.len(),
            None => 1,
        };
        for bi in 0..blocks {
            let block = match message["content"].as_array() {
                Some(array) => &array[bi],
                None => message,
            };
            if block.get("cache_control").is_some() {
                found = Some((mi, bi));
            }
        }
    }
    found
}

/// Everything the previous request cached (tools, system, messages up to its
/// last breakpoint) must recur verbatim in the next request.
fn explicit_violations(prev_request: &Value, next_request: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    let prev_messages = prev_request["messages"].as_array().unwrap();
    let Some((last_mi, last_bi)) = last_breakpoint(prev_messages) else {
        violations.push("previous request carries no message breakpoint".into());
        return violations;
    };

    let prev = canonical(prev_request);
    let next = canonical(next_request);
    for section in ["tools", "system"] {
        if prev[section] != next[section] {
            violations.push(format!(
                "cached section `{section}` changed between requests"
            ));
        }
    }

    let prev_messages = prev["messages"].as_array().unwrap();
    let next_messages = next["messages"].as_array().unwrap();
    if next_messages.len() <= last_mi {
        violations.push("messages shrank below the previous breakpoint".into());
        return violations;
    }
    for mi in 0..last_mi {
        if prev_messages[mi] != next_messages[mi] {
            violations.push(format!(
                "cached bytes changed at message {mi}:\n  sent: {}\n  next: {}",
                prev_messages[mi], next_messages[mi]
            ));
        }
    }

    let prev_blocks = prev_messages[last_mi]["content"].as_array().unwrap();
    match next_messages[last_mi]["content"].as_array() {
        Some(next_blocks)
            if next_blocks.len() > last_bi
                && prev_messages[last_mi]["role"] == next_messages[last_mi]["role"] =>
        {
            for bi in 0..=last_bi {
                if prev_blocks[bi] != next_blocks[bi] {
                    violations.push(format!(
                        "cached bytes changed at message {last_mi} block {bi}:\n  sent: {}\n  next: {}",
                        prev_blocks[bi], next_blocks[bi]
                    ));
                }
            }
        }
        _ => violations.push(format!("cached message {last_mi} changed shape")),
    }
    violations
}

fn consecutive<T>(items: &[T]) -> impl Iterator<Item = (usize, &T, &T)> {
    items.windows(2).enumerate().map(|(i, w)| (i, &w[0], &w[1]))
}

fn assert_clean(label: &str, violations: Vec<String>) {
    assert!(
        violations.is_empty(),
        "{label}: cache-relevant bytes changed between consecutive requests:\n{}",
        violations.join("\n")
    );
}

#[test]
fn anthropic_requests_stay_breakpoint_compatible() {
    let requests: Vec<Value> = session_states()
        .iter()
        .map(|m| anthropic_request(m))
        .collect();
    assert_ne!(requests[0], requests[1], "vacuous: requests are identical");
    for (i, prev, next) in consecutive(&requests) {
        assert_clean(
            &format!("anthropic pair {i}"),
            explicit_violations(prev, next),
        );
    }
}

/// The chat fleet's caches are merely tolerant, but with no relocation left
/// the strict relation holds anyway.
#[test]
fn openai_chat_requests_are_append_only() {
    let requests: Vec<Vec<Value>> = session_states()
        .iter()
        .map(|m| openai_chat_messages("gpt-4.1-mini", m))
        .collect();
    for (i, prev, next) in consecutive(&requests) {
        assert_clean(
            &format!("openai chat pair {i}"),
            strict_violations(prev, next),
        );
    }
}

#[test]
fn responses_requests_are_append_only() {
    let requests: Vec<Vec<Value>> = session_states()
        .iter()
        .map(|m| responses_input(m))
        .collect();
    for (i, prev, next) in consecutive(&requests) {
        assert_clean(
            &format!("responses pair {i}"),
            strict_violations(prev, next),
        );
    }
}

#[test]
fn responses_stack_chat_requests_are_append_only() {
    let requests: Vec<Vec<Value>> = session_states()
        .iter()
        .map(|m| openai_chat_messages("gpt-5.6-terra", m))
        .collect();
    for (i, prev, next) in consecutive(&requests) {
        assert_clean(
            &format!("responses-stack chat pair {i}"),
            strict_violations(prev, next),
        );
    }
}

/// The OpenRouter/LiteLLM path: an OpenAI chat payload post-processed with
/// Anthropic-dialect breakpoints by the shared writer.
#[test]
fn openrouter_style_requests_stay_breakpoint_compatible() {
    let requests: Vec<Value> = session_states()
        .iter()
        .map(|m| openrouter_style_request(m))
        .collect();
    for (i, prev, next) in consecutive(&requests) {
        assert_clean(
            &format!("openrouter pair {i}"),
            explicit_violations(prev, next),
        );
    }
}

#[test]
fn databricks_requests_stay_breakpoint_compatible() {
    let requests: Vec<Value> = session_states()
        .iter()
        .map(|m| databricks_request(m))
        .collect();
    for (i, prev, next) in consecutive(&requests) {
        assert_clean(
            &format!("databricks pair {i}"),
            explicit_violations(prev, next),
        );
    }
}

#[test]
fn seeded_regression_volatile_bytes_in_cached_prefix_are_caught() {
    let request_for = |stamp: &str| {
        let block =
            format!("<turn-context>\n<current-time>{stamp}</current-time>\n</turn-context>");
        let [a1, t1] = tool_exchange("call_1", "src/main.rs");
        anthropic_request(&[
            Message::user()
                .with_text(block)
                .with_visibility(false, true),
            Message::user().with_text("Now add error handling to it."),
            a1,
            t1,
        ])
    };
    let violations = explicit_violations(
        &request_for("2026-08-06 12:00:00"),
        &request_for("2026-08-06 12:01:00"),
    );
    assert!(
        !violations.is_empty(),
        "expected per-request volatile bytes inside the cached prefix to be caught"
    );
}

#[test]
fn seeded_regression_relocated_tail_is_caught() {
    let states = session_states();
    let prev = responses_input(&states[0]);

    let mut relocated_state = states[1].clone();
    let block = relocated_state.remove(1);
    relocated_state.push(block);
    let next = responses_input(&relocated_state);

    let violations = strict_violations(&prev, &next);
    assert!(
        !violations.is_empty(),
        "expected a relocated turn-context block to violate the append-only relation"
    );
}
