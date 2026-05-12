use anyhow::{anyhow, Result};
use console::style;
use goose::config::paths::Paths;
use goose::config::Config;
use goose::conversation::message::Message;
use goose::providers::errors::ProviderError;
use goose::session::session_manager::{DB_NAME, SESSIONS_FOLDER};
use serde_yaml;
use std::time::Duration;

fn print_aligned(label: &str, value: &str, width: usize) {
    println!("  {:<width$} {}", label, value, width = width);
}

use goose::config::base::CONFIG_YAML_NAME;
use std::fs;
use std::path::Path;

fn check_path_status(path: &Path) -> String {
    if path.exists() {
        "".to_string()
    } else {
        let mut current = path.parent();
        while let Some(parent) = current {
            if parent.exists() {
                return match fs::metadata(parent).map(|m| !m.permissions().readonly()) {
                    Ok(true) => style("missing (can create)").dim().to_string(),
                    Ok(false) => style("missing (read-only parent)").red().to_string(),
                    Err(_) => style("missing (cannot check)").red().to_string(),
                };
            }
            current = parent.parent();
        }
        style("missing (no writable parent)").red().to_string()
    }
}

struct ProviderCheckSuccess {
    provider: String,
    model: String,
    elapsed: Duration,
}

enum ProviderCheckError {
    NotConfigured {
        label: &'static str,
        error: String,
    },
    InvalidModel(String),
    ProviderCreate {
        error: String,
        show_api_key_hint: bool,
    },
    ProviderRequest(ProviderError),
}

async fn check_provider(
    config: &Config,
) -> std::result::Result<ProviderCheckSuccess, ProviderCheckError> {
    let (provider, model) = match (config.get_goose_provider(), config.get_goose_model()) {
        (Ok(provider), Ok(model)) => (provider, model),
        (Err(e), _) => {
            return Err(ProviderCheckError::NotConfigured {
                label: "Provider:",
                error: e.to_string(),
            });
        }
        (_, Err(e)) => {
            return Err(ProviderCheckError::NotConfigured {
                label: "Model:",
                error: e.to_string(),
            });
        }
    };

    let model_config = goose::model::ModelConfig::new(&model)
        .map_err(|e| ProviderCheckError::InvalidModel(e.to_string()))?
        .with_canonical_limits(&provider);

    let provider_client = goose::providers::create(&provider, model_config, Vec::new())
        .await
        .map_err(|e| {
            let error = e.to_string();
            ProviderCheckError::ProviderCreate {
                show_api_key_hint: error.contains("not found") || error.contains("API_KEY"),
                error,
            }
        })?;

    let test_msg = Message::user().with_text("Say 'ok'");
    let model_config = provider_client.get_model_config();
    let start = std::time::Instant::now();
    provider_client
        .complete(&model_config, "check", "", &[test_msg], &[])
        .await
        .map_err(ProviderCheckError::ProviderRequest)?;

    Ok(ProviderCheckSuccess {
        provider,
        model,
        elapsed: start.elapsed(),
    })
}

pub async fn handle_info(verbose: bool, check: bool) -> Result<()> {
    let logs_dir = Paths::in_state_dir("logs");
    let sessions_dir = Paths::in_data_dir(SESSIONS_FOLDER);
    let sessions_db = sessions_dir.join(DB_NAME);
    let config = Config::global();
    let config_dir = Paths::config_dir();
    let config_yaml_file = config_dir.join(CONFIG_YAML_NAME);

    let paths = [
        ("Config dir:", &config_dir),
        ("Config yaml:", &config_yaml_file),
        ("Sessions DB (sqlite):", &sessions_db),
        ("Logs dir:", &logs_dir),
    ];

    let label_padding = paths.iter().map(|(l, _)| l.len()).max().unwrap_or(0) + 4;
    let path_padding = paths
        .iter()
        .map(|(_, p)| p.display().to_string().len())
        .max()
        .unwrap_or(0)
        + 4;

    println!("{}", style("goose Version:").cyan().bold());
    print_aligned("Version:", env!("CARGO_PKG_VERSION"), label_padding);
    println!();

    println!("{}", style("Paths:").cyan().bold());
    for (label, path) in &paths {
        println!(
            "{:<label_padding$}{:<path_padding$}{}",
            label,
            path.display(),
            check_path_status(path)
        );
    }

    if verbose {
        println!("\n{}", style("goose Configuration:").cyan().bold());
        let values = config.all_values()?;
        if values.is_empty() {
            println!("  No configuration values set");
            println!(
                "  Run '{}' to configure goose",
                style("goose configure").cyan()
            );
        } else {
            let sorted_values: std::collections::BTreeMap<_, _> =
                values.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

            if let Ok(yaml) = serde_yaml::to_string(&sorted_values) {
                for line in yaml.lines() {
                    println!("  {}", line);
                }
            }
        }
    }

    if check {
        println!("\n{}", style("Provider Check:").cyan().bold());

        let result = check_provider(config).await;
        match &result {
            Ok(success) => {
                print_aligned("Provider:", &success.provider, label_padding);
                print_aligned("Model:", &success.model, label_padding);
                print_aligned("Auth:", &style("ok").green().to_string(), label_padding);
                print_aligned(
                    "Connection:",
                    &format!(
                        "{} (verified in {:.1}s)",
                        style("ok").green(),
                        success.elapsed.as_secs_f64()
                    ),
                    label_padding,
                );
            }
            Err(ProviderCheckError::NotConfigured { label, error }) => {
                print_aligned(
                    label,
                    &format!("{} {}", style("not configured:").red(), error),
                    label_padding,
                );
                print_aligned(
                    "Hint:",
                    &format!("Run '{}'", style("goose configure").cyan()),
                    label_padding,
                );
            }
            Err(ProviderCheckError::InvalidModel(error)) => {
                print_aligned(
                    "Model:",
                    &format!("{} {}", style("invalid:").red(), error),
                    label_padding,
                );
            }
            Err(ProviderCheckError::ProviderCreate {
                error,
                show_api_key_hint,
            }) => {
                // Split auth failures (missing/invalid credential) from provider
                // construction failures (unknown provider, malformed provider
                // config). Labeling the latter as "Auth: FAILED" misdirects
                // troubleshooting toward rotating API keys.
                if *show_api_key_hint {
                    print_aligned(
                        "Auth:",
                        &format!("{} {}", style("FAILED").red().bold(), error),
                        label_padding,
                    );
                    print_aligned(
                        "Hint:",
                        &format!(
                            "Set the API key in your environment or run '{}'",
                            style("goose configure").cyan()
                        ),
                        label_padding,
                    );
                } else {
                    print_aligned(
                        "Provider:",
                        &format!("{} {}", style("FAILED").red().bold(), error),
                        label_padding,
                    );
                    print_aligned(
                        "Hint:",
                        &format!(
                            "Check the provider name and config, or run '{}'",
                            style("goose configure").cyan()
                        ),
                        label_padding,
                    );
                }
            }
            Err(ProviderCheckError::ProviderRequest(error)) => match error {
                ProviderError::Authentication(_) => {
                    print_aligned(
                        "Auth:",
                        &format!("{} {}", style("FAILED").red().bold(), error),
                        label_padding,
                    );
                    print_aligned(
                        "Hint:",
                        &format!(
                            "Check your API key or run '{}'",
                            style("goose configure").cyan()
                        ),
                        label_padding,
                    );
                }
                _ => {
                    print_aligned(
                        "Check:",
                        &format!("{} {}", style("FAILED").red().bold(), error),
                        label_padding,
                    );
                }
            },
        }

        // Propagate non-zero exit status so automation (CI scripts, install
        // checks, health probes) can rely on `goose info --check` as a
        // pre-flight verifier.
        if result.is_err() {
            return Err(anyhow!("provider check failed"));
        }
    }

    Ok(())
}
