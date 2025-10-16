//
// Copyright (c) 2024 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

use std::{
    future::{Future, IntoFuture},
    pin::Pin,
    time::Duration,
};

use async_trait::async_trait;
#[cfg(all(feature = "unstable", feature = "internal"))]
use tokio::sync::oneshot::Receiver;
use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;
use zenoh_runtime::ZRuntime;

/// A builder for close operations, returned by the
/// [`Session::close`](crate::Session::close) method.
// NOTE: `Closeable` is only pub(crate) because it is a zenoh-internal trait, so we don't
// care about the `private_bounds` lint in this particular case.
#[allow(private_bounds)]
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct CloseBuilder<TCloseable: Closeable> {
    closee: TCloseable::TClosee,
    timeout: Duration,
}

// NOTE: `Closeable` is only pub(crate) because it is a zenoh-internal trait, so we don't
// care about the `private_bounds` lint in this particular case.
#[allow(private_bounds)]
impl<TCloseable: Closeable> CloseBuilder<TCloseable> {
    pub(crate) fn new(closeable: &'_ TCloseable) -> Self {
        Self {
            closee: closeable.get_closee(),
            timeout: Duration::from_secs(10),
        }
    }

    #[cfg(all(feature = "unstable", feature = "internal"))]
    /// Set the timeout for the close operation
    ///
    /// # Arguments
    ///
    /// * `timeout` - The timeout value for the close operation (10s by default)
    ///
    #[doc(hidden)]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[cfg(all(feature = "unstable", feature = "internal"))]
    /// Run the Close operation concurrently
    #[doc(hidden)]
    pub fn in_background(
        self,
    ) -> BackgroundCloseBuilder<<CloseBuilder<TCloseable> as Resolvable>::To> {
        BackgroundCloseBuilder::new(self.into_future())
    }
}

impl<TCloseable: Closeable> Resolvable for CloseBuilder<TCloseable> {
    type To = ZResult<()>;
}

impl<TCloseable: Closeable> Wait for CloseBuilder<TCloseable> {
    fn wait(self) -> Self::To {
        let future = self.into_future();
        match tokio::runtime::Handle::try_current() {
            Ok(_) => {
                tracing::trace!("tokio TLS available, closing closeable directly");
                ZRuntime::Net.block_in_place(future)
            }
            Err(e) if e.is_missing_context() => {
                tracing::trace!("tokio TLS is just missing, closing closeable directly");
                ZRuntime::Net.block_in_place(future)
            }
            Err(_) => {
                #[cfg(nolocal_thread_not_available)]
                panic!("Close when thread-local storage is unavailable (typically in atexit()) does not work for this Rust 1.85..1.85.1, see https://github.com/rust-lang/rust/issues/138696");

                #[cfg(not(nolocal_thread_not_available))]
                {
                    let evaluate = move || {
                        // NOTE: tracing logger also panics if used inside atexit() handler!!!
                        tracing::trace!(
                            "tokio TLS NOT available, closing closeable in separate thread"
                        );
                        ZRuntime::Net.block_in_place(future)
                    };
                    std::thread::spawn(evaluate)
                        .join()
                        .expect("Error spawning atexit-safe thread")
                }
            }
        }
    }
}

impl<TCloseable: Closeable> IntoFuture for CloseBuilder<TCloseable> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Pin<Box<dyn Future<Output = <Self as IntoFuture>::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(
            async move {
                if tokio::time::timeout(self.timeout, self.closee.close_inner())
                    .await
                    .is_err()
                {
                    bail!("close operation timed out!")
                }
                Ok(())
            }
            .into_future(),
        )
    }
}

#[cfg(all(feature = "unstable", feature = "internal"))]
/// A builder for close operations running in the background
#[doc(hidden)]
pub struct BackgroundCloseBuilder<TOutput: Send + 'static> {
    inner: Pin<Box<dyn Future<Output = TOutput> + Send>>,
}

#[cfg(all(feature = "unstable", feature = "internal"))]
#[doc(hidden)]
impl<TOutput: Send + 'static> BackgroundCloseBuilder<TOutput> {
    fn new(inner: Pin<Box<dyn Future<Output = TOutput> + Send>>) -> Self {
        Self { inner }
    }
}

#[cfg(all(feature = "unstable", feature = "internal"))]
impl<TOutput: Send + 'static> Resolvable for BackgroundCloseBuilder<TOutput> {
    type To = NolocalJoinHandle<TOutput>;
}

#[cfg(all(feature = "unstable", feature = "internal"))]
impl<TOutput: Send + 'static> Wait for BackgroundCloseBuilder<TOutput> {
    fn wait(self) -> Self::To {
        ZRuntime::Net.block_in_place(self.into_future())
    }
}

#[cfg(all(feature = "unstable", feature = "internal"))]
impl<TOutput: Send + 'static> IntoFuture for BackgroundCloseBuilder<TOutput> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Pin<Box<dyn Future<Output = <Self as IntoFuture>::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(
            async move {
                let (tx, rx) = tokio::sync::oneshot::channel();
                ZRuntime::Net.spawn(async move {
                    if tx.send(self.inner.await).is_err() {
                        panic!("BackgroundCloseBuilder: critical error sending the result");
                    }
                });
                NolocalJoinHandle::new(rx)
            }
            .into_future(),
        )
    }
}

#[cfg(all(feature = "unstable", feature = "internal"))]
#[doc(hidden)]
pub struct NolocalJoinHandle<TOutput: Send + 'static> {
    rx: Receiver<TOutput>,
}

#[cfg(all(feature = "unstable", feature = "internal"))]
impl<TOutput: Send + 'static> NolocalJoinHandle<TOutput> {
    fn new(rx: Receiver<TOutput>) -> Self {
        Self { rx }
    }
}

#[cfg(all(feature = "unstable", feature = "internal"))]
impl<TOutput: Send + 'static> Resolvable for NolocalJoinHandle<TOutput> {
    type To = TOutput;
}

#[cfg(all(feature = "unstable", feature = "internal"))]
impl<TOutput: Send + 'static> Wait for NolocalJoinHandle<TOutput> {
    fn wait(self) -> Self::To {
        ZRuntime::Net.block_in_place(self.into_future())
    }
}

#[cfg(all(feature = "unstable", feature = "internal"))]
impl<TOutput: Send + 'static> IntoFuture for NolocalJoinHandle<TOutput> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Pin<Box<dyn Future<Output = <Self as IntoFuture>::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(
            async move {
                self.rx
                    .await
                    .expect("NolocalJoinHandle: critical error receiving the result")
            }
            .into_future(),
        )
    }
}

#[async_trait]
pub(crate) trait Closee: Send + Sync + 'static {
    async fn close_inner(&self);
}

pub(crate) trait Closeable {
    type TClosee: Closee;
    fn get_closee(&self) -> Self::TClosee;
}
