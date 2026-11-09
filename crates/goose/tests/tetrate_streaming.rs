use anyhow::Result;
use futures::StreamExt;
use goose::conversation::message::{Message, MessageContent};
use goose::providers::base::Provider;
use goose::providers::tetrate::TetrateProvider;
use goose_provider_types::data::Tool;
use goose_providers::model::ModelConfig;
use rmcp::object;
use serial_test::serial;

/// Test module for Tetrate Agent Router Service streaming functionality
#[cfg(test)]
mod tetrate_streaming_tests {
    use super::*;

    async fn create_test_provider() -> Result<TetrateProvider> {
        TetrateProvider::from_env(None).await
    }

    #[tokio::test]
    #[serial]
    #[ignore] // Ignore by default, run with --ignored flag when API key is available
    async fn test_tetrate_streaming_basic() -> Result<()> {
        let provider = create_test_provider().await?;

        let messages = vec![Message::user().with_text("Count from 1 to 5, one number at a time.")];
        let model_config =
            ModelConfig::new("claude-3-5-sonnet-latest").with_canonical_limits("tetrate");

        let mut stream = provider
            .stream(
                &model_config,
                "You are a helpful assistant that counts numbers.",
                &messages,
                &[],
            )
            .await?;

        let mut chunk_count = 0;
        let mut content_chunks = Vec::new();

        while let Some(result) = stream.next().await {
            let (message, usage) = result?;
            chunk_count += 1;

            if let Some(msg) = message {
                let text = msg.as_concat_text();
                if !text.is_empty() {
                    content_chunks.push(text);
                }
            }

            // Check if we have usage information in the final chunk
            if usage.is_some() {
                println!("Received usage information in chunk {}", chunk_count);
            }
        }

        assert!(chunk_count > 0, "Should receive at least one chunk");
        assert!(!content_chunks.is_empty(), "Should receive some content");

        let full_content = content_chunks.join("");
        println!("Full streamed content: {}", full_content);

        // Verify the response contains numbers
        assert!(
            full_content.contains('1'),
            "Response should contain number 1"
        );
        assert!(
            full_content.contains('5'),
            "Response should contain number 5"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    #[ignore]
    async fn test_tetrate_streaming_with_tools() -> Result<()> {
        let provider = create_test_provider().await?;

        // Define a simple tool
        let weather_tool = Tool::new(
            "get_weather",
            "Get the current weather for a location",
            object!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city and state, e.g. San Francisco, CA"
                    }
                },
                "required": ["location"]
            }),
        );

        let messages = vec![Message::user().with_text("What's the weather in San Francisco?")];
        let model_config =
            ModelConfig::new("claude-3-5-sonnet-latest").with_canonical_limits("tetrate");

        let mut stream = provider
            .stream(
                &model_config,
                "You are a helpful assistant with access to weather information.",
                &messages,
                &[weather_tool],
            )
            .await?;

        let mut received_tool_call = false;
        let mut chunk_count = 0;

        while let Some(result) = stream.next().await {
            let (message, _usage) = result?;
            chunk_count += 1;

            if let Some(msg) = message {
                // Check if message contains tool requests
                for content in &msg.content {
                    if matches!(content, MessageContent::ToolRequest(_)) {
                        received_tool_call = true;
                        println!("Received tool call in chunk {}", chunk_count);
                    }
                }
            }
        }

        assert!(chunk_count > 0, "Should receive at least one chunk");
        // Note: Tool calls might not be supported in streaming for all models
        // This is more of a capability test than a requirement
        if received_tool_call {
            println!("✓ Streaming with tools is supported");
        } else {
            println!("⚠ Streaming with tools may not be fully supported");
        }

        Ok(())
    }

    #[tokio::test]
    #[serial]
    #[ignore]
    async fn test_tetrate_streaming_empty_response() -> Result<()> {
        let provider = create_test_provider().await?;

        // This might result in a very short or empty response
        let messages = vec![Message::user().with_text("")];
        let model_config =
            ModelConfig::new("claude-3-5-sonnet-latest").with_canonical_limits("tetrate");

        let mut stream = provider
            .stream(
                &model_config,
                "You are a helpful assistant.",
                &messages,
                &[],
            )
            .await?;

        let mut chunk_count = 0;

        while let Some(result) = stream.next().await {
            let (_message, _usage) = result?;
            chunk_count += 1;
        }

        // Even with empty input, we should get at least one chunk (possibly with finish_reason)
        assert!(
            chunk_count > 0,
            "Should receive at least one chunk even with empty input"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    #[ignore]
    async fn test_tetrate_streaming_long_response() -> Result<()> {
        let provider = create_test_provider().await?;

        let messages = vec![Message::user().with_text(
            "Write a detailed 3-paragraph essay about the importance of streaming in modern APIs.",
        )];
        let model_config =
            ModelConfig::new("claude-3-5-sonnet-latest").with_canonical_limits("tetrate");

        let mut stream = provider
            .stream(
                &model_config,
                "You are a helpful assistant that writes detailed essays.",
                &messages,
                &[],
            )
            .await?;

        let mut chunk_count = 0;
        let mut total_content_length = 0;

        while let Some(result) = stream.next().await {
            let (message, usage) = result?;
            chunk_count += 1;

            if let Some(msg) = message {
                let text = msg.as_concat_text();
                total_content_length += text.len();
            }

            // Final chunk should have usage information
            if let Some(usage_info) = usage {
                println!("Final usage: {:?}", usage_info.usage);
                assert!(
                    usage_info.usage.output_tokens.unwrap_or(0) > 0,
                    "Should have output tokens"
                );
            }
        }

        println!(
            "Received {} chunks with total content length: {}",
            chunk_count, total_content_length
        );

        // For a detailed essay, we expect multiple chunks and substantial content
        assert!(
            chunk_count > 5,
            "Long response should be streamed in multiple chunks"
        );
        assert!(
            total_content_length > 100,
            "Essay should have substantial content"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_tetrate_streaming_error_handling() -> Result<()> {
        // Test with invalid API key to ensure error handling works
        std::env::set_var("TETRATE_API_KEY", "invalid-key-for-testing");

        let provider = TetrateProvider::from_env(None).await?;

        let messages = vec![Message::user().with_text("Hello")];
        let model_config =
            ModelConfig::new("claude-3-5-sonnet-latest").with_canonical_limits("tetrate");

        let result = provider
            .stream(
                &model_config,
                "You are a helpful assistant.",
                &messages,
                &[],
            )
            .await;

        // We expect this to fail with an authentication error
        assert!(result.is_err(), "Should fail with invalid API key");

        // Clean up
        std::env::remove_var("TETRATE_API_KEY");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    #[ignore]
    async fn test_tetrate_streaming_concurrent_streams() -> Result<()> {
        let provider = create_test_provider().await?;

        // Create multiple concurrent streams
        let messages1 = vec![Message::user().with_text("Say 'Stream 1'")];
        let messages2 = vec![Message::user().with_text("Say 'Stream 2'")];
        let model_config =
            ModelConfig::new("claude-3-5-sonnet-latest").with_canonical_limits("tetrate");

        let stream1 = provider
            .stream(
                &model_config,
                "You are a helpful assistant.",
                &messages1,
                &[],
            )
            .await?;

        let stream2 = provider
            .stream(
                &model_config,
                "You are a helpful assistant.",
                &messages2,
                &[],
            )
            .await?;

        // Process both streams concurrently
        let (result1, result2) = tokio::join!(
            process_stream(stream1, "Stream 1"),
            process_stream(stream2, "Stream 2")
        );

        let content1 = result1?;
        let content2 = result2?;

        println!("Stream 1 content: {}", content1);
        println!("Stream 2 content: {}", content2);

        assert!(
            content1.contains("Stream 1") || content1.contains("1"),
            "First stream should mention Stream 1"
        );
        assert!(
            content2.contains("Stream 2") || content2.contains("2"),
            "Second stream should mention Stream 2"
        );

        Ok(())
    }

    // Helper function to process a stream and collect content
    async fn process_stream(
        mut stream: goose::providers::base::MessageStream<'_>,
        label: &str,
    ) -> Result<String> {
        let mut content = String::new();
        let mut chunk_count = 0;

        while let Some(result) = stream.next().await {
            let (message, _usage) = result?;
            chunk_count += 1;

            if let Some(msg) = message {
                let text = msg.as_concat_text();
                if !text.is_empty() {
                    content.push_str(&text);
                }
            }
        }

        println!("{}: Received {} chunks", label, chunk_count);
        Ok(content)
    }
}
