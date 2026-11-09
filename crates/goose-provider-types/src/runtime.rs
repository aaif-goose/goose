//! Executor-independent services supplied by the embedding application.
use alloc::{boxed::Box, string::String};
use core::{future::Future, pin::Pin, time::Duration};

#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
pub use alloc::rc::Rc as Shared;
#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
pub use alloc::sync::Arc as Shared;

#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

use crate::maybe_send::{MaybeSend, MaybeSync};

pub trait HostServices: MaybeSend + MaybeSync {
    fn unix_timestamp(&self) -> i64;
    /// IDs must be unique across sessions and reboots that share persisted history.
    fn new_id(&self) -> String;
    fn sleep(&self, duration: Duration) -> BoxFuture<'_, ()>;
    /// Return a value in `0..upper_bound`, or zero if the bound is zero.
    fn jitter_ms(&self, upper_bound: u64) -> u64;
}

#[cfg(feature = "std")]
pub struct SystemHost;

#[cfg(feature = "std")]
impl HostServices for SystemHost {
    fn unix_timestamp(&self) -> i64 {
        chrono::Utc::now().timestamp()
    }
    fn new_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
    fn sleep(&self, duration: Duration) -> BoxFuture<'_, ()> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Box::pin(tokio::time::sleep(duration))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(gloo_timers::future::sleep(duration))
        }
    }
    fn jitter_ms(&self, upper_bound: u64) -> u64 {
        if upper_bound == 0 {
            0
        } else {
            rand::random::<u64>() % upper_bound
        }
    }
}

/// A short-lived guard: callers must release it before awaiting a future.
#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
pub struct Cell<T>(std::sync::Mutex<T>);
#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
impl<T> Cell<T> {
    pub fn new(value: T) -> Self {
        Self(std::sync::Mutex::new(value))
    }
    pub fn borrow_mut(&self) -> std::sync::MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
}
#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
pub struct Cell<T>(core::cell::RefCell<T>);
#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
impl<T> Cell<T> {
    pub fn new(value: T) -> Self {
        Self(core::cell::RefCell::new(value))
    }
    pub fn borrow_mut(&self) -> core::cell::RefMut<'_, T> {
        self.0.borrow_mut()
    }
}
impl<T: Default> Default for Cell<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
pub type BoxStream<'a, T> = Pin<Box<dyn futures::Stream<Item = T> + Send + 'a>>;
#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
pub type BoxStream<'a, T> = Pin<Box<dyn futures::Stream<Item = T> + 'a>>;

#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
pub type CellGuard<'a, T> = std::sync::MutexGuard<'a, T>;
#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
pub type CellGuard<'a, T> = core::cell::RefMut<'a, T>;
