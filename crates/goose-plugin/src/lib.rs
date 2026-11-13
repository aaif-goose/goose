pub mod host;
pub mod manifest;
pub mod runtime;

pub use host::HostClient;
pub use manifest::{McpSurface, PluginManifest, WebUiSurface, MANIFEST_FILE};
pub use runtime::PluginRuntime;

pub use rmcp::{
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, ContentBlock, ErrorCode, ErrorData, Implementation, InitializeResult,
        ServerCapabilities, ServerConfig,
    },
    schemars,
    schemars::JsonSchema,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
