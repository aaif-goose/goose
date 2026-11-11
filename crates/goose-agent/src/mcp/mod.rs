//! MCP connections and pinned catalogs without application configuration or storage.

mod catalog;
mod manager;
mod operation;
mod runtime;
mod transport;

pub use catalog::{provider_safe_tool_name, Catalog, CatalogEntry, CollisionPolicy};
pub use manager::{
    Connector, Lease, LoadResult, LoadStage, Manager, Registration, Server, Started, StartupPolicy,
};
pub use operation::{McpHost, McpOperation};
/// Advanced ownership primitives for hosts with custom selection and persistence policy.
pub mod host {
    pub use super::runtime::{Manager, Runtime, Scopes, Selection};
}
pub use transport::{ClientOptions, ElicitationHandler, RequestSender, TransportClient};

/// Runtime identity copied from advertised tools to generated requests.
pub const BINDING_META_KEY: &str = "gdk.mcp_binding";

mod scoped_client;
pub use scoped_client::{CallContext, Client};

mod request;
pub use request::await_response;

pub mod schema;

pub mod discovery;

#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "mcp-stdio", feature = "mcp-http")
))]
mod connectors;
#[cfg(all(not(target_arch = "wasm32"), feature = "mcp-http"))]
pub use connectors::HttpConnector;
#[cfg(all(not(target_arch = "wasm32"), feature = "mcp-stdio"))]
pub use connectors::{StderrPolicy, StdioConnector};
