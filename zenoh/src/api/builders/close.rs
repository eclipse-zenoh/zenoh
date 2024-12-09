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
use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;
use zenoh_runtime::ZRuntime;

/// A builder for close operations.
// NOTE: `Closeable` is only pub(crate) because it is zenoh-internal trait, so we don't
// care about the `private_bounds` lint in this particular case.
#[allow(private_bounds)]
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct CloseBuilder<TCloseable: Closeable> {
    closee: TCloseable::TClosee,
    timeout: Duration,
}

// NOTE: `Closeable` is only pub(crate) because it is zenoh-internal trait, so we don't
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
        ZRuntime::Application.block_in_place(self.into_future())
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
/// A builder for close operations running in background
// NOTE: `Closeable` is only pub(crate) because it is zenoh-internal trait, so we don't
// care about the `private_bounds` lint in this particular case.
#[doc(hidden)]
#[allow(private_bounds)]
pub struct BackgroundCloseBuilder<TOutput: Send + 'static> {
    inner: Pin<Box<dyn Future<Output = TOutput> + Send>>,
}

#[cfg(all(feature = "unstable", feature = "internal"))]
#[doc(hidden)]
// NOTE: `Closeable` is only pub(crate) because it is zenoh-internal trait, so we don't
// care about the `private_bounds` lint in this particular case.
#[allow(private_bounds)]
impl<TOutput: Send + 'static> BackgroundCloseBuilder<TOutput> {
    fn new(inner: Pin<Box<dyn Future<Output = TOutput> + Send>>) -> Self {
        Self { inner }
    }
}

#[cfg(all(feature = "unstable", feature = "internal"))]
impl<TOutput: Send + 'static> Resolvable for BackgroundCloseBuilder<TOutput> {
    type To = tokio::task::JoinHandle<TOutput>;
}

#[cfg(all(feature = "unstable", feature = "internal"))]
impl<TOutput: Send + 'static> Wait for BackgroundCloseBuilder<TOutput> {
    fn wait(self) -> Self::To {
        ZRuntime::Application.block_in_place(self.into_future())
    }
}

#[cfg(all(feature = "unstable", feature = "internal"))]
impl<TOutput: Send + 'static> IntoFuture for BackgroundCloseBuilder<TOutput> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Pin<Box<dyn Future<Output = <Self as IntoFuture>::Output> + Send>>;

    // NOTE: yes, we need to return a future that returns JoinHandle
    #[allow(clippy::async_yields_async)]
    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move { ZRuntime::Application.spawn(self.inner) }.into_future())
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
