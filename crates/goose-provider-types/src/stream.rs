//! Backpressured streams without thread-local storage or an executor dependency.
use crate::runtime::{Cell, Shared};
use alloc::boxed::Box;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use futures::Stream;

pub struct Yield<T> {
    slot: Shared<Cell<Option<T>>>,
}
impl<T> Yield<T> {
    pub async fn send(&mut self, item: T) {
        *self.slot.borrow_mut() = Some(item);
        let mut yielded = false;
        core::future::poll_fn(|_| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                Poll::Pending
            }
        })
        .await;
    }
}

pub struct TryStream<T, E, F> {
    slot: Shared<Cell<Option<T>>>,
    producer: Pin<Box<F>>,
    done: bool,
    error: core::marker::PhantomData<fn() -> E>,
}
impl<T, E, F> Unpin for TryStream<T, E, F> {}
impl<T, E, F: Future<Output = Result<(), E>>> Stream for TryStream<T, E, F> {
    type Item = Result<T, E>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        match this.producer.as_mut().poll(cx) {
            Poll::Ready(result) => {
                this.done = true;
                match result {
                    Ok(()) => Poll::Ready(None),
                    Err(e) => Poll::Ready(Some(Err(e))),
                }
            }
            Poll::Pending => match this.slot.borrow_mut().take() {
                Some(item) => Poll::Ready(Some(Ok(item))),
                None => Poll::Pending,
            },
        }
    }
}
pub fn try_stream<T, E, F: Future<Output = Result<(), E>>>(
    make: impl FnOnce(Yield<T>) -> F,
) -> TryStream<T, E, F> {
    let slot = Shared::new(Cell::new(None));
    let producer = Box::pin(make(Yield { slot: slot.clone() }));
    TryStream {
        slot,
        producer,
        done: false,
        error: core::marker::PhantomData,
    }
}
