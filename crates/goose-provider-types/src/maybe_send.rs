//! `Send` and `Sync` bounds for threaded desktop and local execution.
//!
//! Alloc-only builds and the `local` feature use a single executor thread. A
//! wasm32 host also runs on one thread, and its capabilities (fetch, timers,
//! storage) are JavaScript promises whose futures are not `Send`. The GDK traits
//! use these bounds so an embedder there can implement them without asserting
//! `Send` for values that are not. Threaded desktop builds keep the real bounds.

#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
pub trait MaybeSend: Send {}
#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
impl<T: Send + ?Sized> MaybeSend for T {}

#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
pub trait MaybeSend {}
#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
impl<T: ?Sized> MaybeSend for T {}

#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
pub trait MaybeSync: Sync {}
#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
impl<T: Sync + ?Sized> MaybeSync for T {}

#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
pub trait MaybeSync {}
#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
impl<T: ?Sized> MaybeSync for T {}
