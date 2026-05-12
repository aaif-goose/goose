mod commands;
mod configuration;
mod error;
mod logging;
mod openapi;
mod routes;
mod session_event_bus;
mod state;
mod tunnel;

use std::path::PathBuf;
use std::{backtrace::Backtrace, panic::PanicHookInfo};

use clap::{Parser, Subcommand};
use goose::agents::validate_extensions;
use goose_mcp::{
    mcp_server_runner::{serve, McpCommand},
    AutoVisualiserRouter, ComputerControllerServer, MemoryServer, TutorialServer,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the agent server
    Agent,
    /// Run the MCP server
    Mcp {
        #[arg(value_parser = clap::value_parser!(McpCommand))]
        server: McpCommand,
    },
    /// Validate a bundled-extensions JSON file
    #[command(name = "validate-extensions")]
    ValidateExtensions {
        /// Path to the bundled-extensions JSON file
        path: PathBuf,
    },
}

fn boot_marker(message: &str) {
    eprintln!("GOOSED_BOOT: {message}");
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info: &PanicHookInfo<'_>| {
        let location = panic_info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown".to_string());

        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|msg| (*msg).to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic payload".to_string());

        eprintln!("GOOSED_BOOT: panic at {location}: {payload}");
        eprintln!("GOOSED_BOOT: backtrace:\n{}", Backtrace::force_capture());

        default_hook(panic_info);
    }));
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_panic_hook();
    boot_marker("main entered");

    let cli = Cli::parse();
    boot_marker(&format!(
        "command parsed: {:?}",
        std::mem::discriminant(&cli.command)
    ));

    match cli.command {
        Commands::Agent => {
            commands::agent::run().await?;
        }
        Commands::Mcp { server } => {
            logging::setup_logging(Some(&format!("mcp-{}", server.name())))?;
            match server {
                McpCommand::AutoVisualiser => serve(AutoVisualiserRouter::new()).await?,
                McpCommand::ComputerController => serve(ComputerControllerServer::new()).await?,
                McpCommand::Memory => serve(MemoryServer::new()).await?,
                McpCommand::Tutorial => serve(TutorialServer::new()).await?,
            }
        }
        Commands::ValidateExtensions { path } => {
            match validate_extensions::validate_bundled_extensions(&path) {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
