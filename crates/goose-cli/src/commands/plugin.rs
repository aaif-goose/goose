use anyhow::Result;
use console::style;

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
