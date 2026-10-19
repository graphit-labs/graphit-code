//! Local privacy scope for the ACP SDK futures used by native consumers.
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

// The SDK emits raw protocol tracing before callbacks, including during drop.
// Keep this scope local to the operation; never replace the global subscriber.
pub(crate) struct QuietSdk<F> {
    future: Option<Pin<Box<F>>>,
    dispatcher: tracing::Dispatch,
}
impl<F> QuietSdk<F> {
    pub(crate) fn new(future: F) -> Self {
        Self {
            future: Some(Box::pin(future)),
            dispatcher: tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default()),
        }
    }
}
impl<F: Future> Future for QuietSdk<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let future = this
            .future
            .as_mut()
            .expect("SDK future retained until drop");
        tracing::dispatcher::with_default(&this.dispatcher, || future.as_mut().poll(cx))
    }
}
impl<F> Drop for QuietSdk<F> {
    fn drop(&mut self) {
        let future = self.future.take();
        tracing::dispatcher::with_default(&self.dispatcher, || drop(future));
    }
}
