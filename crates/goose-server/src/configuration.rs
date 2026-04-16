use crate::error::{to_env_var, ConfigError};
use config::{Config, Environment};
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Debug, Default, Deserialize)]
pub struct Settings {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_tls")]
    pub tls: bool,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
}

impl Settings {
    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("Failed to parse socket address")
    }

    pub fn new() -> Result<Self, ConfigError> {
        Self::load_and_validate()
    }

    fn load_and_validate() -> Result<Self, ConfigError> {
        // Start with default configuration
        let config = Config::builder()
            // Server defaults
            .set_default("host", default_host())?
            .set_default("port", default_port())?
            .set_default("tls", default_tls())?
            // Layer on the environment variables
            .add_source(
                Environment::with_prefix("GOOSE")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        // Try to deserialize the configuration
        let result: Result<Self, config::ConfigError> = config.try_deserialize();

        // Handle missing field errors specially
        match result {
            Ok(settings) => Ok(settings),
            Err(err) => {
                tracing::debug!("Configuration error: {:?}", &err);

                // Handle both NotFound and missing field message variants
                let error_str = err.to_string();
                if error_str.starts_with("missing field") {
                    // Extract field name from error message "missing field `type`"
                    let field = error_str
                        .trim_start_matches("missing field `")
                        .trim_end_matches("`");
                    let env_var = to_env_var(field);
                    Err(ConfigError::MissingEnvVar { env_var })
                } else if let config::ConfigError::NotFound(field) = &err {
                    let env_var = to_env_var(field);
                    Err(ConfigError::MissingEnvVar { env_var })
                } else {
                    Err(ConfigError::Other(err))
                }
            }
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_tls() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_addr_conversion() {
        let server_settings = Settings {
            host: "127.0.0.1".to_string(),
            port: 3000,
            tls: true,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let addr = server_settings.socket_addr();
        assert_eq!(addr.to_string(), "127.0.0.1:3000");
    }
}
