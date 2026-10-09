use dotenvy::dotenv;
use futures::StreamExt;
use goose::agents::{AgentEvent, ExtensionConfig, SessionConfig, StateMachineServices};
use goose::config::{GooseMode, DEFAULT_EXTENSION_DESCRIPTION, DEFAULT_EXTENSION_TIMEOUT};
use goose::conversation::message::Message;
use goose::providers::create;
use goose::session::session_manager::SessionType;
use goose_providers::databricks::DATABRICKS_DEFAULT_MODEL;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenv();

    let provider = create("databricks", Vec::new()).await?;
    let model_config =
        goose::model_config::model_config_from_user_config("databricks", DATABRICKS_DEFAULT_MODEL)?;

    let agent = StateMachineServices::new();

    let session = agent
        .config
        .session_manager
        .create_session(
            PathBuf::default(),
            "max-turn-test".to_string(),
            SessionType::Hidden,
            GooseMode::default(),
        )
        .await?;

    agent
        .update_provider(provider, model_config, &session.id)
        .await?;

    let config = ExtensionConfig::stdio(
        "developer",
        "./target/debug/goose",
        DEFAULT_EXTENSION_DESCRIPTION,
        DEFAULT_EXTENSION_TIMEOUT,
    )
    .with_args(vec!["mcp", "developer"]);
    agent.extension_manager.enable(&session.id, config).await?;

    println!("Extensions:");
    for extension in agent.extension_manager.list_extensions(&session.id).await? {
        println!("  {}", extension);
    }

    let session_config = SessionConfig {
        id: session.id,
        schedule_id: None,
        max_turns: None,
    };

    let user_message = Message::user()
        .with_text("can you summarize the readme.md in this dir using just a haiku?");

    let mut stream = agent.reply(user_message, session_config, None).await?;

    while let Some(Ok(AgentEvent::Message(message))) = stream.next().await {
        println!("{}", serde_json::to_string_pretty(&message)?);
        println!("\n");
    }

    Ok(())
}
