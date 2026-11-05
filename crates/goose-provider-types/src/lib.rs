#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
#[allow(unused_imports)]
pub(crate) mod prelude {
    pub use alloc::borrow::ToOwned;
    pub use alloc::{
        boxed::Box,
        format,
        string::{String, ToString},
        vec,
        vec::Vec,
    };
}
pub mod base;
pub mod cache_semantics;
pub mod canonical;
pub mod context_limit;
pub mod conversation;
pub mod documents;
pub mod errors;
#[cfg(feature = "formats")]
pub mod formats;
pub mod goose_mode;
pub mod images;
pub mod json;
pub mod maybe_send;
pub(crate) mod mcp_utils;
pub mod model;
mod model_names;
pub mod permission;
#[cfg(feature = "std")]
pub mod request_log;
pub mod retry;
pub mod thinking;
pub mod utils;

pub mod data;
pub mod runtime;
pub mod stream;

#[macro_export]
macro_rules! object {
    ({$($tokens:tt)*}) => { $crate::data::object(serde_json::json!({$($tokens)*})) };
}

#[cfg(feature = "mcp-interop")]
pub mod mcp_interop;

#[doc(hidden)]
pub mod trace;
