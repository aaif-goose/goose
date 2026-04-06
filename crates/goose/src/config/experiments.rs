use super::base::Config;
use anyhow::Result;
use std::collections::HashMap;

/// It is the ground truth for init experiments. The experiment names in users' experiment list but not
/// in the list will be remove from user list; The experiment names in the ground-truth list but not
/// in users' experiment list will be added to user list with default value false;
const ALL_EXPERIMENTS: &[(&str, bool)] = &[];

/// Experiment configuration management
pub struct ExperimentManager;

impl ExperimentManager {
    /// Get all experiments and their configurations
    ///
    /// - Ensures the user's experiment list is synchronized with `ALL_EXPERIMENTS`.
    /// - Adds missing experiments from `ALL_EXPERIMENTS` with the default value.
    /// - Removes experiments not in `ALL_EXPERIMENTS`.
    pub fn get_all() -> Result<Vec<(String, bool)>> {
        let config = Config::global();
        let mut experiments: HashMap<String, bool> =
            config.get_param("experiments").unwrap_or_default();
        Self::refresh_experiments(&mut experiments);

        Ok(experiments.into_iter().collect())
    }

    /// Enable or disable an experiment
    pub fn set_enabled(name: &str, enabled: bool) -> Result<()> {
        let config = Config::global();
        let mut experiments: HashMap<String, bool> = config
            .get_param("experiments")
            .unwrap_or_else(|_| HashMap::new());
        Self::refresh_experiments(&mut experiments);
        experiments.insert(name.to_string(), enabled);

        config.set_param("experiments", experiments)?;
        Ok(())
    }

    /// Check if an experiment is enabled
    pub fn is_enabled(name: &str) -> Result<bool> {
        let experiments = Self::get_all()?;
        let experiments_map: HashMap<String, bool> = experiments.into_iter().collect();
        Ok(*experiments_map.get(name).unwrap_or(&false))
    }

    fn refresh_experiments(experiments: &mut HashMap<String, bool>) {
        // Add missing experiments from `ALL_EXPERIMENTS`
        for &(key, default_value) in ALL_EXPERIMENTS {
            experiments.entry(key.to_string()).or_insert(default_value);
        }

        // Remove experiments not present in `ALL_EXPERIMENTS`
        experiments.retain(|key, _| ALL_EXPERIMENTS.iter().any(|(k, _)| k == key));
    }
}
