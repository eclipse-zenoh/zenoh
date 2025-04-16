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
use tracing::debug;
use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;
use zenoh_runtime::ZRuntime;

/// A builder for close operations.
// NOTE: `Closeable` is only pub(crate) because it is zenoh-internal trait, so we don't
// care about the `private_bounds` lint in this particular case.
#[allow(private_bounds)]
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct CloseBuilder {
    closee: Box<dyn Closee>,
    timeout: Duration,
}

// NOTE: `Closeable` is only pub(crate) because it is zenoh-internal trait, so we don't
// care about the `private_bounds` lint in this particular case.
#[allow(private_bounds)]
impl CloseBuilder {
    pub(crate) fn new(closee: Box<dyn Closee>) -> Self {
        Self {
            closee,
            timeout: Duration::from_secs(10),
        }
    }

    #[cfg(all(feature = "unstable", feature = "internal"))]
    /// Set the timeout for close operation
    ///
    /// # Arguments
    ///
    /// * `timeout` - The timeout value for close operation (10s by default)
    ///
    #[doc(hidden)]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[cfg(all(feature = "unstable", feature = "internal"))]
    /// Run Close operation concurrently
    #[doc(hidden)]
    pub fn in_background(self) -> BackgroundCloseBuilder<<CloseBuilder as Resolvable>::To> {
        BackgroundCloseBuilder::new(self.into_future())
    }
}

impl std::fmt::Display for CloseBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Closing {} (timeout {:?})", self.closee, self.timeout)
    }
}

impl Resolvable for CloseBuilder {
    type To = ZResult<()>;
}

impl Wait for CloseBuilder {
    fn wait(self) -> Self::To {
        match tokio::runtime::Handle::try_current() {
            Ok(_) => {
                debug!("{self}: tokio TLS available, closing closeable directly");
                ZRuntime::Net.block_in_place(self.into_future())
            }
            Err(e) if e.is_missing_context() => {
                debug!("{self}: tokio TLS is just missing, closing closeable directly");
                ZRuntime::Net.block_in_place(self.into_future())
            }
            Err(_) => {
                #[cfg(nolocal_thread_not_available)]
                panic!("{self}: close when thread-local storage is unavailable (typically in atexit()) does not work for this Rust 1.85..1.85.1, see https://github.com/rust-lang/rust/issues/138696");

                #[cfg(not(nolocal_thread_not_available))]
                {
                    let evaluate = move || {
                        // NOTE: tracing logger also panics if used inside atexit(), so we place it here in new thread!!!
                        debug!(
                            "{self}: tokio TLS NOT available, closing closeable in separate thread"
                        );
                        ZRuntime::Net.block_in_place(self.into_future())
                    };
                    std::thread::spawn(evaluate)
                        .join()
                        .expect("Error spawning atexit-safe thread")
                }
            }
        }
    }
}

impl IntoFuture for CloseBuilder {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Pin<Box<dyn Future<Output = <Self as IntoFuture>::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(
            async move {
                debug!("{}: execute closing code...", self.closee);
                if tokio::time::timeout(self.timeout, self.closee.close_inner())
                    .await
                    .is_err()
                {
                    debug!("{}: close operation timed out", self.closee);
                    bail!("{}: close operation timed out", self.closee)
                }
                Ok(())
            }
            .into_future(),
        )
    }
}

#[cfg(all(feature = "unstable", feature = "internal"))]
/// A builder for close operations running in background
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
pub(crate) trait Closee: std::fmt::Display + Send + Sync + 'static {
    async fn close_inner(&self);
}
