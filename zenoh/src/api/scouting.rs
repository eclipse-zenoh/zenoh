//
// Copyright (c) 2023 ZettaScale Technology
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
// filetag{rust.scouting}

use std::{
    fmt,
    future::{IntoFuture, Ready},
    net::SocketAddr,
    ops::Deref,
    time::Duration,
};

use tokio::net::UdpSocket;
use zenoh_core::{Resolvable, Wait};
use zenoh_protocol::{core::WhatAmIMatcher, scouting::Hello};
use zenoh_result::ZResult;
use zenoh_task::TerminatableTask;

use crate::{
    api::handlers::{locked, Callback, DefaultHandler, IntoHandler},
    net::runtime::{orchestrator::Loop, Runtime},
};

/// A builder for initializing a [`Scout`].
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
///
/// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
///     .await
///     .unwrap();
/// while let Ok(hello) = receiver.recv_async().await {
///     println!("{}", hello);
/// }
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct ScoutBuilder<Handler> {
    pub(crate) what: WhatAmIMatcher,
    pub(crate) config: ZResult<crate::config::Config>,
    pub(crate) handler: Handler,
}

impl ScoutBuilder<DefaultHandler> {
    /// Receive the [`Hello`] messages from this scout with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
    ///     .callback(|hello| { println!("{}", hello); })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> ScoutBuilder<Callback>
    where
        Callback: Fn(Hello) + Send + Sync + 'static,
    {
        let ScoutBuilder {
            what,
            config,
            handler: _,
        } = self;
        ScoutBuilder {
            what,
            config,
            handler: callback,
        }
    }

    /// Receive the [`Hello`] messages from this scout with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](ScoutBuilder::callback) method, we suggest you use it instead of `callback_mut`.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let mut n = 0;
    /// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
    ///     .callback_mut(move |_hello| { n += 1; })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> ScoutBuilder<impl Fn(Hello) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Hello) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the [`Hello`] messages from this scout with a [`Handler`](crate::prelude::IntoHandler).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .unwrap();
    /// while let Ok(hello) = receiver.recv_async().await {
    ///     println!("{}", hello);
    /// }
    /// # }
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> ScoutBuilder<Handler>
    where
        Handler: IntoHandler<'static, Hello>,
    {
        let ScoutBuilder {
            what,
            config,
            handler: _,
        } = self;
        ScoutBuilder {
            what,
            config,
            handler,
        }
    }
}

impl<Handler> Resolvable for ScoutBuilder<Handler>
where
    Handler: IntoHandler<'static, Hello> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Scout<Handler::Handler>>;
}

impl<Handler> Wait for ScoutBuilder<Handler>
where
    Handler: IntoHandler<'static, Hello> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_handler();
        _scout(self.what, self.config?, callback).map(|scout| Scout { scout, receiver })
    }
}

impl<Handler> IntoFuture for ScoutBuilder<Handler>
where
    Handler: IntoHandler<'static, Hello> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A scout that returns [`Hello`] messages through a callback.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
///
/// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
///     .callback(|hello| { println!("{}", hello); })
///     .await
///     .unwrap();
/// # }
/// ```
pub(crate) struct ScoutInner {
    #[allow(dead_code)]
    pub(crate) scout_task: Option<TerminatableTask>,
}

impl ScoutInner {
    /// Stop scouting.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
    ///     .callback(|hello| { println!("{}", hello); })
    ///     .await
    ///     .unwrap();
    /// scout.stop();
    /// # }
    /// ```
    pub(crate) fn stop(self) {
        std::mem::drop(self);
    }
}

impl Drop for ScoutInner {
    fn drop(&mut self) {
        if self.scout_task.is_some() {
            let task = self.scout_task.take();
            task.unwrap().terminate(Duration::from_secs(10));
        }
    }
}

impl fmt::Debug for ScoutInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CallbackScout").finish()
    }
}

/// A scout that returns [`Hello`] messages through a [`Handler`](crate::prelude::IntoHandler).
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
///
/// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
///     .with(flume::bounded(32))
///     .await
///     .unwrap();
/// while let Ok(hello) = receiver.recv_async().await {
///     println!("{}", hello);
/// }
/// # }
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub struct Scout<Receiver> {
    pub(crate) scout: ScoutInner,
    pub(crate) receiver: Receiver,
}

impl<Receiver> Deref for Scout<Receiver> {
    type Target = Receiver;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl<Receiver> Scout<Receiver> {
    /// Stop scouting.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let scout = zenoh::scout(WhatAmI::Router, config::default())
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .unwrap();
    /// let _router = scout.recv_async().await;
    /// scout.stop();
    /// # }
    /// ```
    pub fn stop(self) {
        self.scout.stop()
    }
}

fn _scout(
    what: WhatAmIMatcher,
    config: zenoh_config::Config,
    callback: Callback<'static, Hello>,
) -> ZResult<ScoutInner> {
    tracing::trace!("scout({}, {})", what, &config);
    let default_addr = SocketAddr::from(zenoh_config::defaults::scouting::multicast::address);
    let addr = config.scouting.multicast.address().unwrap_or(default_addr);
    let ifaces = config.scouting.multicast.interface().as_ref().map_or(
        zenoh_config::defaults::scouting::multicast::interface,
        |s| s.as_ref(),
    );
    let ifaces = Runtime::get_interfaces(ifaces);
    if !ifaces.is_empty() {
        let sockets: Vec<UdpSocket> = ifaces
            .into_iter()
            .filter_map(|iface| Runtime::bind_ucast_port(iface).ok())
            .collect();
        if !sockets.is_empty() {
            let cancellation_token = TerminatableTask::create_cancellation_token();
            let cancellation_token_clone = cancellation_token.clone();
            let task = TerminatableTask::spawn(
                zenoh_runtime::ZRuntime::Acceptor,
                async move {
                    let scout = Runtime::scout(&sockets, what, &addr, move |hello| {
                        let callback = callback.clone();
                        async move {
                            callback(hello);
                            Loop::Continue
                        }
                    });
                    tokio::select! {
                        _ = scout => {},
                        _ = cancellation_token_clone.cancelled() => { tracing::trace!("stop scout({}, {})", what, &config); },
                    }
                },
                cancellation_token.clone(),
            );
            return Ok(ScoutInner {
                scout_task: Some(task),
            });
        }
    }
    Ok(ScoutInner { scout_task: None })
}

/// Scout for routers and/or peers.
///
/// [`scout`] spawns a task that periodically sends scout messages and waits for [`Hello`](crate::scouting::Hello) replies.
///
/// Drop the returned [`Scout`](crate::scouting::Scout) to stop the scouting task.
///
/// # Arguments
///
/// * `what` - The kind of zenoh process to scout for
/// * `config` - The configuration [`Config`] to use for scouting
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
/// use zenoh::scouting::WhatAmI;
///
/// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
///     .await
///     .unwrap();
/// while let Ok(hello) = receiver.recv_async().await {
///     println!("{}", hello);
/// }
/// # }
/// ```
pub fn scout<I: Into<WhatAmIMatcher>, TryIntoConfig>(
    what: I,
    config: TryIntoConfig,
) -> ScoutBuilder<DefaultHandler>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error:
        Into<zenoh_result::Error>,
{
    ScoutBuilder {
        what: what.into(),
        config: config.try_into().map_err(|e| e.into()),
        handler: DefaultHandler::default(),
    }
}
