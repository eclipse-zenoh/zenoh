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
    time::{Duration, Instant},
};

use async_trait::async_trait;
use tracing::warn;
use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;
use zenoh_runtime::ZRuntime;
use zenoh_task::TaskController;

/// A behavior selector for close operations used if initial timeout was reached
pub enum CloseBackoff {
    /// fail immediately
    Fail,
    /// wait for an additional timeout, then fail if timeout reached
    Wait(Duration),
}

/// A builder for close operations.
// NOTE: `Closeable` is only pub(crate) because it is zenoh-internal trait, so we don't
// care about the `private_bounds` lint in this particular case.
#[allow(private_bounds)]
pub struct CloseBuilder<TCloseable: Closeable> {
    closee: TCloseable::TClosee,
    timeout: Duration,
    backoff: CloseBackoff,
}

// NOTE: `Closeable` is only pub(crate) because it is zenoh-internal trait, so we don't
// care about the `private_bounds` lint in this particular case.
#[allow(private_bounds)]
impl<TCloseable: Closeable> CloseBuilder<TCloseable> {
    pub(crate) fn new(closeable: &TCloseable) -> Self {
        Self {
            closee: closeable.get_closee(),
            timeout: Duration::MAX,
            backoff: CloseBackoff::Fail,
        }
    }

    /// Set the timeout for close operation
    ///
    /// # Arguments
    ///
    /// * `timeout` - The timeout value for close operation
    ///
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the backoff for close operation
    ///
    /// # Arguments
    ///
    /// * `backoff` - The backoff behavior for close operation
    ///
    pub fn backoff(mut self, backoff: CloseBackoff) -> Self {
        self.backoff = backoff;
        self
    }
}

impl<TCloseable: Closeable> Resolvable for CloseBuilder<TCloseable> {
    type To = ZResult<()>;
}

impl<TCloseable: Closeable> Wait for CloseBuilder<TCloseable> {
    fn wait(self) -> Self::To {
        ZRuntime::Application.block_on(self.into_future())
    }
}

impl<TCloseable: Closeable> IntoFuture for CloseBuilder<TCloseable> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Pin<Box<dyn Future<Output = <Self as IntoFuture>::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(
            async move {
                let mut state = CloseState::new(Instant::now() + self.timeout, self.backoff);
                state.close(&self.closee).await
            }
            .into_future(),
        )
    }
}

pub(crate) struct CloseState {
    time_limit: Instant,
    backoff: CloseBackoff,
}

impl CloseState {
    pub(crate) fn new(time_limit: Instant, backoff: CloseBackoff) -> Self {
        Self {
            time_limit,
            backoff,
        }
    }

    pub(crate) async fn close(&mut self, closee: &impl Closee) -> ZResult<()> {
        closee.close_inner(self).await
    }

    pub(crate) async fn close_future<T: Future<Output = ()>>(&mut self, future: T) -> ZResult<()> {
        tokio::time::timeout(self.timeout()?, future)
            .await
            .map_err(|_e| "execution timed out".into())
    }

    fn timeout(&mut self) -> ZResult<Duration> {
        let now = Instant::now();
        if now > self.time_limit {
            self.do_backoff(now)?;
            debug_assert!(now >= self.time_limit);
        }
        Ok(now - self.time_limit)
    }

    fn do_backoff(&mut self, now: Instant) -> ZResult<()> {
        match self.backoff {
            CloseBackoff::Fail => bail!("Backoff failure reached"),
            CloseBackoff::Wait(duration) => {
                self.time_limit = now + duration;
                self.backoff = CloseBackoff::Fail;
                warn!(
                    "Backoff reached, switching to {:?} duration. Next timeout will be {:?}",
                    duration, self.time_limit
                );
                Ok(())
            }
        }
    }
}

#[async_trait]
pub(crate) trait Closee: Send + Sync + 'static {
    async fn close_inner(&self, state: &mut CloseState) -> ZResult<()>;
}

#[async_trait]
impl Closee for TaskController {
    async fn close_inner(&self, state: &mut CloseState) -> ZResult<()> {
        while self.terminate_all_async(state.timeout()?).await != 0 {}
        Ok(())
    }
}

pub(crate) trait Closeable {
    type TClosee: Closee;
    fn get_closee(&self) -> Self::TClosee;
}
