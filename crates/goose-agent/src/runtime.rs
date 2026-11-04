//! Runtime adapters for the portable agent loop.
#[cfg(not(feature = "std"))]
use crate::prelude::*;
pub use goose_provider_types::runtime::{Cell, HostServices, Shared};

#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
pub use local::{CancellationToken, Cancelled};
#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
pub use tokio_util::sync::CancellationToken;

#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
mod local {
    use super::*;
    use core::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };
    #[derive(Clone, Default)]
    pub struct CancellationToken {
        state: Shared<Cell<State>>,
    }
    #[derive(Default)]
    struct State {
        cancelled: bool,
        parent: Option<CancellationToken>,
        waiters: Vec<Option<Waker>>,
    }
    impl CancellationToken {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn child_token(&self) -> Self {
            Self {
                state: Shared::new(Cell::new(State {
                    parent: Some(self.clone()),
                    ..State::default()
                })),
            }
        }
        pub fn is_cancelled(&self) -> bool {
            let state = self.state.borrow_mut();
            state.cancelled || state.parent.as_ref().is_some_and(Self::is_cancelled)
        }
        pub fn cancel(&self) {
            let waiters = {
                let mut state = self.state.borrow_mut();
                state.cancelled = true;
                state
                    .waiters
                    .iter_mut()
                    .filter_map(Option::take)
                    .collect::<Vec<_>>()
            };
            for waker in waiters {
                waker.wake();
            }
        }
        pub fn cancelled(&self) -> Cancelled {
            Cancelled {
                token: self.clone(),
                registrations: Vec::new(),
            }
        }
    }
    pub struct Cancelled {
        token: CancellationToken,
        registrations: Vec<(CancellationToken, usize)>,
    }
    impl Future for Cancelled {
        type Output = ();
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            let this = self.get_mut();
            if this.token.is_cancelled() {
                return Poll::Ready(());
            }
            if this.registrations.is_empty() {
                let mut token = Some(this.token.clone());
                while let Some(current) = token {
                    let mut state = current.state.borrow_mut();
                    let index = state
                        .waiters
                        .iter()
                        .position(Option::is_none)
                        .unwrap_or(state.waiters.len());
                    if index == state.waiters.len() {
                        state.waiters.push(None);
                    }
                    state.waiters[index] = Some(cx.waker().clone());
                    token = state.parent.clone();
                    drop(state);
                    this.registrations.push((current, index));
                }
            } else {
                for (token, index) in &this.registrations {
                    token.state.borrow_mut().waiters[*index] = Some(cx.waker().clone());
                }
            }
            Poll::Pending
        }
    }
    impl Drop for Cancelled {
        fn drop(&mut self) {
            for (token, index) in &self.registrations {
                token.state.borrow_mut().waiters[*index] = None;
            }
        }
    }
}
