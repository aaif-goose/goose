//! `Send` and `Sync` bounds that apply natively but not on wasm32.
//!
//! A wasm32 host runs on one thread, and its capabilities (fetch, timers,
//! storage) are JavaScript promises whose futures are not `Send`. The GDK traits
//! use these bounds so an embedder there can implement them without asserting
//! `Send` for values that are not. Native builds keep the real bounds.

#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> MaybeSend for T {}

#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSend for T {}

#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSync: Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Sync + ?Sized> MaybeSync for T {}

#[cfg(target_arch = "wasm32")]
pub trait MaybeSync {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSync for T {}
