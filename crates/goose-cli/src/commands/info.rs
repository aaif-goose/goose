use anyhow::Result;
use console::style;
use goose::config::paths::Paths;
use goose::config::Config;
use goose::session::session_manager::{DB_NAME, SESSIONS_FOLDER};
use serde_yaml;

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

        let provider_name: Result<String, _> = config.get_goose_provider();
        let model_name: Result<String, _> = config.get_goose_model();

        match (provider_name, model_name) {
            (Ok(provider), Ok(model)) => {
                print_aligned("Provider:", &provider, label_padding);
                print_aligned("Model:", &model, label_padding);

                let start = std::time::Instant::now();
                match goose::model::ModelConfig::new(&model) {
                    Ok(model_config) => {
                        match goose::providers::create(&provider, model_config, Vec::new()).await {
                            Ok(p) => {
                                let elapsed = start.elapsed();
                                print_aligned(
                                    "Auth:",
                                    &style("ok").green().to_string(),
                                    label_padding,
                                );
                                print_aligned(
                                    "Connection:",
                                    &format!(
                                        "{} (initialized in {:.1}s)",
                                        style("ok").green(),
                                        elapsed.as_secs_f64()
                                    ),
                                    label_padding,
                                );

                                match p.fetch_supported_models().await {
                                    Ok(models) if !models.is_empty() => {
                                        print_aligned(
                                            "Models:",
                                            &format!("{} available", models.len()),
                                            label_padding,
                                        );
                                    }
                                    Ok(_) => {
                                        print_aligned("Models:", "none listed", label_padding);
                                    }
                                    Err(e) => {
                                        print_aligned(
                                            "Models:",
                                            &format!("{} {}", style("error:").red(), e),
                                            label_padding,
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                let err_str = e.to_string();
                                print_aligned(
                                    "Auth:",
                                    &format!("{} {}", style("FAILED").red().bold(), err_str),
                                    label_padding,
                                );
                                if err_str.contains("not found") || err_str.contains("API_KEY") {
                                    print_aligned(
                                        "Hint:",
                                        &format!(
                                            "Set the API key in your environment or run '{}'",
                                            style("goose configure").cyan()
                                        ),
                                        label_padding,
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        print_aligned(
                            "Model:",
                            &format!("{} {}", style("invalid:").red(), e),
                            label_padding,
                        );
                    }
                }
            }
            _ => {
                println!(
                    "  {} No provider configured. Run '{}' first.",
                    style("⚠").yellow(),
                    style("goose configure").cyan()
                );
            }
        }
    }

    Ok(())
}
