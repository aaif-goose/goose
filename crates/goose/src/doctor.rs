use std::sync::Arc;

use crate::agents::platform_extensions::developer;
use crate::agents::ExtensionConfig;
use crate::config::Config;
use crate::conversation::message::Message;
use crate::providers;
use crate::providers::base::Provider;
use crate::providers::ProviderBackend;
use crate::session::extension_data::EnabledExtensionsState;
use crate::session::{
    config_path, latest_llm_log_path, read_capped, read_tail, recent_cli_log_paths, Session,
    SessionManager, SystemInfo,
};
use goose_providers::errors::ProviderError;

pub(crate) const DEVELOPER_EXTENSION_REQUIRED_MESSAGE: &str = "**Goose Doctor**\n\n\
`/doctor` requires the Developer extension, but it is disabled for this session.\n\n\
Enable it for this session and run `/doctor` again:\n\
- CLI: `/builtin developer`\n\
- Desktop: select **Developer** in the session extension selector.";

pub async fn run(session_manager: &SessionManager, session: &Session) -> anyhow::Result<Message> {
    if let Some(message) = require_developer_extension(session) {
        return Ok(message);
    }

    if let Some(msg) = ensure_working_provider(session_manager, &session.id).await? {
        return Ok(msg);
    }

    let info = SystemInfo::collect();
    let extensions: Vec<String> = session_extensions(session)
        .iter()
        .map(ExtensionConfig::name)
        .collect();

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

    if let Some(path) = recent_cli_log_paths().into_iter().next() {
        if let Some(tail) = read_tail(&path, 50) {
            prompt.push_str(&format!("\nRecent CLI log:\n```\n{}\n```\n", tail));
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

fn session_extensions(session: &Session) -> Vec<ExtensionConfig> {
    EnabledExtensionsState::extensions_or_default(Some(&session.extension_data), Config::global())
}

fn require_developer_extension(session: &Session) -> Option<Message> {
    let has_developer = session_extensions(session)
        .iter()
        .any(is_developer_platform_config);

    (!has_developer).then(|| Message::assistant().with_text(DEVELOPER_EXTENSION_REQUIRED_MESSAGE))
}

fn is_developer_platform_config(config: &ExtensionConfig) -> bool {
    matches!(
        config,
        ExtensionConfig::Builtin { .. } | ExtensionConfig::Platform { .. }
    ) && config.key() == developer::EXTENSION_NAME
}

async fn ensure_working_provider(
    session_manager: &SessionManager,
    session_id: &str,
) -> anyhow::Result<Option<Message>> {
    let config = Config::global();
    let mut log: Vec<String> = Vec::new();

    let provider_name = config.get_goose_provider().ok();
    let model_name = config.get_goose_model().ok();

    if let (Some(ref pname), Some(ref mname)) = (&provider_name, &model_name) {
        if providers::get_from_registry(pname)
            .await
            .is_ok_and(|entry| entry.is_acp())
        {
            return Ok(Some(Message::assistant().with_text(
                "**goose Doctor**\n\nCompletion health checks are not supported for ACP backends. Your configured backend has not been changed.",
            )));
        }
        log.push(format!("Checking {} / {} ...", pname, mname));
        match try_create_and_test(pname, mname).await {
            Ok(_) => return Ok(None),
            Err(e) => {
                log.push(format!("❌ {} / {}: {}", pname, mname, describe_error(&e)));
            }
        }

        log.push(format!("Looking for alternative models on {} ...", pname));
        if let Some((working, model_config)) = try_other_models(pname, mname, &mut log).await {
            let new_model = model_config.model_name.clone();
            save_and_set(session_manager, session_id, working, model_config).await?;
            let preamble = log.join("\n");
            return Ok(Some(Message::assistant().with_text(format!(
                "**goose Doctor**\n\n{}\n\n\
                 Your configured model wasn't working, so I switched to \
                 **{} / {}**. Your next message will use it.",
                preamble, pname, new_model,
            ))));
        }
    } else {
        log.push("No provider/model configured.".to_string());
    }

    log.push("Looking for other configured providers ...".to_string());
    let skip = provider_name.as_deref().unwrap_or("");
    if let Some((working, model_config)) = try_other_providers(skip, &mut log).await {
        let name = working.get_name().to_string();
        let model = model_config.model_name.clone();
        save_and_set(session_manager, session_id, working, model_config).await?;
        let preamble = log.join("\n");
        return Ok(Some(Message::assistant().with_text(format!(
            "**goose Doctor**\n\n{}\n\n\
             Switched to **{} / {}**. Your next message will use it.",
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

async fn save_and_set(
    session_manager: &SessionManager,
    session_id: &str,
    provider: Arc<dyn Provider>,
    model_config: goose_providers::model::ModelConfig,
) -> anyhow::Result<()> {
    let config = Config::global();
    crate::config::set_active_provider(config, provider.get_name(), &model_config.model_name)?;
    session_manager
        .update(session_id)
        .provider_name(provider.get_name())
        .model_config(model_config)
        .apply()
        .await
}

async fn test_provider(
    provider: &dyn Provider,
    model_config: &goose_providers::model::ModelConfig,
) -> Result<(), ProviderError> {
    let messages = vec![Message::user().with_text("Say 'hello' and nothing else.")];
    crate::session_context::with_session_id(
        Some("doctor-check".to_string()),
        provider.complete(
            model_config,
            "Respond as briefly as possible.",
            &messages,
            &[],
        ),
    )
    .await?;
    Ok(())
}

async fn try_create_and_test(
    provider_name: &str,
    model_name: &str,
) -> Result<(Arc<dyn Provider>, goose_providers::model::ModelConfig), ProviderError> {
    let model_config =
        crate::model_config::model_config_from_user_config(provider_name, model_name)
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;

    let provider = providers::create_standard(provider_name, vec![])
        .await
        .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;
    test_provider(provider.as_ref(), &model_config).await?;
    Ok((provider, model_config))
}

async fn try_other_models(
    provider_name: &str,
    skip_model: &str,
    log: &mut Vec<String>,
) -> Option<(Arc<dyn Provider>, goose_providers::model::ModelConfig)> {
    let entry = providers::get_from_registry(provider_name).await.ok()?;
    let ProviderBackend::Standard(temp) =
        entry.create_backend_with_default_model(vec![]).await.ok()?
    else {
        return None;
    };
    let toolshim = Config::global()
        .get_param::<bool>("GOOSE_TOOLSHIM")
        .unwrap_or(false);
    let models = temp.fetch_recommended_models(toolshim).await.ok()?;

    for model in models.iter().filter(|m| m.as_str() != skip_model).take(3) {
        log.push(format!("  Trying {} / {} ...", provider_name, model));
        match try_create_and_test(provider_name, model).await {
            Ok(result) => {
                log.push(format!("  ✓ {} / {} works", provider_name, model));
                return Some(result);
            }
            Err(e) => log.push(format!("  ✗ {}", describe_error(&e))),
        }
    }
    None
}

async fn try_other_providers(
    skip: &str,
    log: &mut Vec<String>,
) -> Option<(Arc<dyn Provider>, goose_providers::model::ModelConfig)> {
    for (meta, _) in providers::providers().await {
        if meta.name == skip {
            continue;
        }
        let entry = match providers::get_from_registry(&meta.name).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.is_acp() {
            continue;
        }
        let model_name = entry.metadata().default_model.clone();
        let model_config =
            match crate::model_config::model_config_from_user_config(&meta.name, &model_name) {
                Ok(config) => config,
                Err(_) => continue,
            };
        let provider = match entry.create_backend_with_default_model(vec![]).await {
            Ok(ProviderBackend::Standard(provider)) => provider,
            _ => continue,
        };
        log.push(format!("  Trying {} / {} ...", meta.name, model_name));
        match test_provider(provider.as_ref(), &model_config).await {
            Ok(()) => {
                log.push(format!("  ✓ {} / {} works", meta.name, model_name));
                return Some((provider, model_config));
            }
            Err(e) => log.push(format!("  ✗ {}", describe_error(&e))),
        }
    }
    None
}

fn describe_error(e: &ProviderError) -> String {
    match e {
        ProviderError::NotConfigured => {
            "Provider is not configured. Run `goose configure` to set it up.".to_string()
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::extension_data::ExtensionState;

    #[tokio::test]
    async fn acp_health_check_never_changes_config_or_session_when_agent_is_unavailable() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_str().unwrap();
        let _env = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", Some(root_path)),
            ("GOOSE_PROVIDER", Some("codex-acp")),
            ("GOOSE_MODEL", Some("current")),
            ("PATH", Some(root_path)),
        ]);
        let manager = SessionManager::new(root.path().to_path_buf());
        let session = manager
            .create_session(
                root.path().to_path_buf(),
                "doctor".into(),
                crate::session::SessionType::Hidden,
                crate::config::GooseMode::Auto,
            )
            .await
            .unwrap();
        manager
            .update(&session.id)
            .provider_name("codex-acp")
            .model_config(goose_providers::model::ModelConfig::new("current"))
            .apply()
            .await
            .unwrap();
        let message = ensure_working_provider(&manager, &session.id)
            .await
            .unwrap()
            .unwrap();
        assert!(message
            .as_concat_text()
            .contains("Completion health checks are not supported"));
        let unchanged = manager.get_session(&session.id, false).await.unwrap();
        assert_eq!(unchanged.provider_name.as_deref(), Some("codex-acp"));
        assert_eq!(unchanged.model_config.unwrap().model_name, "current");
        assert_eq!(Config::global().get_goose_provider().unwrap(), "codex-acp");
    }

    #[test]
    fn developer_requirement_reads_session_extensions() {
        let mut session = Session::default();
        EnabledExtensionsState::new(Vec::new())
            .to_extension_data(&mut session.extension_data)
            .unwrap();
        assert!(require_developer_extension(&session).is_some());

        EnabledExtensionsState::new(vec![ExtensionConfig::Platform {
            name: developer::EXTENSION_NAME.to_string(),
            description: "Developer tools".to_string(),
            display_name: Some("Developer".to_string()),
            bundled: None,
            available_tools: vec![],
        }])
        .to_extension_data(&mut session.extension_data)
        .unwrap();
        assert!(require_developer_extension(&session).is_none());
    }

    #[test]
    fn custom_extension_named_developer_does_not_satisfy_requirement() {
        let config = ExtensionConfig::stdio(
            developer::EXTENSION_NAME,
            "custom-developer",
            "Unrelated custom extension",
            30_u64,
        );

        assert!(!is_developer_platform_config(&config));
    }
}
