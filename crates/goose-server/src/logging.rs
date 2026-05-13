use anyhow::Result;
use tracing_subscriber::util::SubscriberInitExt;

/// Sets up the logging infrastructure for the server.
/// Logs go to a JSON file and a pretty console layer on stderr.
pub fn setup_logging(name: Option<&str>) -> Result<()> {
    let config = goose::logging::LoggingConfig {
        component: "server",
        name,
        extra_directives: &["goose_server=info", "tower_http=info"],
        console: true,
        json: false,
    };
    let subscriber = goose::logging::build_logging_subscriber(&config)?;
    subscriber.try_init()?;
    Ok(())
}
