use crate::conversation::message::Message;
use crate::token_counter::create_token_counter;
use anyhow::Result;
use goose_providers::conversation::token_usage::ProviderUsage;
use rmcp::model::Tool;

/// Ensures that ProviderUsage has token counts, estimating them if necessary.
/// This provides a single place to handle the fallback logic for providers that don't return usage data.
pub async fn ensure_usage_tokens(
    provider_usage: &mut ProviderUsage,
    system_prompt: &str,
    request_messages: &[Message],
    response: &Message,
    tools: &[Tool],
) -> Result<()> {
    if provider_usage.usage.input_tokens.is_some() && provider_usage.usage.output_tokens.is_some() {
        return Ok(());
    }

    let token_counter = create_token_counter()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create token counter: {}", e))?;

    if provider_usage.usage.input_tokens.is_none() {
        let input_count = token_counter.count_chat_tokens(system_prompt, request_messages, tools);
        provider_usage.usage.input_tokens = Some(input_count as i64);
    }

    if provider_usage.usage.output_tokens.is_none() {
        let response_text = response
            .content
            .iter()
            .map(|c| format!("{}", c))
            .collect::<Vec<_>>()
            .join(" ");
        let output_count = token_counter.count_tokens(&response_text);
        provider_usage.usage.output_tokens = Some(output_count as i64);
    }

    if let (Some(input), Some(output)) = (
        provider_usage.usage.input_tokens,
        provider_usage.usage.output_tokens,
    ) {
        provider_usage.usage.total_tokens = Some(input.saturating_add(output));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::Message;
    use goose_providers::conversation::token_usage::Usage;

    #[tokio::test]
    async fn test_ensure_usage_tokens_already_complete() {
        let mut usage = ProviderUsage::new(
            "test-model".to_string(),
            Usage::new(Some(100), Some(50), Some(150)),
        );

        let response = Message::assistant().with_text("Test response");

        ensure_usage_tokens(&mut usage, "system", &[], &response, &[])
            .await
            .unwrap();

        // Should remain unchanged
        assert_eq!(usage.usage.input_tokens, Some(100));
        assert_eq!(usage.usage.output_tokens, Some(50));
        assert_eq!(usage.usage.total_tokens, Some(150));
    }

    #[tokio::test]
    async fn test_ensure_usage_tokens_missing_all() {
        let mut usage = ProviderUsage::new("test-model".to_string(), Usage::default());

        let response = Message::assistant().with_text("Test response");
        let messages = vec![Message::user().with_text("Hello")];

        ensure_usage_tokens(
            &mut usage,
            "You are a helpful assistant",
            &messages,
            &response,
            &[],
        )
        .await
        .unwrap();

        // Should have estimated values
        assert!(usage.usage.input_tokens.is_some());
        assert!(usage.usage.output_tokens.is_some());
        assert!(usage.usage.total_tokens.is_some());

        // Basic sanity checks
        assert!(usage.usage.input_tokens.unwrap() > 0);
        assert!(usage.usage.output_tokens.unwrap() > 0);
        assert_eq!(
            usage.usage.total_tokens.unwrap(),
            usage.usage.input_tokens.unwrap() + usage.usage.output_tokens.unwrap()
        );
    }

    #[tokio::test]
    async fn test_ensure_usage_tokens_partial() {
        let mut usage =
            ProviderUsage::new("test-model".to_string(), Usage::new(Some(100), None, None));

        let response = Message::assistant().with_text("Test response");

        ensure_usage_tokens(&mut usage, "system", &[], &response, &[])
            .await
            .unwrap();

        // Input should remain unchanged
        assert_eq!(usage.usage.input_tokens, Some(100));
        // Output should be estimated
        assert!(usage.usage.output_tokens.is_some());
        assert!(usage.usage.output_tokens.unwrap() > 0);
        // Total should be calculated
        assert_eq!(
            usage.usage.total_tokens.unwrap(),
            usage.usage.input_tokens.unwrap() + usage.usage.output_tokens.unwrap()
        );
    }
}
