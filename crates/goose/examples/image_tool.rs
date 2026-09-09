use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use dotenvy::dotenv;
use goose::conversation::message::Message;
use goose::providers::anthropic::ANTHROPIC_DEFAULT_MODEL;
use goose::providers::create_with_named_model;
use goose::providers::openai::OPEN_AI_DEFAULT_MODEL;
use goose_providers::databricks::DATABRICKS_DEFAULT_MODEL;
use rmcp::model::{CallToolRequestParams, ContentBlock, Tool};
use rmcp::object;
use std::fs;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file
    dotenv().ok();

    // Create providers
    let providers: Vec<(
        Arc<dyn goose::providers::base::Provider>,
        goose_providers::model::ModelConfig,
    )> = vec![
        (
            create_with_named_model("databricks", Vec::new()).await?,
            goose::model_config::model_config_from_user_config(
                "databricks",
                DATABRICKS_DEFAULT_MODEL,
            )?,
        ),
        (
            create_with_named_model("openai", Vec::new()).await?,
            goose::model_config::model_config_from_user_config("openai", OPEN_AI_DEFAULT_MODEL)?,
        ),
        (
            create_with_named_model("anthropic", Vec::new()).await?,
            goose::model_config::model_config_from_user_config(
                "anthropic",
                ANTHROPIC_DEFAULT_MODEL,
            )?,
        ),
    ];
    for (provider, model_config) in providers {
        // Read and encode test image
        let image_data = fs::read("crates/goose/examples/test_assets/test_image.png")?;
        let base64_image = BASE64.encode(image_data);

        // Create a message sequence that includes a tool response with both text and image
        let messages = vec![
            Message::user().with_text("Read the image at ./test_image.png please"),
            Message::assistant().with_tool_request(
                "000",
                Ok(CallToolRequestParams::new("view_image")
                    .with_arguments(object!({"path": "./test_image.png"}))),
            ),
            Message::user().with_tool_response(
                "000",
                Ok(rmcp::model::CallToolResult::success(vec![
                    ContentBlock::image(base64_image, "image/png"),
                ])),
            ),
        ];

        // Get a response from the model about the image
        let input_schema = object!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "default": null,
                    "description": "The path to the image"
                },
            }
        });
        let (response, usage) = provider
            .complete(
                &model_config,
                "You are a helpful assistant. Please describe any text you see in the image.",
                &messages,
                &[Tool::new("view_image", "View an image", input_schema)],
            )
            .await?;

        // Print the response and usage statistics
        println!("\nResponse from AI:");
        println!("---------------");
        for content in response.content {
            println!("{:?}", content);
        }
        println!("\nToken Usage:");
        println!("------------");
        println!("Input tokens: {:?}", usage.usage.input_tokens);
        println!("Output tokens: {:?}", usage.usage.output_tokens);
        println!("Total tokens: {:?}", usage.usage.total_tokens);
    }

    Ok(())
}
