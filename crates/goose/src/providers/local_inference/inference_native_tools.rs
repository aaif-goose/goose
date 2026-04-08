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
                reasoning_format: if ctx.settings.enable_thinking {
                    Some("auto")
                } else {
                    None
                },
                chat_template_kwargs: None,
                add_generation_prompt: true,
                use_jinja: true,
                parallel_tool_calls: false,
                enable_thinking: ctx.settings.enable_thinking,
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
    // Accumulate thinking/reasoning across the entire generation so we can
    // attach it to the final tool-call message (mirroring what the OpenAI
    // streaming path does). Streaming chunks are still sent for UI display.
    let mut accumulated_thinking = String::new();

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
                            // Stream thinking/reasoning content
                            if let Some(reasoning) =
                                delta.get("reasoning_content").and_then(|v| v.as_str())
                            {
                                if !reasoning.is_empty() {
                                    accumulated_thinking.push_str(reasoning);
                                    let mut msg = Message::assistant().with_thinking(reasoning, "");
                                    msg.id = Some(message_id.to_string());
                                    if tx.blocking_send(Ok((Some(msg), None))).is_err() {
                                        return Ok(TokenAction::Stop);
                                    }
                                }
                            }
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
                if let Some(reasoning) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                    if !reasoning.is_empty() {
                        accumulated_thinking.push_str(reasoning);
                        let mut msg = Message::assistant().with_thinking(reasoning, "");
                        msg.id = Some(message_id.to_string());
                        let _ = tx.blocking_send(Ok((Some(msg), None)));
                    }
                }
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

    // Build a single message combining thinking + all tool calls, mirroring
    // the structure produced by the OpenAI streaming path. The agent relies
    // on this combined message to:
    //   1. Extract thinking and attach it to per-tool-request messages
    //   2. Enable merge_split_tool_call_messages to reconstruct the standard
    //      OpenAI format (one assistant msg with N tool_calls, then N tool results)
    let tool_call_contents = extract_oai_tool_call_contents(&accumulated_tool_calls);
    if !tool_call_contents.is_empty() {
        let mut contents: Vec<MessageContent> = Vec::new();
        if !accumulated_thinking.is_empty() {
            contents.push(MessageContent::thinking(&accumulated_thinking, ""));
        }
        contents.extend(tool_call_contents);
        let mut msg = Message::new(
            rmcp::model::Role::Assistant,
            chrono::Utc::now().timestamp(),
            contents,
        );
        msg.id = Some(message_id.to_string());
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

/// Merge OpenAI streaming deltas by `index` into `MessageContent` items.
///
/// Returns one `ToolRequest` content per distinct tool call index. The caller
/// is responsible for combining these into a single `Message` (together with
/// any accumulated thinking content).
fn extract_oai_tool_call_contents(deltas: &[Value]) -> Vec<MessageContent> {
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

            Some(MessageContent::tool_request(id, Ok(tool_call)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn get_content_tool_call_name(content: &MessageContent) -> &str {
        match content {
            MessageContent::ToolRequest(req) => {
                let call = req.tool_call.as_ref().unwrap();
                &call.name
            }
            _ => panic!("Expected ToolRequest"),
        }
    }

    fn get_content_tool_call_args(
        content: &MessageContent,
    ) -> Option<&serde_json::Map<String, Value>> {
        match content {
            MessageContent::ToolRequest(req) => {
                let call = req.tool_call.as_ref().unwrap();
                call.arguments.as_ref()
            }
            _ => panic!("Expected ToolRequest"),
        }
    }

    #[test]
    fn test_merge_streaming_deltas() {
        let deltas = vec![
            json!({"index": 0, "id": "call_1", "type": "function", "function": {"name": "developer__shell", "arguments": ""}}),
            json!({"index": 0, "function": {"arguments": "{\"command\":"}}),
            json!({"index": 0, "function": {"arguments": " \"ls\"}"}}),
        ];
        let contents = extract_oai_tool_call_contents(&deltas);
        assert_eq!(contents.len(), 1);
        assert_eq!(get_content_tool_call_name(&contents[0]), "developer__shell");
        let args = get_content_tool_call_args(&contents[0]).unwrap();
        assert_eq!(args.get("command").unwrap(), "ls");
    }

    #[test]
    fn test_multiple_tool_calls_by_index() {
        let deltas = vec![
            json!({"index": 0, "id": "call_1", "function": {"name": "developer__shell", "arguments": "{\"command\": \"ls\"}"}}),
            json!({"index": 1, "id": "call_2", "function": {"name": "developer__shell", "arguments": "{\"command\": \"pwd\"}"}}),
        ];
        let contents = extract_oai_tool_call_contents(&deltas);
        assert_eq!(contents.len(), 2);
        let args0 = get_content_tool_call_args(&contents[0]).unwrap();
        let args1 = get_content_tool_call_args(&contents[1]).unwrap();
        assert_eq!(args0.get("command").unwrap(), "ls");
        assert_eq!(args1.get("command").unwrap(), "pwd");
    }

    #[test]
    fn test_multiple_arguments_streamed() {
        let deltas = vec![
            json!({"index": 0, "id": "call_1", "function": {"name": "developer__shell", "arguments": ""}}),
            json!({"index": 0, "function": {"arguments": "{\"command\""}}),
            json!({"index": 0, "function": {"arguments": ": \"ls -la\","}}),
            json!({"index": 0, "function": {"arguments": " \"timeout\":"}}),
            json!({"index": 0, "function": {"arguments": " 30}"}}),
        ];
        let contents = extract_oai_tool_call_contents(&deltas);
        assert_eq!(contents.len(), 1);
        let args = get_content_tool_call_args(&contents[0]).unwrap();
        assert_eq!(args.get("command").unwrap(), "ls -la");
        assert_eq!(args.get("timeout").unwrap(), 30);
    }

    #[test]
    fn test_empty_name_skipped() {
        let deltas = vec![json!({"index": 0, "function": {"name": "", "arguments": "{}"}})];
        let contents = extract_oai_tool_call_contents(&deltas);
        assert!(contents.is_empty());
    }

    #[test]
    fn test_no_deltas() {
        let contents = extract_oai_tool_call_contents(&[]);
        assert!(contents.is_empty());
    }

    #[test]
    fn test_tool_call_without_arguments() {
        let deltas = vec![json!({"index": 0, "id": "call_1", "function": {"name": "some_tool"}})];
        let contents = extract_oai_tool_call_contents(&deltas);
        assert_eq!(contents.len(), 1);
        assert_eq!(get_content_tool_call_name(&contents[0]), "some_tool");
        assert!(get_content_tool_call_args(&contents[0]).is_none());
    }

    #[test]
    fn test_malformed_arguments_drops_tool_call() {
        let deltas = vec![
            json!({"index": 0, "id": "call_1", "function": {"name": "developer__shell", "arguments": ""}}),
            json!({"index": 0, "function": {"arguments": "{\"command\": \"rm -rf"}}),
        ];
        let contents = extract_oai_tool_call_contents(&deltas);
        assert!(contents.is_empty());
    }

    #[test]
    fn test_generates_id_when_missing() {
        let deltas =
            vec![json!({"index": 0, "function": {"name": "some_tool", "arguments": "{}"}})];
        let contents = extract_oai_tool_call_contents(&deltas);
        assert_eq!(contents.len(), 1);
        assert_eq!(get_content_tool_call_name(&contents[0]), "some_tool");
        match &contents[0] {
            MessageContent::ToolRequest(req) => {
                assert!(!req.id.is_empty());
            }
            _ => panic!("Expected ToolRequest"),
        }
    }
}
