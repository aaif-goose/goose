#[cfg(not(any(feature = "rustls-tls", feature = "native-tls")))]
compile_error!("At least one of `rustls-tls` or `native-tls` features must be enabled");

#[cfg(all(feature = "rustls-tls", feature = "native-tls"))]
compile_error!("Features `rustls-tls` and `native-tls` are mutually exclusive");

pub mod auth;
pub mod configuration;
pub mod error;
pub mod openapi;
pub mod routes;
pub mod session_event_bus;
pub mod state;
#[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
pub mod tls;
pub mod tunnel;

// Re-export commonly used items
pub use openapi::*;
pub use state::*;
