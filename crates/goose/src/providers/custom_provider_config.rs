use oi_kit_providers::declarative::KeyResolver;

use crate::config::{Config, ConfigError};

pub struct ConfigKeyResolver<'a> {
    config: &'a Config,
}

impl<'a> ConfigKeyResolver<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self { config }
    }
}

impl<'a> KeyResolver for ConfigKeyResolver<'a> {
    type Error = ConfigError;

    fn resolve_key(&self, key: &str) -> std::result::Result<String, Self::Error> {
        self.config.get_secret(key)
    }
}
