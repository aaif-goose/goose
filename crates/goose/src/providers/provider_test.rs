use crate::{conversation::message::Message, providers::create_standard};
use anyhow::Result;
use futures::StreamExt;
use rmcp::model::ToolAnnotations;
use rmcp::{model::Tool, object};

pub async fn test_provider_configuration(
    provider_name: &str,
    model: &str,
    toolshim_enabled: bool,
    toolshim_model: Option<String>,
) -> Result<()> {
    let model_config = crate::model_config::model_config_from_user_config(provider_name, model)?
        .with_max_tokens(Some(50))
        .with_toolshim(toolshim_enabled)
        .with_toolshim_model(toolshim_model);

    let provider = create_standard(provider_name, Vec::new()).await?;

    let messages =
        vec![Message::user().with_text("What is the weather like in San Francisco today?")];

    let tools = if !toolshim_enabled {
        vec![create_sample_weather_tool()]
    } else {
        vec![]
    };

    let mut stream = crate::session_context::with_session_id(
        Some("test-session-id".to_string()),
        provider.stream(
            &model_config,
            "You are an AI agent called goose. You use tools of connected extensions to solve problems.",
            &messages,
            &tools.into_iter().collect::<Vec<_>>(),
        ),
    )
    .await?;

    let first_chunk = stream
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("Provider test stream returned no events"))?;
    first_chunk?;

    Ok(())
}

fn create_sample_weather_tool() -> Tool {
    Tool::new(
        "get_weather".to_string(),
        "Get current temperature for a given location.".to_string(),
        object!({
            "type": "object",
            "required": ["location"],
            "properties": {
                "location": {"type": "string"}
            }
        }),
    )
    .annotate(
        ToolAnnotations::with_title("Get weather".to_string())
            .read_only(true)
            .destructive(false)
            .idempotent(false)
            .open_world(false),
    )
}
