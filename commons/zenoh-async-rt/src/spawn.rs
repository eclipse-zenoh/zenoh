use futures::FutureExt;
use std::fmt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pub fn spawn<F, T>(future: F) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    JoinHandle(async_std::task::spawn(future))
}

pub fn spawn_blocking<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    JoinHandle(async_std::task::spawn_blocking(f))
}

pub struct JoinHandle<T>(async_std::task::JoinHandle<T>);

impl<T> JoinHandle<T> {
    pub async fn cancel(self) -> Option<T> {
        self.0.cancel().await
    }
}

impl<T> fmt::Debug for JoinHandle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("JoinHandle").finish()
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_unpin(cx)
    }
}
