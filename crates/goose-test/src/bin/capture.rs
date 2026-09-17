use clap::{Parser, Subcommand, ValueEnum};
use goose_test_support::mcp::McpFixtureServer;
use rmcp::{transport::stdio, ServiceExt};

use goose_test::mcp::stdio::playback::playback;
use goose_test::mcp::stdio::record::record;

#[derive(Parser)]
struct Cli {
    #[arg(value_enum)]
    transport: Transport,
    #[command(subcommand)]
    mode: Mode,
}

#[derive(ValueEnum, Clone, Debug)]
enum Transport {
    Stdio,
}

#[derive(Subcommand, Clone, Debug)]
enum Mode {
    Record {
        file: String,
        command: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Playback {
        file: String,
    },
    Fixture,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.mode {
        Mode::Record {
            file,
            command,
            args,
        } => record(&file, &command, &args)?,
        Mode::Playback { file } => playback(&file)?,
        Mode::Fixture => {
            let service = McpFixtureServer::new().serve(stdio()).await?;
            service.waiting().await?;
        }
    }

    Ok(())
}
