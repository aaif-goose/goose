use crate::conversation::message::Message;
use crate::providers::errors::ProviderError;
use llama_cpp_2::model::AddBos;
use llama_cpp_2::openai::OpenAIChatTemplateParams;

use super::finalize_usage;
use super::inference_engine::{
    context_cap, create_and_prefill_context, estimate_max_context_for_memory, generation_loop,
    validate_and_compute_context, GenerationContext, TokenAction,
};
use super::tool_parsing::{
    extract_gemma4_tool_call_messages, extract_tool_call_messages, extract_xml_tool_call_messages,
    safe_stream_end, split_content_and_gemma4_tool_calls, split_content_and_tool_calls,
    split_content_and_xml_tool_calls,
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
    let mut streamed_len: usize = 0;

    let output_token_count = generation_loop(
        &ctx.loaded.model,
        &mut llama_ctx,
        ctx.settings,
        prompt_token_count,
        effective_ctx,
        |piece| {
            generated_text.push_str(piece);

            let has_gemma4_tc = split_content_and_gemma4_tool_calls(&generated_text).is_some();
            let has_xml_tc = split_content_and_xml_tool_calls(&generated_text).is_some();
            let (content, tc) = split_content_and_tool_calls(&generated_text);
            let stream_up_to = if has_gemma4_tc {
                split_content_and_gemma4_tool_calls(&generated_text)
                    .map(|(c, _)| c.len())
                    .unwrap_or(0)
            } else if tc.is_some() {
                content.len()
            } else if has_xml_tc {
                split_content_and_xml_tool_calls(&generated_text)
                    .map(|(c, _)| c.len())
                    .unwrap_or(0)
            } else {
                safe_stream_end(&generated_text)
            };
            if stream_up_to > streamed_len {
                #[allow(clippy::string_slice)]
                let new_text = &generated_text[streamed_len..stream_up_to];
                if !new_text.is_empty() {
                    let mut msg = Message::assistant().with_text(new_text);
                    msg.id = Some(message_id.to_string());
                    if tx.blocking_send(Ok((Some(msg), None))).is_err() {
                        return Ok(TokenAction::Stop);
                    }
                }
                streamed_len = stream_up_to;
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

    let (content, tool_call_msgs) = if let Some((gemma4_content, gemma4_calls)) =
        split_content_and_gemma4_tool_calls(&generated_text)
    {
        let msgs = extract_gemma4_tool_call_messages(gemma4_calls, message_id);
        (gemma4_content, msgs)
    } else if let Some((xml_content, xml_calls)) =
        split_content_and_xml_tool_calls(&generated_text)
    {
        let msgs = extract_xml_tool_call_messages(xml_calls, message_id);
        (xml_content, msgs)
    } else {
        let (json_content, tool_calls_json) = split_content_and_tool_calls(&generated_text);
        let msgs = tool_calls_json
            .map(|tc| extract_tool_call_messages(&tc, message_id))
            .unwrap_or_default();
        (json_content, msgs)
    };

    if content.len() > streamed_len {
        #[allow(clippy::string_slice)]
        let remaining = &content[streamed_len..];
        if !remaining.is_empty() {
            let mut msg = Message::assistant().with_text(remaining);
            msg.id = Some(message_id.to_string());
            let _ = tx.blocking_send(Ok((Some(msg), None)));
        }
    }

    if !tool_call_msgs.is_empty() {
        for msg in tool_call_msgs {
            let _ = tx.blocking_send(Ok((Some(msg), None)));
        }
    } else if content.is_empty() && !generated_text.is_empty() {
        let mut msg = Message::assistant().with_text(&generated_text);
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
