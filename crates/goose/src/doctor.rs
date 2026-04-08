use std::sync::Arc;

use crate::agents::platform_extensions::developer;
use crate::agents::ExtensionConfig;
use crate::config::Config;
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::base::Provider;
use crate::providers::{self, errors::ProviderError};
use crate::session::{
    config_path, latest_llm_log_path, latest_server_log_path, read_capped, read_tail, SystemInfo,
};

pub async fn run(agent: &crate::agents::Agent, session_id: &str) -> anyhow::Result<Message> {
    if let Some(msg) = ensure_working_provider(agent, session_id).await? {
        return Ok(msg);
    }

    ensure_developer_extension(agent, session_id).await;

    let info = SystemInfo::collect();
    let extensions = agent.list_extensions().await;

    let mut prompt = format!(
        "I ran /doctor because something seems off. Here's my system info:\n\n\
         {}\n\
         Loaded extensions: {}\n\
         Config file: {}\n",
        info.to_text(),
        if extensions.is_empty() {
            "none".to_string()
        } else {
            extensions.join(", ")
        },
        config_path().display(),
    );

    if let Some(path) = latest_server_log_path() {
        if let Some(tail) = read_tail(&path, 50) {
            prompt.push_str(&format!("\nRecent server log:\n```\n{}\n```\n", tail));
        }
    }

    if let Some(path) = latest_llm_log_path() {
        if let Some(content) = read_capped(&path, 10_000) {
            prompt.push_str(&format!("\nLast LLM request log:\n```\n{}\n```\n", content));
        }
    }

    prompt.push_str(
        "\nUse your tools to investigate what might be wrong. \
         Check if common developer tools are available (git, etc.) \
         and report what you find.",
    );

    Ok(Message::user().with_text(prompt))
}

async fn ensure_working_provider(
    agent: &crate::agents::Agent,
    session_id: &str,
) -> anyhow::Result<Option<Message>> {
    let config = Config::global();
    let mut log: Vec<String> = Vec::new();

    let provider_name = config.get_goose_provider().ok();
    let model_name = config.get_goose_model().ok();

    if let (Some(ref pname), Some(ref mname)) = (&provider_name, &model_name) {
        log.push(format!("Checking {} / {} ...", pname, mname));
        match try_create_and_test(pname, mname).await {
            Ok(_) => {
                return Ok(None);
            }
            Err(e) => {
                log.push(format!("❌ {} / {}: {}", pname, mname, describe_error(&e)));
            }
        }

        log.push(format!("Looking for alternative models on {} ...", pname));
        if let Some(working) = try_other_models(pname, mname, &mut log).await {
            let new_model = working.get_model_config().model_name.clone();
            save_and_set(agent, session_id, working).await?;
            let preamble = log.join("\n");
            return Ok(Some(Message::assistant().with_text(format!(
                "**Goose Doctor**\n\n{}\n\n\
                 Your configured model wasn't working, so I switched to \
                 **{} / {}**. You can continue chatting now.",
                preamble, pname, new_model,
            ))));
        }
    } else {
        log.push("No provider/model configured.".to_string());
    }

    log.push("Looking for other configured providers ...".to_string());
    let skip = provider_name.as_deref().unwrap_or("");
    if let Some(working) = try_other_providers(skip, &mut log).await {
        let name = working.get_name().to_string();
        let model = working.get_model_config().model_name.clone();
        save_and_set(agent, session_id, working).await?;
        let preamble = log.join("\n");
        return Ok(Some(Message::assistant().with_text(format!(
            "**Goose Doctor**\n\n{}\n\n\
             Switched to **{} / {}**. You can continue chatting now.",
            preamble, name, model,
        ))));
    }

    let preamble = log.join("\n");
    Ok(Some(Message::assistant().with_text(format!(
        "**Goose Doctor**\n\n{}\n\n\
         No working provider found. Run `goose configure` to set one up.",
        preamble,
    ))))
}

async fn ensure_developer_extension(agent: &crate::agents::Agent, session_id: &str) {
    if agent
        .extension_manager
        .is_extension_enabled(developer::EXTENSION_NAME)
        .await
    {
        return;
    }
    let config = ExtensionConfig::Platform {
        name: developer::EXTENSION_NAME.to_string(),
        description: "Write and edit files, and execute shell commands".to_string(),
        display_name: Some("Developer".to_string()),
        bundled: None,
        available_tools: vec![],
    };
    if let Err(e) = agent.add_extension(config, session_id).await {
        tracing::warn!("Doctor: failed to load developer extension: {}", e);
    }
}

async fn save_and_set(
    agent: &crate::agents::Agent,
    session_id: &str,
    provider: Arc<dyn Provider>,
) -> anyhow::Result<()> {
    let config = Config::global();
    config.set_goose_provider(provider.get_name()).ok();
    config
        .set_goose_model(&provider.get_model_config().model_name)
        .ok();
    agent.update_provider(provider, session_id).await
}

async fn test_provider(provider: &dyn Provider) -> Result<(), ProviderError> {
    let messages = vec![Message::user().with_text("Say 'hello' and nothing else.")];
    provider
        .complete(
            &provider.get_model_config(),
            "doctor-check",
            "Respond as briefly as possible.",
            &messages,
            &[],
        )
        .await?;
    Ok(())
}

async fn try_create_and_test(
    provider_name: &str,
    model_name: &str,
) -> Result<Arc<dyn Provider>, ProviderError> {
    let model_config = ModelConfig::new(model_name)
        .map_err(|e| ProviderError::ExecutionError(e.to_string()))?
        .with_canonical_limits(provider_name);

    let provider = providers::create(provider_name, model_config, vec![])
        .await
        .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;

    test_provider(provider.as_ref()).await?;
    Ok(provider)
}

async fn try_other_models(
    provider_name: &str,
    skip_model: &str,
    log: &mut Vec<String>,
) -> Option<Arc<dyn Provider>> {
    let entry = providers::get_from_registry(provider_name).await.ok()?;
    let temp = entry.create_with_default_model(vec![]).await.ok()?;
    let models = temp.fetch_recommended_models().await.ok()?;

    for model in models.iter().filter(|m| m.as_str() != skip_model).take(3) {
        log.push(format!("  Trying {} / {} ...", provider_name, model));
        match try_create_and_test(provider_name, model).await {
            Ok(p) => {
                log.push(format!("  ✓ {} / {} works", provider_name, model));
                return Some(p);
            }
            Err(e) => log.push(format!("  ✗ {}", describe_error(&e))),
        }
    }
    None
}

async fn try_other_providers(skip: &str, log: &mut Vec<String>) -> Option<Arc<dyn Provider>> {
    for (meta, _) in providers::providers().await {
        if meta.name == skip {
            continue;
        }
        let entry = match providers::get_from_registry(&meta.name).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        let provider = match entry.create_with_default_model(vec![]).await {
            Ok(p) => p,
            Err(_) => continue,
        };
        let model_name = provider.get_model_config().model_name.clone();
        log.push(format!("  Trying {} / {} ...", meta.name, model_name));
        match test_provider(provider.as_ref()).await {
            Ok(()) => {
                log.push(format!("  ✓ {} / {} works", meta.name, model_name));
                return Some(provider);
            }
            Err(e) => log.push(format!("  ✗ {}", describe_error(&e))),
        }
    }
    None
}

fn describe_error(e: &ProviderError) -> String {
    match e {
        ProviderError::Authentication(_) => {
            "Authentication failed — check your API key. Run `goose configure` to update it."
                .to_string()
        }
        ProviderError::CreditsExhausted { top_up_url, .. } => {
            let mut msg = "Credits exhausted.".to_string();
            if let Some(url) = top_up_url {
                msg.push_str(&format!(" Top up at: {}", url));
            }
            msg
        }
        ProviderError::RateLimitExceeded { .. } => {
            "Rate limited — wait a moment and try again.".to_string()
        }
        ProviderError::EndpointNotFound(_) => {
            "Model not found — the model name may be wrong for this provider.".to_string()
        }
        ProviderError::NetworkError(_) => {
            "Network error — check your internet connection.".to_string()
        }
        ProviderError::ServerError(_) => {
            "Provider server error — the service may be temporarily down.".to_string()
        }
        other => format!("{}", other),
    }
}
