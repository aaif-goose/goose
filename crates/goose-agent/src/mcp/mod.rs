//! MCP connections and pinned catalogs without application configuration or storage.

mod catalog;
mod manager;
mod operation;
mod runtime;
mod transport;

pub use catalog::{provider_safe_tool_name, Catalog, CatalogEntry, CollisionPolicy};
pub use manager::{Client, Connector, Lease, LoadResult, Manager, Registration};
pub use operation::{McpHost, McpOperation};
pub use runtime::{Runtime, ScopedRuntime, Scopes};
pub use transport::{ClientOptions, ElicitationHandler, TransportClient};

/// Runtime identity copied from advertised tools to generated requests.
pub const BINDING_META_KEY: &str = "gdk.mcp_binding";
