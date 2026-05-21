//! Pre-send deduplication of repeated tool-result text blocks.
//!
//! When the same `(tool_name, arguments)` pair returns the same text body
//! twice within a single conversation (e.g. the agent re-reads `plan.md`
//! three times), the second and subsequent copies are replaced with a short
//! pointer back to the first occurrence. The session DB and the on-screen
//! view are untouched — only the conversation snapshot sent to the LLM is
//! deduped.
//!
//! Disable by setting `GOOSE_TOOL_RESULT_DEDUP=0`.
//!
//! Dedup only fires when the `tool_name + arguments` are byte-identical to
//! a prior call AND the text body is byte-identical to that prior call's
//! response. Any change in either arguments or response content (e.g. the
//! file actually changed) flows through unmodified.

use std::collections::HashMap;

use rmcp::model::RawContent;
use sha2::{Digest, Sha256};

use crate::conversation::message::{Message, MessageContent};

/// Environment variable for disabling tool-result dedup. Set to `0` to disable.
pub const TOOL_RESULT_DEDUP_ENV: &str = "GOOSE_TOOL_RESULT_DEDUP";

/// Minimum body length (in bytes) before we bother deduping. Tiny results
/// (file paths, exit codes, "Edited X (1 lines -> 1 lines)") aren't worth
/// the replacement marker overhead.
const MIN_DEDUP_BYTES: usize = 512;

fn dedup_enabled() -> bool {
    match std::env::var(TOOL_RESULT_DEDUP_ENV) {
        Ok(v) => v.trim() != "0",
        Err(_) => true,
    }
}

fn short_hash(s: &str) -> String {
    let digest = Sha256::digest(s.as_bytes());
    let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    hex
}

/// First-seen record for a given `(tool_name, args, body)` triple.
#[derive(Clone)]
struct SeenRecord {
    /// 1-based turn index (message index in the conversation) where we
    /// first saw this exact tool result.
    turn: usize,
    /// Short hash of the body, for the marker.
    body_sha8: String,
    /// Original body length in bytes, for the marker.
    body_bytes: usize,
}

/// Walk `messages` left-to-right and replace duplicate tool_response text
/// blocks with a short pointer to their first occurrence. Returns a new
/// `Vec<Message>` — the input is not mutated. When dedup is disabled, the
/// input is cloned through unchanged.
pub fn dedup_tool_results(messages: &[Message]) -> Vec<Message> {
    if !dedup_enabled() {
        return messages.to_vec();
    }

    // Map tool_use_id -> (tool_name, args_canonical_json). Built as we walk.
    let mut tool_use_info: HashMap<String, (String, String)> = HashMap::new();

    // Map sha256(tool_name + args + body) -> first-seen record.
    let mut seen: HashMap<[u8; 32], SeenRecord> = HashMap::new();

    let mut out = Vec::with_capacity(messages.len());
    for (turn_idx, msg) in messages.iter().enumerate() {
        let mut new_content = Vec::with_capacity(msg.content.len());
        for content in &msg.content {
            match content {
                MessageContent::ToolRequest(req) => {
                    if let Ok(call) = &req.tool_call {
                        let args_json = serde_json::to_string(&call.arguments)
                            .unwrap_or_else(|_| String::new());
                        tool_use_info.insert(req.id.clone(), (call.name.to_string(), args_json));
                    }
                    new_content.push(content.clone());
                }
                MessageContent::ToolResponse(resp) => {
                    let info = tool_use_info.get(&resp.id);
                    let new_resp = maybe_dedup_response(resp, info, turn_idx + 1, &mut seen);
                    new_content.push(MessageContent::ToolResponse(new_resp));
                }
                other => new_content.push(other.clone()),
            }
        }
        let mut new_msg = msg.clone();
        new_msg.content = new_content;
        out.push(new_msg);
    }
    out
}

fn maybe_dedup_response(
    resp: &crate::conversation::message::ToolResponse,
    info: Option<&(String, String)>,
    turn: usize,
    seen: &mut HashMap<[u8; 32], SeenRecord>,
) -> crate::conversation::message::ToolResponse {
    // Only dedup successful results — keep errors verbatim so the model can
    // see them every time.
    let result = match resp.tool_result.as_ref() {
        Ok(r) => r,
        Err(_) => return resp.clone(),
    };

    let (tool_name, args_json) = match info {
        Some(t) => (t.0.as_str(), t.1.as_str()),
        // No matching tool_use — leave alone.
        None => return resp.clone(),
    };

    let mut new_result = result.clone();
    for content in &mut new_result.content {
        if let RawContent::Text(text_content) = &mut content.raw {
            if text_content.text.len() < MIN_DEDUP_BYTES {
                continue;
            }
            let mut hasher = Sha256::new();
            hasher.update(tool_name.as_bytes());
            hasher.update(b"\x00");
            hasher.update(args_json.as_bytes());
            hasher.update(b"\x00");
            hasher.update(text_content.text.as_bytes());
            let key: [u8; 32] = hasher.finalize().into();

            if let Some(record) = seen.get(&key) {
                let marker = format!(
                    "[goose: identical to tool result at turn {turn0} \
(sha={sha8}, {bytes} bytes elided, tool={tool_name})]",
                    turn0 = record.turn,
                    sha8 = record.body_sha8,
                    bytes = record.body_bytes,
                    tool_name = tool_name,
                );
                text_content.text = marker;
            } else {
                let body_sha8 = short_hash(&text_content.text);
                seen.insert(
                    key,
                    SeenRecord {
                        turn,
                        body_sha8,
                        body_bytes: text_content.text.len(),
                    },
                );
            }
        }
    }

    crate::conversation::message::ToolResponse {
        id: resp.id.clone(),
        tool_result: Ok(new_result),
        metadata: resp.metadata.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{Message, MessageContent};
    use rmcp::model::{CallToolRequestParams, CallToolResult, Content};
    use rmcp::object;
    use serial_test::serial;

    fn mk_request(
        id: &str,
        name: &str,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> Message {
        let call = CallToolRequestParams::new(name.to_string()).with_arguments(args);
        Message::assistant().with_tool_request(id, Ok(call))
    }

    fn mk_response(id: &str, body: &str) -> Message {
        let result = CallToolResult::success(vec![Content::text(body.to_string())]);
        Message::user().with_tool_response(id, Ok(result))
    }

    fn body_of(msg: &Message) -> String {
        for c in &msg.content {
            if let MessageContent::ToolResponse(resp) = c {
                if let Ok(r) = &resp.tool_result {
                    for content in &r.content {
                        if let RawContent::Text(t) = &content.raw {
                            return t.text.clone();
                        }
                    }
                }
            }
        }
        String::new()
    }

    #[test]
    #[serial]
    fn test_dedup_replaces_second_identical_result() {
        std::env::remove_var(TOOL_RESULT_DEDUP_ENV);
        let body = "x".repeat(2000);
        let args = object!({ "path": "/foo" });
        let msgs = vec![
            mk_request("1", "shell", args.clone()),
            mk_response("1", &body),
            mk_request("2", "shell", args.clone()),
            mk_response("2", &body),
        ];
        let out = dedup_tool_results(&msgs);
        assert_eq!(body_of(&out[1]), body);
        let dup = body_of(&out[3]);
        assert!(dup.starts_with("[goose: identical"), "got: {dup}");
        assert!(dup.contains("turn 2"));
        assert!(dup.contains("tool=shell"));
    }

    #[test]
    #[serial]
    fn test_dedup_preserves_when_args_differ() {
        std::env::remove_var(TOOL_RESULT_DEDUP_ENV);
        let body = "x".repeat(2000);
        let msgs = vec![
            mk_request("1", "shell", object!({ "path": "/a" })),
            mk_response("1", &body),
            mk_request("2", "shell", object!({ "path": "/b" })),
            mk_response("2", &body),
        ];
        let out = dedup_tool_results(&msgs);
        assert_eq!(body_of(&out[1]), body);
        assert_eq!(body_of(&out[3]), body, "different args must not dedup");
    }

    #[test]
    #[serial]
    fn test_dedup_preserves_when_body_changed() {
        std::env::remove_var(TOOL_RESULT_DEDUP_ENV);
        let args = object!({ "path": "/foo" });
        let body_a = "x".repeat(2000);
        let body_b = format!("{}{}", "x".repeat(1999), "y");
        let msgs = vec![
            mk_request("1", "shell", args.clone()),
            mk_response("1", &body_a),
            mk_request("2", "shell", args.clone()),
            mk_response("2", &body_b),
        ];
        let out = dedup_tool_results(&msgs);
        assert_eq!(body_of(&out[1]), body_a);
        assert_eq!(body_of(&out[3]), body_b, "changed body must not dedup");
    }

    #[test]
    #[serial]
    fn test_dedup_skips_small_bodies() {
        std::env::remove_var(TOOL_RESULT_DEDUP_ENV);
        let body = "tiny";
        let args = object!({ "path": "/foo" });
        let msgs = vec![
            mk_request("1", "shell", args.clone()),
            mk_response("1", body),
            mk_request("2", "shell", args.clone()),
            mk_response("2", body),
        ];
        let out = dedup_tool_results(&msgs);
        assert_eq!(body_of(&out[1]), body);
        assert_eq!(body_of(&out[3]), body, "small bodies should not be deduped");
    }

    #[test]
    #[serial]
    fn test_dedup_disabled_via_env_zero() {
        std::env::set_var(TOOL_RESULT_DEDUP_ENV, "0");
        let body = "x".repeat(2000);
        let args = object!({ "path": "/foo" });
        let msgs = vec![
            mk_request("1", "shell", args.clone()),
            mk_response("1", &body),
            mk_request("2", "shell", args.clone()),
            mk_response("2", &body),
        ];
        let out = dedup_tool_results(&msgs);
        assert_eq!(body_of(&out[3]), body, "disabled flag must pass through");
        std::env::remove_var(TOOL_RESULT_DEDUP_ENV);
    }

    #[test]
    #[serial]
    fn test_dedup_third_copy_still_points_to_first() {
        std::env::remove_var(TOOL_RESULT_DEDUP_ENV);
        let body = "x".repeat(2000);
        let args = object!({ "path": "/foo" });
        let msgs = vec![
            mk_request("1", "shell", args.clone()),
            mk_response("1", &body),
            mk_request("2", "shell", args.clone()),
            mk_response("2", &body),
            mk_request("3", "shell", args.clone()),
            mk_response("3", &body),
        ];
        let out = dedup_tool_results(&msgs);
        let third = body_of(&out[5]);
        assert!(
            third.contains("turn 2"),
            "third copy should point to first occurrence (turn 2)"
        );
    }
}
