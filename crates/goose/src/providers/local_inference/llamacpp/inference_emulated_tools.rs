//! Tool call emulation for models without native tool-calling support.
//!
//! The model is prompted to emit shell commands as `$ command` on a new line and
//! code blocks as `` ```execute `` fenced blocks. A streaming parser detects these
//! patterns and converts them into tool-call messages.
//!
//! # Known false-positive scenarios
//!
//! Because detection is purely text-based, the parser can misinterpret model output:
//!
//! - **`$` at line start in explanatory text.** If the model writes a line starting
//!   with `$` as an example (e.g. "$ is the jQuery selector"), it will be treated as
//!   a shell command. Mid-sentence `$` (e.g. "costs $50") is safe — only `\n$` or
//!   `$` at the very start of output triggers command detection.
//!
//! - **`` ```execute `` in explanatory code fences.** If the model uses this exact
//!   fence tag in prose, the content will be executed. Standard `` ```js `` or
//!   `` ```python `` fences are not affected.
//!
//! These are inherent to text-based tool emulation. Models with native tool-calling
//! support should use the `inference_native_tools` path instead.

use crate::providers::errors::ProviderError;
use crate::providers::local_inference::tool_emulation::{
    message_for_emulator_action, StreamingEmulatorParser,
};
use llama_cpp_2::model::AddBos;

use super::super::finalize_usage;
use super::inference_engine::{
    create_and_prefill_context, create_and_prefill_multimodal, generation_loop,
    validate_and_compute_context, GenerationContext, TokenAction,
};

pub(super) fn generate_with_emulated_tools(
    ctx: &mut GenerationContext<'_>,
    code_mode_enabled: bool,
) -> Result<(), ProviderError> {
    // Use oaicompat variant — its C++ wrapper catches exceptions that would
    // otherwise abort the process when other native libs disturb the C++ ABI.
    let prompt = ctx
        .loaded
        .model
        .apply_chat_template_with_tools_oaicompat(
            &ctx.loaded.template,
            ctx.chat_messages,
            None, // no tools for emulated path
            None, // no json_schema
            true, // add_generation_prompt
        )
        .map(|r| r.prompt)
        .map_err(|e| {
            ProviderError::ExecutionError(format!("Failed to apply chat template: {}", e))
        })?;

    let (mut llama_ctx, prompt_token_count, effective_ctx) = if !ctx.images.is_empty() {
        create_and_prefill_multimodal(
            ctx.loaded,
            ctx.backend,
            &prompt,
            ctx.images,
            ctx.context_limit,
            ctx.settings,
        )?
    } else {
        let tokens = ctx
            .loaded
            .model
            .str_to_token(&prompt, AddBos::Never)
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;
        let (ptc, ectx) = validate_and_compute_context(
            ctx.loaded,
            ctx.backend,
            tokens.len(),
            ctx.context_limit,
            ctx.settings,
        )?;
        let lctx =
            create_and_prefill_context(ctx.loaded, ctx.backend, &tokens, ectx, ctx.settings)?;
        (lctx, ptc, ectx)
    };

    let message_id = ctx.message_id;
    let tx = ctx.tx;
    let mut emulator_parser = StreamingEmulatorParser::new(code_mode_enabled);
    let mut tool_call_emitted = false;
    let mut send_failed = false;

    let output_token_count = generation_loop(
        &ctx.loaded.model,
        &mut llama_ctx,
        ctx.settings,
        prompt_token_count,
        effective_ctx,
        |piece| {
            let actions = emulator_parser.process_chunk(piece);
            for action in actions {
                let (message, is_tool) = message_for_emulator_action(&action, message_id);
                if tx.blocking_send(Ok((Some(message), None))).is_err() {
                    send_failed = true;
                    return Ok(TokenAction::Stop);
                }
                if is_tool {
                    tool_call_emitted = true;
                }
            }
            if tool_call_emitted {
                Ok(TokenAction::Stop)
            } else {
                Ok(TokenAction::Continue)
            }
        },
    )?;

    if !send_failed {
        for action in emulator_parser.flush() {
            let (message, _) = message_for_emulator_action(&action, message_id);
            if tx.blocking_send(Ok((Some(message), None))).is_err() {
                break;
            }
        }
    }

    let provider_usage = finalize_usage(
        ctx.log,
        std::mem::take(&mut ctx.model_name),
        "emulator",
        prompt_token_count,
        output_token_count,
        None,
    );
    let _ = ctx.tx.blocking_send(Ok((None, Some(provider_usage))));
    Ok(())
}
