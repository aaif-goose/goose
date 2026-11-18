use anyhow::Result;
use console::style;
use goose::client_extensions::{
    disable_client_extension, enable_client_extension, install_client_extension,
    list_client_extensions, uninstall_client_extension,
};
use std::path::PathBuf;

pub fn handle_client_extension_install(path: PathBuf) -> Result<()> {
    let install = install_client_extension(&path)?;

    println!(
        "{} Installed client extension '{}' ({})",
        style("✓").green(),
        style(&install.id).bold(),
        install.version
    );
    println!("  Location: {}", install.directory.display());

    Ok(())
}

pub fn handle_client_extension_list() -> Result<()> {
    let extensions = list_client_extensions()?;

    if extensions.is_empty() {
        println!("No client extensions installed.");
        return Ok(());
    }

    for extension in extensions {
        let state = if extension.enabled {
            style("enabled").green()
        } else {
            style("disabled").dim()
        };
        println!(
            "{}  {} ({}) [{}] {}",
            state,
            style(&extension.id).bold(),
            extension.version,
            extension.source,
            extension.directory.display()
        );
    }

    Ok(())
}

pub fn handle_client_extension_enable(id: &str) -> Result<()> {
    enable_client_extension(id)?;
    println!(
        "{} Enabled client extension '{}'",
        style("✓").green(),
        style(id).bold()
    );
    Ok(())
}

pub fn handle_client_extension_disable(id: &str) -> Result<()> {
    disable_client_extension(id)?;
    println!(
        "{} Disabled client extension '{}'",
        style("✓").green(),
        style(id).bold()
    );
    Ok(())
}

pub fn handle_client_extension_uninstall(id: &str) -> Result<()> {
    uninstall_client_extension(id)?;
    println!(
        "{} Uninstalled client extension '{}'",
        style("✓").green(),
        style(id).bold()
    );
    Ok(())
}
