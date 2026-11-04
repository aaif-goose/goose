//! MCP connections and pinned catalogs without application configuration or storage.

mod catalog;
mod manager;
mod operation;
mod runtime;
mod transport;

pub use catalog::{provider_safe_tool_name, Catalog, CatalogEntry, CollisionPolicy};
pub use manager::{Connector, Lease, LoadResult, Registration};
pub use operation::{McpHost, McpOperation};
pub use runtime::{Manager, Runtime, Scopes, Selection};
pub use transport::{ClientOptions, ElicitationHandler, RequestSender, TransportClient};

/// Runtime identity copied from advertised tools to generated requests.
pub const BINDING_META_KEY: &str = "gdk.mcp_binding";

mod scoped_client;
pub use scoped_client::{CallContext, Client};

mod request;
pub use request::await_response;

pub mod schema;

pub mod discovery;
