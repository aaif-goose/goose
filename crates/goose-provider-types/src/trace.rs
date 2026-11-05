//! Desktop telemetry is deliberately absent from alloc-only builds.
#[cfg(not(feature = "std"))]
pub use no_std::*;
#[cfg(feature = "std")]
pub use tracing::*;
#[cfg(not(feature = "std"))]
mod no_std {
    #[derive(Clone)]
    pub struct Span;
    impl Span {
        pub fn current() -> Self {
            Self
        }
        pub fn record<T>(&self, _name: &str, _value: T) {}
    }
    pub mod field {
        pub struct Empty;
    }
    pub trait Instrument: Sized {
        fn instrument(self, _span: Span) -> Self {
            self
        }
    }
    impl<T: core::future::Future> Instrument for T {}
}
#[cfg(not(feature = "std"))]
#[macro_export]
#[doc(hidden)]
macro_rules! __gdk_log {
    () => {{}};
    (target: $target:expr, $($rest:tt)*) => {{ let _ = || { let _ = &$target; }; $crate::__gdk_log!($($rest)*); }};
    ($($key:ident).+ = %$value:expr, $($rest:tt)*) => {{ let _ = || { let _ = &$value; }; $crate::__gdk_log!($($rest)*); }};
    ($($key:ident).+ = ?$value:expr, $($rest:tt)*) => {{ let _ = || { let _ = &$value; }; $crate::__gdk_log!($($rest)*); }};
    ($($key:ident).+ = $value:expr, $($rest:tt)*) => {{ let _ = || { let _ = &$value; }; $crate::__gdk_log!($($rest)*); }};
    ($key:literal = %$value:expr, $($rest:tt)*) => {{ let _ = || { let _ = &$value; }; $crate::__gdk_log!($($rest)*); }};
    ($key:literal = ?$value:expr, $($rest:tt)*) => {{ let _ = || { let _ = &$value; }; $crate::__gdk_log!($($rest)*); }};
    ($key:literal = $value:expr, $($rest:tt)*) => {{ let _ = || { let _ = &$value; }; $crate::__gdk_log!($($rest)*); }};
    (%$value:ident, $($rest:tt)*) => {{ let _ = &$value; $crate::__gdk_log!($($rest)*); }};
    (?$value:ident, $($rest:tt)*) => {{ let _ = &$value; $crate::__gdk_log!($($rest)*); }};
    ($value:ident, $($rest:tt)*) => {{ let _ = &$value; $crate::__gdk_log!($($rest)*); }};
    ($format:literal $(, $arg:expr)* $(,)?) => {{ let _ = || { let _ = core::format_args!($format $(, $arg)*); }; }};
}
#[cfg(not(feature = "std"))]
#[macro_export]
#[doc(hidden)]
macro_rules! __gdk_span {
    (target: $target:expr, $name:literal, $($rest:tt)*) => {{ $crate::__gdk_log!($($rest)*); $crate::trace::Span }};
    ($name:literal, $($rest:tt)*) => {{ $crate::__gdk_log!($($rest)*); $crate::trace::Span }};
}
#[cfg(not(feature = "std"))]
pub use crate::__gdk_log as debug;
#[cfg(not(feature = "std"))]
pub use crate::__gdk_log as info;
#[cfg(not(feature = "std"))]
pub use crate::__gdk_log as warn;
#[cfg(not(feature = "std"))]
pub use crate::__gdk_log as error;
#[cfg(not(feature = "std"))]
pub use crate::__gdk_span as info_span;
