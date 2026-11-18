use anyhow::{bail, Result};
use console::style;
use std::io::{stdin, stdout, Write};

pub fn handle_plugin_list() -> Result<()> {
    let plugins = goose::plugins::list_installed_plugins();
    if plugins.is_empty() {
        println!("No plugins installed.");
        return Ok(());
    }
    for plugin in &plugins {
        println!(
            "{} {} ({})",
            style(&plugin.name).bold(),
            style(format!("v{}", plugin.version)).dim(),
            style(&plugin.format).cyan()
        );
        if plugin.mcp_tools {
            println!("  {} MCP tools enabled", style("✓").green());
        }
        if !plugin.provides.is_empty() {
            println!("  Provides: {}", plugin.provides.join(", "));
        }
        if !plugin.config_keys.is_empty() {
            println!("  Config: {}", plugin.config_keys.join(", "));
        }
        if !plugin.permissions.is_empty() {
            println!("  Permissions: {}", plugin.permissions.join(", "));
        }
        if !plugin.skills.is_empty() {
            println!("  Skills: {}", plugin.skills.join(", "));
        }
    }
    Ok(())
}

pub fn handle_plugin_uninstall(name: &str) -> Result<()> {
    goose::plugins::uninstall_plugin(name)?;
    println!("{} Uninstalled plugin '{name}'", style("✓").green());
    Ok(())
}

pub fn handle_plugin_config(
    name: &str,
    key: Option<&str>,
    value: Option<&str>,
    secret: bool,
) -> Result<()> {
    let config = goose::config::Config::global();
    match (key, value) {
        (Some(key), Some(value)) => {
            let config_key = format!("plugins.{name}.config.{key}");
            if secret {
                config.set_secret(&config_key, &value.to_string())?;
            } else {
                config.set_param(&config_key, value.to_string())?;
            }
            println!("{} Set {name}.{key}", style("✓").green());
        }
        (Some(key), None) => {
            let config_key = format!("plugins.{name}.config.{key}");
            let val = config
                .get_secret::<String>(&config_key)
                .ok()
                .map(|_| "****".to_string())
                .or_else(|| config.get_param::<String>(&config_key).ok());
            match val {
                Some(v) => println!("{key} = {v}"),
                None => println!("{key} = (not set)"),
            }
        }
        (None, _) => {
            let native = goose::plugins::plugin_install_dir().join(name);
            match goose::plugins::formats::goose_native::read_manifest(&native) {
                Ok(Some(manifest)) => {
                    if manifest.config.is_empty() {
                        println!("Plugin '{name}' has no config keys.");
                    } else {
                        println!("Config for '{name}':");
                        let mut keys: Vec<_> = manifest.config.keys().collect();
                        keys.sort();
                        for k in keys {
                            let entry = &manifest.config[k];
                            let config_key = format!("plugins.{name}.config.{k}");
                            let current = config
                                .get_param::<String>(&config_key)
                                .ok()
                                .unwrap_or_else(|| {
                                    entry
                                        .default
                                        .as_ref()
                                        .map(|v| v.to_string())
                                        .unwrap_or_else(|| "(not set)".to_string())
                                });
                            let type_hint = if entry.kind == "enum" {
                                if let Some(vals) = &entry.enum_values {
                                    format!("enum({})", vals.join("|"))
                                } else {
                                    "enum".to_string()
                                }
                            } else {
                                entry.kind.clone()
                            };
                            let required_mark = if entry.required.unwrap_or(false) {
                                "*"
                            } else {
                                ""
                            };
                            println!(
                                "  {k}{required_mark} = {current}  [{type_hint}] {}",
                                entry.description
                            );
                        }
                    }
                }
                Ok(None) => bail!("Plugin '{name}' is not a native plugin or is not installed"),
                Err(e) => bail!("Failed to read plugin manifest: {e}"),
            }
        }
    }
    Ok(())
}

pub fn handle_plugin_enable(name: &str) -> Result<()> {
    goose::plugins::set_plugin_enabled(name, true)?;
    println!("{} Enabled plugin '{name}'", style("✓").green());
    Ok(())
}

pub fn handle_plugin_disable(name: &str) -> Result<()> {
    goose::plugins::set_plugin_enabled(name, false)?;
    println!("{} Disabled plugin '{name}'", style("✓").green());
    Ok(())
}

pub fn handle_plugin_install(url: &str, auto_update: bool) -> Result<()> {
    let install = goose::plugins::install_plugin_with_options(
        url,
        goose::plugins::PluginInstallOptions { auto_update },
    )?;

    println!(
        "{} Installed {} plugin '{}' ({})",
        style("✓").green(),
        install.format,
        style(&install.name).bold(),
        install.version
    );
    print_plugin_install(&install);

    if let Ok(Some(manifest)) =
        goose::plugins::formats::goose_native::read_manifest(&install.directory)
    {
        if !manifest.permissions.is_empty() {
            println!("\nThis plugin requests the following permissions:");
            for perm in &manifest.permissions {
                println!("  {}", style(perm).yellow());
            }
            print!("Approve all permissions? [y/N] ");
            stdout().flush()?;
            let mut input = String::new();
            stdin().read_line(&mut input)?;
            if input.trim().eq_ignore_ascii_case("y") {
                let config = goose::config::Config::global();
                let key = format!("plugins.{}.permissions.approved", manifest.name);
                config.set_param(&key, manifest.permissions.clone())?;
                println!("{} Permissions approved", style("✓").green());
            } else {
                println!(
                    "{} Permissions deferred — run `goose plugin enable {}` to approve later",
                    style("!").yellow(),
                    install.name
                );
            }
        }
    }

    Ok(())
}

pub fn handle_plugin_update(name: &str) -> Result<()> {
    let install = goose::plugins::update_plugin(name)?;

    println!(
        "{} Updated {} plugin '{}' ({})",
        style("✓").green(),
        install.format,
        style(&install.name).bold(),
        install.version
    );
    print_plugin_install(&install);

    Ok(())
}

fn print_plugin_install(install: &goose::plugins::PluginInstall) {
    println!("  Source: {}", install.source);
    println!("  Location: {}", install.directory.display());

    if install.skills.is_empty() {
        println!("  No skills imported.");
    } else {
        println!("  Imported skills:");
        for skill in &install.skills {
            println!("    - {}", skill.name);
        }
    }
}
