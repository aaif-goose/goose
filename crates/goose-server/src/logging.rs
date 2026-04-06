use anyhow::Result;
use tracing_appender::rolling::Rotation;
use tracing_subscriber::{
    filter::LevelFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
    Registry,
};

#[cfg(feature = "otel")]
use goose::otel::otlp;
use goose::tracing::langfuse_layer;

/// Sets up the logging infrastructure for the application.
/// This includes:
/// - File-based logging with JSON formatting (DEBUG level)
/// - Console output for development (INFO level)
/// - Optional Langfuse integration (DEBUG level)
pub fn setup_logging(name: Option<&str>) -> Result<()> {
    let log_dir = goose::logging::prepare_log_directory("server", true)?;
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let log_filename = if let Some(n) = name {
        format!("{}-{}.log", timestamp, n)
    } else {
        format!("{}.log", timestamp)
    };
    let file_appender =
        tracing_appender::rolling::RollingFileAppender::new(Rotation::NEVER, log_dir, log_filename);

    // Create JSON file logging layer
    let file_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_writer(file_appender)
        .with_ansi(false)
        .with_file(true);

    let base_env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("")
            .add_directive("mcp_client=info".parse().unwrap())
            .add_directive("goose=info".parse().unwrap())
            .add_directive("goose_server=info".parse().unwrap())
            .add_directive("tower_http=info".parse().unwrap())
            .add_directive(LevelFilter::WARN.into())
    });

    let console_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_level(true)
        .with_file(true)
        .with_ansi(false)
        .with_line_number(true)
        .pretty();

    let mut layers = vec![
        file_layer.with_filter(base_env_filter.clone()).boxed(),
        console_layer.with_filter(base_env_filter).boxed(),
    ];

    #[cfg(feature = "otel")]
    layers.extend(otlp::init_otlp_layers(goose::config::Config::global()));

    if let Some(langfuse) = langfuse_layer::create_langfuse_observer() {
        layers.push(langfuse.with_filter(LevelFilter::DEBUG).boxed());
    }

    let subscriber = Registry::default().with(layers);

    subscriber.try_init()?;

    Ok(())
}
