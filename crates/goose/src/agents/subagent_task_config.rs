use crate::providers::base::Provider;
use goose_agent::extension::ExtensionConfig;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for task execution with all necessary dependencies
#[derive(Clone)]
pub struct TaskConfig {
    pub provider: Arc<dyn Provider>,
    pub model_config: goose_providers::model::ModelConfig,
    pub parent_session_id: String,
    pub parent_working_dir: PathBuf,
    pub extensions: Vec<ExtensionConfig>,
    pub max_turns: Option<usize>,
}

impl fmt::Debug for TaskConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskConfig")
            .field("provider", &"<dyn Provider>")
            .field("parent_session_id", &self.parent_session_id)
            .field("parent_working_dir", &self.parent_working_dir)
            .field("max_turns", &self.max_turns)
            .field("extensions", &self.extensions)
            .finish()
    }
}
