use anyhow::Result;
use console::style;

pub fn handle_plugin_install(url: &str) -> Result<()> {
    let install = goose::plugins::install_plugin(url)?;

    println!(
        "{} Installed {} plugin '{}' ({})",
        style("✓").green(),
        install.format,
        style(&install.name).bold(),
        install.version
    );
    println!("  Source: {}", install.source);
    println!("  Location: {}", install.directory.display());

    if install.skills.is_empty() {
        println!("  No skills imported.");
    } else {
        println!("  Imported skills:");
        for skill in install.skills {
            println!("    - {}", skill.name);
        }
    }

    Ok(())
}
