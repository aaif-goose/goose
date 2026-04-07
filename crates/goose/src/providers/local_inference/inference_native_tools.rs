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

    tracing::info!(
        generation_prompt = %template_result.generation_prompt,
        chat_format = template_result.chat_format,
        parse_tool_calls = template_result.parse_tool_calls,
        has_parser = template_result.parser.is_some(),
        additional_stops = ?template_result.additional_stops,
        "Template result fields"
    );

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

/// Convert OpenAI-format tool call values to Goose Message objects.
fn extract_oai_tool_call_messages(tool_calls: &[Value], message_id: &str) -> Vec<Message> {
    tool_calls
        .iter()
        .filter_map(|tc| {
            let func = tc.get("function")?;
            let name = func.get("name")?.as_str()?;
            if name.is_empty() {
                return None;
            }

            let arguments: Option<serde_json::Map<String, Value>> =
                func.get("arguments").and_then(|a| {
                    if let Some(s) = a.as_str() {
                        serde_json::from_str(s).ok()
                    } else if let Some(obj) = a.as_object() {
                        Some(obj.clone())
                    } else {
                        None
                    }
                });

            let id = tc
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| Uuid::new_v4().to_string());

            let tool_call = match arguments {
                Some(args) => {
                    CallToolRequestParams::new(Cow::Owned(name.to_string())).with_arguments(args)
                }
                None => CallToolRequestParams::new(Cow::Owned(name.to_string())),
            };

            let mut msg = Message::assistant();
            msg.content
                .push(MessageContent::tool_request(id, Ok(tool_call)));
            msg.id = Some(message_id.to_string());
            Some(msg)
        })
        .collect()
}
