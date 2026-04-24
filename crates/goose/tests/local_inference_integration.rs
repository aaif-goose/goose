//! Integration tests for LocalInferenceProvider.
//!
//! These tests require a downloaded GGUF model and are ignored by default.
//! Download a model first:
//!   goose local-models download bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M
//!
//! Run with the default model:
//!   cargo test -p goose --test local_inference_integration -- --ignored
//!
//! Run with a specific model:
//!   TEST_MODEL="bartowski/Qwen_Qwen3-32B-GGUF:Q4_K_M" cargo test -p goose --test local_inference_integration -- --ignored
//!
//! Run vision tests (requires a vision-capable model like gemma-4):
//!   TEST_VISION_MODEL="unsloth/gemma-4-E4B-it-GGUF:Q4_K_M" cargo test -p goose --test local_inference_integration test_local_inference_vision -- --ignored

use base64::prelude::*;
use futures::StreamExt;
use goose::conversation::message::Message;
use goose::model::ModelConfig;
use goose::providers::create;

const DEFAULT_TEST_MODEL: &str = "bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M";

fn test_model() -> String {
    std::env::var("TEST_MODEL").unwrap_or_else(|_| DEFAULT_TEST_MODEL.to_string())
}

#[tokio::test]
#[ignore]
async fn test_local_inference_stream_produces_output() {
    let model_config = ModelConfig::new(&test_model()).expect("valid model config");
    let provider = create("local", model_config.clone(), Vec::new())
        .await
        .expect("provider creation should succeed");

    let system = "You are a helpful assistant. Be brief.";
    let messages = vec![Message::user().with_text("Say hello.")];

    let mut stream = provider
        .stream(&model_config, "test-session", system, &messages, &[])
        .await
        .expect("stream should start");

    let mut got_text = false;
    let mut got_usage = false;

    while let Some(result) = stream.next().await {
        let (msg, usage) = result.expect("stream item should be Ok");
        if msg.is_some() {
            got_text = true;
        }
        if let Some(u) = usage {
            got_usage = true;
            let usage_inner = u.usage;
            assert!(
                usage_inner.input_tokens.unwrap_or(0) > 0,
                "should have input tokens"
            );
            assert!(
                usage_inner.output_tokens.unwrap_or(0) > 0,
                "should have output tokens"
            );
        }
    }

    assert!(got_text, "stream should produce at least one text message");
    assert!(got_usage, "stream should produce usage info");
}

#[tokio::test]
#[ignore]
async fn test_local_inference_large_prompt() {
    let model_config = ModelConfig::new(&test_model())
        .expect("valid model config")
        .with_max_tokens(Some(20));
    let provider = create("local", model_config.clone(), Vec::new())
        .await
        .expect("provider creation should succeed");

    // Build a large prompt (~3500 tokens) to exercise prefill performance
    let padding = "You are Goose, a highly capable AI assistant.\n".repeat(80);
    let prompt = format!("{padding}\nNow answer this: what is the capital of Moldova?");
    let messages = vec![Message::user().with_text(&prompt)];

    let start = std::time::Instant::now();
    let (response, _usage) = provider
        .complete(&model_config, "test-session", "", &messages, &[])
        .await
        .expect("large prompt completion should succeed");
    let elapsed = start.elapsed();

    let text = response.as_concat_text();
    assert!(!text.is_empty(), "large prompt should produce a response");
    println!(
        "Large prompt: {elapsed:.2?}, prompt ~{} chars, response length: {}",
        prompt.len(),
        text.len()
    );
}

fn vision_test_model() -> Option<String> {
    std::env::var("TEST_VISION_MODEL").ok()
}

/// Generate a small solid-colour 2x2 red PNG as raw bytes.
fn tiny_red_png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, // RGB, 8-bit
        0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, // IDAT chunk
        0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x36, 0x28, 0x19,
        0x00, // compressed pixel data
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82, // IEND
    ]
}

/// Test that a vision-capable local model can process a message with an embedded image
/// and produce a text response without crashing.
///
/// Requires TEST_VISION_MODEL to be set to a downloaded vision model.
/// Example:
///   TEST_VISION_MODEL="unsloth/gemma-4-E4B-it-GGUF:Q4_K_M" \
///     cargo test -p goose --test local_inference_integration test_local_inference_vision -- --ignored
#[tokio::test]
#[ignore]
async fn test_local_inference_vision_produces_output() {
    let model_id = match vision_test_model() {
        Some(id) => id,
        None => {
            eprintln!(
                "Skipping vision test: TEST_VISION_MODEL not set. \
                 Set it to a vision-capable model like unsloth/gemma-4-E4B-it-GGUF:Q4_K_M"
            );
            return;
        }
    };

    let model_config = ModelConfig::new(&model_id).expect("valid model config");
    let provider = create("local", model_config.clone(), Vec::new())
        .await
        .expect("provider creation should succeed");

    let image_bytes = tiny_red_png();
    let image_b64 = BASE64_STANDARD.encode(&image_bytes);

    let system = "You are a helpful assistant. Describe images briefly.";
    let messages = vec![Message::user()
        .with_text("What color is this image?")
        .with_image(image_b64, "image/png")];

    let mut stream = provider
        .stream(&model_config, "test-vision-session", system, &messages, &[])
        .await
        .expect("stream should start for vision input");

    let mut got_text = false;
    let mut collected_text = String::new();

    while let Some(result) = stream.next().await {
        let (msg, _usage) = result.expect("stream item should be Ok");
        if let Some(m) = msg {
            got_text = true;
            collected_text.push_str(&m.as_concat_text());
        }
    }

    assert!(
        got_text,
        "vision stream should produce at least one text message"
    );
    assert!(
        !collected_text.is_empty(),
        "vision response should contain text"
    );
    println!("Vision response: {collected_text}");
}

/// Test that sending an image to a text-only model produces a clear error
/// rather than crashing.
#[tokio::test]
#[ignore]
async fn test_local_inference_vision_text_only_model_graceful() {
    let model_config = ModelConfig::new(&test_model()).expect("valid model config");
    let provider = create("local", model_config.clone(), Vec::new())
        .await
        .expect("provider creation should succeed");

    let image_bytes = tiny_red_png();
    let image_b64 = BASE64_STANDARD.encode(&image_bytes);

    let system = "You are a helpful assistant.";
    let messages = vec![Message::user()
        .with_text("What is this?")
        .with_image(image_b64, "image/png")];

    let mut stream = provider
        .stream(&model_config, "test-session", system, &messages, &[])
        .await
        .expect("stream should start");

    // The stream should either produce a response with the image stripped
    // (placeholder text) or produce an error — but it must not crash.
    let mut completed = false;
    while let Some(result) = stream.next().await {
        match result {
            Ok(_) => completed = true,
            Err(e) => {
                // An error about missing vision support is acceptable
                let err_msg = e.to_string();
                assert!(
                    err_msg.contains("vision") || err_msg.contains("image"),
                    "error should mention vision/image support, got: {err_msg}"
                );
                completed = true;
                break;
            }
        }
    }

    assert!(
        completed,
        "stream should complete without crashing when images sent to text-only model"
    );
}
