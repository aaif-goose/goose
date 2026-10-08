//! Performance benchmarks for LocalInferenceProvider.
//!
//! These tests require a downloaded GGUF model and are ignored by default.
//! Download a model first:
//!   goose local-models download bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M
//!
//! Run with the default model:
//!   cargo test -p goose --test local_inference_perf -- --ignored --nocapture
//!
//! Run with a specific model:
//!   TEST_MODEL="bartowski/Qwen_Qwen3-32B-GGUF:Q4_K_M" cargo test -p goose --test local_inference_perf -- --ignored --nocapture

use goose::conversation::message::Message;
use goose::providers::create_standard;
use goose_providers::model::ModelConfig;
use std::time::Instant;

const DEFAULT_TEST_MODEL: &str = "bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M";

fn test_model() -> String {
    std::env::var("TEST_MODEL").unwrap_or_else(|_| DEFAULT_TEST_MODEL.to_string())
}

#[tokio::test]
#[ignore]
async fn test_local_inference_cold_vs_warm() {
    let model_config = ModelConfig::new(test_model()).with_max_tokens(Some(20));
    let provider = create_standard("local", Vec::new())
        .await
        .expect("provider creation should succeed");

    // Cold start — includes model loading from disk.
    let messages = vec![Message::user().with_text("What is 2+2?")];
    let start = Instant::now();
    let (response, _) = provider
        .complete(&model_config, "", &messages, &[])
        .await
        .expect("cold completion should succeed");
    let cold_elapsed = start.elapsed();

    let text = response.as_concat_text();
    assert!(!text.is_empty(), "cold start should produce a response");
    println!("Cold start: {cold_elapsed:.2?}, response: {}", text.len());

    // Warm run — model already loaded, only inference.
    let messages2 = vec![Message::user().with_text("What is 3+3?")];
    let start2 = Instant::now();
    let (response2, _) = provider
        .complete(&model_config, "", &messages2, &[])
        .await
        .expect("warm completion should succeed");
    let warm_elapsed = start2.elapsed();

    let text2 = response2.as_concat_text();
    assert!(!text2.is_empty(), "warm run should produce a response");
    println!("Warm run:   {warm_elapsed:.2?}, response: {}", text2.len());

    if warm_elapsed < cold_elapsed {
        let speedup = cold_elapsed.as_secs_f64() / warm_elapsed.as_secs_f64();
        println!("Warm is {speedup:.1}x faster than cold");
    } else {
        println!("Warning: warm was not faster (model may have been pre-loaded by another test)");
    }
}
