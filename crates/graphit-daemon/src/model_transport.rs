//! Forward an SDK transport while signalling disconnection to its native worker.

use futures::{Sink, Stream};
use graphit_core::file_lock::Cancellation;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

/// One connection's transport and the cancellation signal shared with its worker.
///
/// This adapter owns no codec, buffer or task. The service retains native workers
/// until they finish; dropping an async handler is not a native worker join.
pub struct DisconnectTransport<T> {
    transport: T,
    cancellation: Cancellation,
}

impl<T> DisconnectTransport<T> {
    pub fn new(transport: T, cancellation: Cancellation) -> Self {
        Self {
            transport,
            cancellation,
        }
    }
}

impl<T, M, E> Stream for DisconnectTransport<T>
where
    T: Stream<Item = Result<M, E>> + Unpin,
{
    type Item = Result<M, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.transport).poll_next(cx);
        if matches!(&result, Poll::Ready(None | Some(Err(_)))) {
            this.cancellation.cancel();
        }
        result
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.transport.size_hint()
    }
}

impl<T, I> Sink<I> for DisconnectTransport<T>
where
    T: Sink<I> + Unpin,
{
    type Error = T::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.transport).poll_ready(cx);
        if matches!(&result, Poll::Ready(Err(_))) {
            this.cancellation.cancel();
        }
        result
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.transport).start_send(item);
        if result.is_err() {
            this.cancellation.cancel();
        }
        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.transport).poll_flush(cx);
        if matches!(&result, Poll::Ready(Err(_))) {
            this.cancellation.cancel();
        }
        result
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.transport).poll_close(cx);
        if result.is_ready() {
            this.cancellation.cancel();
        }
        result
    }
}

impl<T> Drop for DisconnectTransport<T> {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}
