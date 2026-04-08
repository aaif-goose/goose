use crate::conversation::message::{Message, MessageContent};
use crate::providers::errors::ProviderError;
use llama_cpp_2::model::AddBos;
use llama_cpp_2::openai::OpenAIChatTemplateParams;
use rmcp::model::CallToolRequestParams;
use serde_json::Value;
use std::borrow::Cow;
use uuid::Uuid;

use super::finalize_usage;
use super::inference_engine::{
    context_cap, create_and_prefill_context, estimate_max_context_for_memory, generation_loop,
    validate_and_compute_context, GenerationContext, TokenAction,
};

pub(super) fn generate_with_native_tools(
    ctx: &mut GenerationContext<'_>,
    oai_messages_json: &Option<String>,
    full_tools_json: Option<&str>,
    compact_tools: Option<&str>,
) -> Result<(), ProviderError> {
    let min_generation_headroom = 512;
    let n_ctx_train = ctx.loaded.model.n_ctx_train() as usize;
    let memory_max_ctx = estimate_max_context_for_memory(&ctx.loaded.model, ctx.runtime);
    let cap = context_cap(ctx.settings, ctx.context_limit, n_ctx_train, memory_max_ctx);
    let token_budget = cap.saturating_sub(min_generation_headroom);

    let apply_template = |tools: Option<&str>| {
        if let Some(ref messages_json) = oai_messages_json {
            let params = OpenAIChatTemplateParams {
                messages_json: messages_json.as_str(),
                tools_json: tools,
                tool_choice: None,
                json_schema: None,
                grammar: None,
                reasoning_format: None,
                chat_template_kwargs: None,
                add_generation_prompt: true,
                use_jinja: true,
                parallel_tool_calls: false,
                enable_thinking: false,
                add_bos: false,
                add_eos: false,
                parse_tool_calls: true,
            };
            ctx.loaded
                .model
                .apply_chat_template_oaicompat(&ctx.loaded.template, &params)
        } else {
            ctx.loaded.model.apply_chat_template_with_tools_oaicompat(
                &ctx.loaded.template,
                ctx.chat_messages,
                tools,
                None,
                true,
            )
        }
    };

    let template_result = match apply_template(full_tools_json) {
        Ok(r) => {
            let token_count = ctx
                .loaded
                .model
                .str_to_token(&r.prompt, AddBos::Never)
                .map(|t| t.len())
                .unwrap_or(0);
            if token_count > token_budget {
                apply_template(compact_tools).unwrap_or(r)
            } else {
                r
            }
        }
        Err(_) => apply_template(compact_tools).map_err(|e| {
            ProviderError::ExecutionError(format!("Failed to apply chat template: {}", e))
        })?,
    };

    let _ = ctx.log.write(
        &serde_json::json!({"applied_prompt": &template_result.prompt}),
        None,
    );

    let tokens = ctx
        .loaded
        .model
        .str_to_token(&template_result.prompt, AddBos::Never)
        .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;

    let (prompt_token_count, effective_ctx) = validate_and_compute_context(
        ctx.loaded,
        ctx.runtime,
        tokens.len(),
        ctx.context_limit,
        ctx.settings,
    )?;
    let mut llama_ctx = create_and_prefill_context(
        ctx.loaded,
        ctx.runtime,
        &tokens,
        effective_ctx,
        ctx.settings,
    )?;

    let message_id = ctx.message_id;
    let tx = ctx.tx;
    let mut generated_text = String::new();

    // Initialize streaming parser — handles thinking tokens, tool calls, etc.
    let mut stream_parser = template_result.streaming_state_oaicompat().map_err(|e| {
        ProviderError::ExecutionError(format!("Failed to init streaming parser: {}", e))
    })?;

    // Feed the generation prompt to the parser so it knows the context.
    // The model may echo this prefix; the parser needs to see it to strip it.
    if !template_result.generation_prompt.is_empty() {
        let _ = stream_parser.update(&template_result.generation_prompt, true);
    }

    // Accumulate tool calls across streaming deltas
    let mut accumulated_tool_calls: Vec<Value> = Vec::new();

    let output_token_count = generation_loop(
        &ctx.loaded.model,
        &mut llama_ctx,
        ctx.settings,
        prompt_token_count,
        effective_ctx,
        |piece| {
            generated_text.push_str(piece);

            // Feed the new piece to the streaming parser
            match stream_parser.update(piece, true) {
                Ok(deltas) => {
                    for delta_json in deltas {
                        if let Ok(delta) = serde_json::from_str::<Value>(&delta_json) {
                            // Stream content text to the UI
                            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                                if !content.is_empty() {
                                    let mut msg = Message::assistant().with_text(content);
                                    msg.id = Some(message_id.to_string());
                                    if tx.blocking_send(Ok((Some(msg), None))).is_err() {
                                        return Ok(TokenAction::Stop);
                                    }
                                }
                            }
                            // Accumulate tool call deltas
                            if let Some(tool_calls) =
                                delta.get("tool_calls").and_then(|v| v.as_array())
                            {
                                for tc in tool_calls {
                                    accumulated_tool_calls.push(tc.clone());
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Streaming parser error: {}", e);
                    let mut msg = Message::assistant().with_text(piece);
                    msg.id = Some(message_id.to_string());
                    if tx.blocking_send(Ok((Some(msg), None))).is_err() {
                        return Ok(TokenAction::Stop);
                    }
                }
            }

            let should_stop = template_result
                .additional_stops
                .iter()
                .any(|stop| generated_text.ends_with(stop));
            if should_stop {
                Ok(TokenAction::Stop)
            } else {
                Ok(TokenAction::Continue)
            }
        },
    )?;

    // Finalize the streaming parser with is_partial=false
    if let Ok(final_deltas) = stream_parser.update("", false) {
        for delta_json in final_deltas {
            if let Ok(delta) = serde_json::from_str::<Value>(&delta_json) {
                if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                    if !content.is_empty() {
                        let mut msg = Message::assistant().with_text(content);
                        msg.id = Some(message_id.to_string());
                        let _ = tx.blocking_send(Ok((Some(msg), None)));
                    }
                }
                if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tool_calls {
                        accumulated_tool_calls.push(tc.clone());
                    }
                }
            }
        }
    }

    // Convert accumulated tool calls to messages
    let tool_call_msgs = extract_oai_tool_call_messages(&accumulated_tool_calls, message_id);
    for msg in tool_call_msgs {
        let _ = tx.blocking_send(Ok((Some(msg), None)));
    }

    let provider_usage = finalize_usage(
        ctx.log,
        std::mem::take(&mut ctx.model_name),
        "native",
        prompt_token_count,
        output_token_count,
        Some(("generated_text", &generated_text)),
    );
    let _ = ctx.tx.blocking_send(Ok((None, Some(provider_usage))));
    Ok(())
}

/// Merge OpenAI streaming deltas by `index` into complete tool calls, then
/// convert to Goose Message objects.
///
/// The streaming parser emits partial deltas like:
///   {"tool_calls": [{"index": 0, "id": "abc", "function": {"name": "shell"}}]}
///   {"tool_calls": [{"index": 0, "function": {"arguments": "{\"command\":"}}]}
///   {"tool_calls": [{"index": 0, "function": {"arguments": " \"ls\"}"}}]}
///
/// These must be merged by `index` before extracting complete tool calls.
fn extract_oai_tool_call_messages(deltas: &[Value], message_id: &str) -> Vec<Message> {
    let mut merged: std::collections::BTreeMap<u64, (String, String, String)> =
        std::collections::BTreeMap::new();

    for delta in deltas {
        let index = delta.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
        let entry = merged
            .entry(index)
            .or_insert_with(|| (String::new(), String::new(), String::new()));

        if let Some(id) = delta.get("id").and_then(|v| v.as_str()) {
            if !id.is_empty() {
                entry.0 = id.to_string();
            }
        }
        if let Some(func) = delta.get("function") {
            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                if !name.is_empty() {
                    entry.1 = name.to_string();
                }
            }
            if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                entry.2.push_str(args);
            }
        }
    }

    merged
        .into_values()
        .filter_map(|(id, name, args_str)| {
            if name.is_empty() {
                return None;
            }

            let id = if id.is_empty() {
                Uuid::new_v4().to_string()
            } else {
                id
            };

            let arguments: Option<serde_json::Map<String, Value>> = if args_str.is_empty() {
                None
            } else {
                match serde_json::from_str(&args_str) {
                    Ok(args) => Some(args),
                    Err(_) => return None,
                }
            };

            let tool_call = match arguments {
                Some(args) => CallToolRequestParams::new(Cow::Owned(name)).with_arguments(args),
                None => CallToolRequestParams::new(Cow::Owned(name)),
            };

            let mut msg = Message::assistant();
            msg.content
                .push(MessageContent::tool_request(id, Ok(tool_call)));
            msg.id = Some(message_id.to_string());
            Some(msg)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn get_tool_call_name(msg: &Message) -> &str {
        match &msg.content[0] {
            MessageContent::ToolRequest(req) => {
                let call = req.tool_call.as_ref().unwrap();
                &call.name
            }
            _ => panic!("Expected ToolRequest"),
        }
    }

    fn get_tool_call_args(msg: &Message) -> Option<&serde_json::Map<String, Value>> {
        match &msg.content[0] {
            MessageContent::ToolRequest(req) => {
                let call = req.tool_call.as_ref().unwrap();
                call.arguments.as_ref()
            }
            _ => panic!("Expected ToolRequest"),
        }
    }

    #[test]
    fn test_merge_streaming_deltas() {
        // Simulates OpenAI streaming: name in first delta, arguments split across multiple
        let deltas = vec![
            json!({"index": 0, "id": "call_1", "type": "function", "function": {"name": "developer__shell", "arguments": ""}}),
            json!({"index": 0, "function": {"arguments": "{\"command\":"}}),
            json!({"index": 0, "function": {"arguments": " \"ls\"}"}}),
        ];
        let msgs = extract_oai_tool_call_messages(&deltas, "msg-1");
        assert_eq!(msgs.len(), 1);
        assert_eq!(get_tool_call_name(&msgs[0]), "developer__shell");
        let args = get_tool_call_args(&msgs[0]).unwrap();
        assert_eq!(args.get("command").unwrap(), "ls");
    }

    #[test]
    fn test_multiple_tool_calls_by_index() {
        let deltas = vec![
            json!({"index": 0, "id": "call_1", "function": {"name": "developer__shell", "arguments": "{\"command\": \"ls\"}"}}),
            json!({"index": 1, "id": "call_2", "function": {"name": "developer__shell", "arguments": "{\"command\": \"pwd\"}"}}),
        ];
        let msgs = extract_oai_tool_call_messages(&deltas, "msg-1");
        assert_eq!(msgs.len(), 2);
        let args0 = get_tool_call_args(&msgs[0]).unwrap();
        let args1 = get_tool_call_args(&msgs[1]).unwrap();
        assert_eq!(args0.get("command").unwrap(), "ls");
        assert_eq!(args1.get("command").unwrap(), "pwd");
    }

    #[test]
    fn test_multiple_arguments_streamed() {
        // Arguments with multiple keys streamed token by token
        let deltas = vec![
            json!({"index": 0, "id": "call_1", "function": {"name": "developer__shell", "arguments": ""}}),
            json!({"index": 0, "function": {"arguments": "{\"command\""}}),
            json!({"index": 0, "function": {"arguments": ": \"ls -la\","}}),
            json!({"index": 0, "function": {"arguments": " \"timeout\":"}}),
            json!({"index": 0, "function": {"arguments": " 30}"}}),
        ];
        let msgs = extract_oai_tool_call_messages(&deltas, "msg-1");
        assert_eq!(msgs.len(), 1);
        let args = get_tool_call_args(&msgs[0]).unwrap();
        assert_eq!(args.get("command").unwrap(), "ls -la");
        assert_eq!(args.get("timeout").unwrap(), 30);
    }

    #[test]
    fn test_empty_name_skipped() {
        let deltas = vec![json!({"index": 0, "function": {"name": "", "arguments": "{}"}})];
        let msgs = extract_oai_tool_call_messages(&deltas, "msg-1");
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_no_deltas() {
        let msgs = extract_oai_tool_call_messages(&[], "msg-1");
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_tool_call_without_arguments() {
        let deltas = vec![json!({"index": 0, "id": "call_1", "function": {"name": "some_tool"}})];
        let msgs = extract_oai_tool_call_messages(&deltas, "msg-1");
        assert_eq!(msgs.len(), 1);
        assert_eq!(get_tool_call_name(&msgs[0]), "some_tool");
        assert!(get_tool_call_args(&msgs[0]).is_none());
    }

    #[test]
    fn test_malformed_arguments_drops_tool_call() {
        let deltas = vec![
            json!({"index": 0, "id": "call_1", "function": {"name": "developer__shell", "arguments": ""}}),
            json!({"index": 0, "function": {"arguments": "{\"command\": \"rm -rf"}}),
        ];
        let msgs = extract_oai_tool_call_messages(&deltas, "msg-1");
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_generates_id_when_missing() {
        let deltas =
            vec![json!({"index": 0, "function": {"name": "some_tool", "arguments": "{}"}})];
        let msgs = extract_oai_tool_call_messages(&deltas, "msg-1");
        assert_eq!(msgs.len(), 1);
        assert_eq!(get_tool_call_name(&msgs[0]), "some_tool");
        match &msgs[0].content[0] {
            MessageContent::ToolRequest(req) => {
                assert!(!req.id.is_empty());
            }
            _ => panic!("Expected ToolRequest"),
        }
    }
}
