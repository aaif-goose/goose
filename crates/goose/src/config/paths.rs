use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};
use std::path::PathBuf;

pub struct Paths;

impl Paths {
    fn get_dir(dir_type: DirType) -> PathBuf {
        if let Ok(test_root) = std::env::var("GOOSE_PATH_ROOT") {
            let base = PathBuf::from(test_root);
            match dir_type {
                DirType::Config => base.join("config"),
                DirType::Data => base.join("data"),
                DirType::State => base.join("state"),
                DirType::Plugins => base.join(".agents").join("plugins"),
            }
        } else {
            // NOTE: "Block" is kept here for backwards compatibility with existing
            // user config/data directories (e.g. ~/Library/Application Support/Block/goose/).
            // Changing this would orphan existing installations.
            let strategy = choose_app_strategy(AppStrategyArgs {
                top_level_domain: "Block".to_string(),
                author: "Block".to_string(),
                app_name: "goose".to_string(),
            })
            .expect("goose requires a home dir");

            match dir_type {
                DirType::Config => strategy.config_dir(),
                DirType::Data => strategy.data_dir(),
                DirType::State => strategy.state_dir().unwrap_or(strategy.data_dir()),
                DirType::Plugins => strategy.home_dir().join(".agents").join("plugins"),
            }
        }
    }

    pub fn config_dir() -> PathBuf {
        Self::get_dir(DirType::Config)
    }

    pub fn data_dir() -> PathBuf {
        Self::get_dir(DirType::Data)
    }

    pub fn state_dir() -> PathBuf {
        Self::get_dir(DirType::State)
    }

    pub fn plugins_dir() -> PathBuf {
        Self::get_dir(DirType::Plugins)
    }

    pub fn in_state_dir(subpath: &str) -> PathBuf {
        Self::state_dir().join(subpath)
    }

    pub fn in_config_dir(subpath: &str) -> PathBuf {
        Self::config_dir().join(subpath)
    }

    pub fn in_data_dir(subpath: &str) -> PathBuf {
        Self::data_dir().join(subpath)
    }
}

enum DirType {
    Config,
    Data,
    State,
    Plugins,
}
