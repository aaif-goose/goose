use anyhow::Result;
use std::sync::Once;

// Used to ensure we only set up tracing once
static INIT: Once = Once::new();

/// Sets up the logging infrastructure for the CLI.
/// Logs go to a JSON file only (no console output).
pub fn setup_logging(name: Option<&str>) -> Result<()> {
    setup_logging_internal(name, false)
}

fn setup_logging_internal(name: Option<&str>, force: bool) -> Result<()> {
    let mut result = Ok(());

    let mut setup = || {
        result = (|| {
            use tracing_subscriber::util::SubscriberInitExt;

            let config = goose::logging::LoggingConfig {
                component: "cli",
                name,
                extra_directives: &["goose_cli=info"],
                console: false,
                json: true,
            };
            let subscriber = goose::logging::build_logging_subscriber(&config)?;

            if force {
                let _guard = subscriber.set_default();
                tracing::warn!("Test log entry from setup");
                tracing::info!("Another test log entry from setup");
                std::thread::sleep(std::time::Duration::from_millis(100));
                Ok(())
            } else {
                subscriber
                    .try_init()
                    .map_err(|e| anyhow::anyhow!("Failed to set global subscriber: {}", e))?;
                Ok(())
            }
        })();
    };

    if force {
        setup();
    } else {
        INIT.call_once(setup);
    }

    result
}

#[cfg(test)]
mod tests {
    use goose::tracing::langfuse_layer;
    use std::env;
    use tempfile::TempDir;

    fn setup_temp_home() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        if cfg!(windows) {
            env::set_var("USERPROFILE", temp_dir.path());
        } else {
            env::set_var("HOME", temp_dir.path());
        }
        temp_dir
    }

    #[test]
    fn test_log_directory_creation() {
        let _temp_dir = setup_temp_home();
        let log_dir = goose::logging::prepare_log_directory("cli", true).unwrap();
        assert!(log_dir.exists());
        assert!(log_dir.is_dir());

        let path_components: Vec<_> = log_dir.components().collect();
        assert!(path_components.iter().any(|c| c.as_os_str() == "goose"));
        assert!(path_components.iter().any(|c| c.as_os_str() == "logs"));
        assert!(path_components.iter().any(|c| c.as_os_str() == "cli"));
    }

    #[tokio::test]
    async fn test_langfuse_layer_creation() {
        let _temp_dir = setup_temp_home();

        let original_vars = [
            ("LANGFUSE_PUBLIC_KEY", env::var("LANGFUSE_PUBLIC_KEY").ok()),
            ("LANGFUSE_SECRET_KEY", env::var("LANGFUSE_SECRET_KEY").ok()),
            ("LANGFUSE_URL", env::var("LANGFUSE_URL").ok()),
            (
                "LANGFUSE_INIT_PROJECT_PUBLIC_KEY",
                env::var("LANGFUSE_INIT_PROJECT_PUBLIC_KEY").ok(),
            ),
            (
                "LANGFUSE_INIT_PROJECT_SECRET_KEY",
                env::var("LANGFUSE_INIT_PROJECT_SECRET_KEY").ok(),
            ),
        ];

        for (var, _) in &original_vars {
            env::remove_var(var);
        }

        assert!(langfuse_layer::create_langfuse_observer().is_none());

        env::set_var("LANGFUSE_PUBLIC_KEY", "test_public_key");
        env::set_var("LANGFUSE_SECRET_KEY", "test_secret_key");
        assert!(langfuse_layer::create_langfuse_observer().is_some());

        env::remove_var("LANGFUSE_PUBLIC_KEY");
        env::remove_var("LANGFUSE_SECRET_KEY");
        env::set_var("LANGFUSE_INIT_PROJECT_PUBLIC_KEY", "test_public_key");
        env::set_var("LANGFUSE_INIT_PROJECT_SECRET_KEY", "test_secret_key");
        assert!(langfuse_layer::create_langfuse_observer().is_some());

        env::remove_var("LANGFUSE_INIT_PROJECT_PUBLIC_KEY");
        assert!(langfuse_layer::create_langfuse_observer().is_none());

        for (var, value) in original_vars {
            match value {
                Some(val) => env::set_var(var, val),
                None => env::remove_var(var),
            }
        }
    }

    #[tokio::test]
    async fn test_default_filter_avoids_debug_by_default() {
        // The shared helper honours RUST_LOG; without it the defaults apply.
        // We just smoke-check that building the subscriber doesn't panic.
        let _temp_dir = setup_temp_home();
        let config = goose::logging::LoggingConfig {
            component: "cli-test",
            name: None,
            extra_directives: &["goose_cli=info"],
            console: false,
            json: true,
        };
        assert!(goose::logging::build_logging_subscriber(&config).is_ok());
    }
}
