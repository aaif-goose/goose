//! Generate manpages for the goose CLI.
//!
//! This binary generates ROFF-formatted manpages from the clap CLI definitions.
//! Manpages are an essential part of the Linux/Unix ecosystem, providing users with
//! offline documentation accessible via the `man` command (e.g., `man goose`).
//!
//! When goose is packaged for Linux distributions (deb, rpm, etc.), the generated
//! manpages should be installed to `/usr/share/man/man1/` so users can access help
//! without an internet connection, following Unix conventions that have existed
//! since the 1970s.
//!
//! Usage:
//!   cargo run -p goose-cli --bin generate_manpages
//!   # or
//!   just generate-manpages
//!
//! Output: target/man/goose.1, target/man/goose-session.1, etc.

use clap_mangen::Man;
use std::env;
use std::fs;
use std::io::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    // Manpages are a Unix/Linux convention - skip generation on Windows
    if cfg!(target_os = "windows") {
        eprintln!("Skipping manpage generation on Windows (manpages are a Unix/Linux convention)");
        return Ok(());
    }

    let package_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let output_dir = PathBuf::from(package_dir)
        .join("..")
        .join("..")
        .join("target")
        .join("man");

    fs::create_dir_all(&output_dir)?;

    let cmd = goose_cli::branded_command();
    let top_name = cmd.get_name().to_string();

    // First pass: collect all command names for SEE ALSO sections
    let mut all_commands: Vec<String> = Vec::new();
    collect_command_names(&cmd, &mut all_commands, None);

    // Second pass: generate manpages with SEE ALSO sections
    generate_manpages(&cmd, &output_dir, None, &all_commands, &top_name)?;

    let canonical_path = output_dir.canonicalize()?;
    eprintln!(
        "Successfully generated manpages at {}",
        canonical_path.display()
    );

    Ok(())
}

fn collect_command_names(cmd: &clap::Command, names: &mut Vec<String>, parent_name: Option<&str>) {
    let name = match parent_name {
        Some(parent) => format!("{}-{}", parent, cmd.get_name()),
        None => cmd.get_name().to_string(),
    };
    names.push(name.clone());

    for subcmd in cmd.get_subcommands() {
        if subcmd.get_name() == "help" || subcmd.is_hide_set() {
            continue;
        }
        collect_command_names(subcmd, names, Some(&name));
    }
}

fn generate_manpages(
    cmd: &clap::Command,
    dir: &PathBuf,
    parent_name: Option<&str>,
    all_commands: &[String],
    top_name: &str,
) -> Result<()> {
    let name = match parent_name {
        Some(parent) => format!("{}-{}", parent, cmd.get_name()),
        None => cmd.get_name().to_string(),
    };

    // Generate the base manpage
    let man = Man::new(cmd.clone());
    let mut buffer = Vec::new();
    man.render(&mut buffer)?;

    // Add SEE ALSO section
    let see_also = generate_see_also(&name, parent_name, cmd, all_commands, top_name);
    buffer.extend_from_slice(see_also.as_bytes());

    let manpage_path = dir.join(format!("{}.1", name));
    fs::write(&manpage_path, buffer)?;
    eprintln!("  Generated: {}.1", name);

    for subcmd in cmd.get_subcommands() {
        if subcmd.get_name() == "help" || subcmd.is_hide_set() {
            continue;
        }
        generate_manpages(subcmd, dir, Some(&name), all_commands, top_name)?;
    }

    Ok(())
}

fn generate_see_also(
    current_name: &str,
    parent_name: Option<&str>,
    cmd: &clap::Command,
    all_commands: &[String],
    top_name: &str,
) -> String {
    let mut references: Vec<String> = Vec::new();

    // Always reference the main top-level command if we're not it
    if current_name != top_name {
        references.push(top_name.to_string());
    }

    // Reference parent command if exists and not already added
    if let Some(parent) = parent_name {
        if parent != top_name && !references.contains(&parent.to_string()) {
            references.push(parent.to_string());
        }
    }

    // For the main command, list immediate subcommands
    // For subcommands, list sibling commands
    if current_name == top_name {
        // Add all immediate subcommands (skip hidden ones)
        for subcmd in cmd.get_subcommands() {
            let subcmd_name = subcmd.get_name();
            if subcmd_name != "help" && !subcmd.is_hide_set() {
                let full_name = format!("{}-{}", top_name, subcmd_name);
                if !references.contains(&full_name) {
                    references.push(full_name);
                }
            }
        }
    } else if let Some(parent) = parent_name {
        // Add sibling commands (other commands with same parent)
        let prefix = format!("{}-", parent);
        for cmd_name in all_commands {
            if cmd_name.starts_with(&prefix) && cmd_name != current_name {
                // Only add immediate siblings, not nested subcommands
                let suffix = &cmd_name.strip_prefix(&prefix).unwrap_or(cmd_name);
                if !suffix.contains('-') && !references.contains(cmd_name) {
                    references.push(cmd_name.clone());
                }
            }
        }
    }

    // Sort references for consistent output
    references.sort();

    if references.is_empty() {
        return String::new();
    }

    // Format as ROFF
    let mut roff = String::from("\n.SH \"SEE ALSO\"\n");
    let formatted_refs: Vec<String> = references
        .iter()
        .map(|r| {
            let escaped = r.replace('-', "\\-");
            format!(".BR {} (1)", escaped)
        })
        .collect();
    roff.push_str(&formatted_refs.join(",\n"));
    roff.push('\n');

    roff
}
