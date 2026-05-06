mod chat_history_search;
mod diagnostics;
pub mod extension_data;
mod legacy;
pub mod session_manager;
pub mod thread_manager;

pub use diagnostics::{
    config_path, generate_diagnostics, get_system_info, latest_llm_log_path,
    latest_server_log_path, read_capped, read_tail, SystemInfo,
};
pub use extension_data::{EnabledExtensionsState, ExtensionData, ExtensionState, TodoState};
pub use session_manager::{
    Session, SessionInsights, SessionManager, SessionNameUpdate, SessionType, SessionUpdateBuilder,
};
pub use thread_manager::{Thread, ThreadManager, ThreadMetadata};
