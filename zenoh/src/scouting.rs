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
use crate::handlers::{locked, Callback, DefaultHandler};
use crate::net::runtime::{orchestrator::Loop, Runtime};

use async_std::net::UdpSocket;
use futures::StreamExt;
use std::future::Ready;
use std::{fmt, ops::Deref};
use zenoh_config::{
    whatami::WhatAmIMatcher, ZN_MULTICAST_INTERFACE_DEFAULT, ZN_MULTICAST_IPV4_ADDRESS_DEFAULT,
};
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};
use zenoh_result::ZResult;

/// Constants and helpers for zenoh `whatami` flags.
pub use zenoh_protocol::core::WhatAmI;

/// A zenoh Hello message.
pub use zenoh_protocol::scouting::Hello;

/// A builder for initializing a [`Scout`].
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
/// use zenoh::scouting::WhatAmI;
///
/// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
///     .res()
///     .await
///     .unwrap();
/// while let Ok(hello) = receiver.recv_async().await {
///     println!("{}", hello);
/// }
/// # })
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
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh::scouting::WhatAmI;
    ///
    /// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
    ///     .callback(|hello| { println!("{}", hello); })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
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
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh::scouting::WhatAmI;
    ///
    /// let mut n = 0;
    /// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
    ///     .callback_mut(move |_hello| { n += 1; })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
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

    /// Receive the [`Hello`] messages from this scout with a [`Handler`](crate::prelude::IntoCallbackReceiverPair).
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh::scouting::WhatAmI;
    ///
    /// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
    ///     .with(flume::bounded(32))
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(hello) = receiver.recv_async().await {
    ///     println!("{}", hello);
    /// }
    /// # })
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> ScoutBuilder<Handler>
    where
        Handler: crate::prelude::IntoCallbackReceiverPair<'static, Hello>,
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
    Handler: crate::prelude::IntoCallbackReceiverPair<'static, Hello> + Send,
    Handler::Receiver: Send,
{
    type To = ZResult<Scout<Handler::Receiver>>;
}

impl<Handler> SyncResolve for ScoutBuilder<Handler>
where
    Handler: crate::prelude::IntoCallbackReceiverPair<'static, Hello> + Send,
    Handler::Receiver: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_cb_receiver_pair();
        scout(self.what, self.config?, callback).map(|scout| Scout { scout, receiver })
    }
}

impl<Handler> AsyncResolve for ScoutBuilder<Handler>
where
    Handler: crate::prelude::IntoCallbackReceiverPair<'static, Hello> + Send,
    Handler::Receiver: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

/// A scout that returns [`Hello`] messages through a callback.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
/// use zenoh::scouting::WhatAmI;
///
/// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
///     .callback(|hello| { println!("{}", hello); })
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
pub(crate) struct ScoutInner {
    #[allow(dead_code)]
    pub(crate) stop_sender: flume::Sender<()>,
}

impl ScoutInner {
    /// Stop scouting.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh::scouting::WhatAmI;
    ///
    /// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
    ///     .callback(|hello| { println!("{}", hello); })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// scout.stop();
    /// # })
    /// ```
    pub fn stop(self) {
        // drop
    }
}

impl fmt::Debug for ScoutInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CallbackScout").finish()
    }
}

/// A scout that returns [`Hello`] messages through a [`Handler`](crate::prelude::IntoCallbackReceiverPair).
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
/// use zenoh::scouting::WhatAmI;
///
/// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default())
///     .with(flume::bounded(32))
///     .res()
///     .await
///     .unwrap();
/// while let Ok(hello) = receiver.recv_async().await {
///     println!("{}", hello);
/// }
/// # })
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub struct Scout<Receiver> {
    pub(crate) scout: ScoutInner,
    pub receiver: Receiver,
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
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh::scouting::WhatAmI;
    ///
    /// let scout = zenoh::scout(WhatAmI::Router, config::default())
    ///     .with(flume::bounded(32))
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// let _router = scout.recv_async().await;
    /// scout.stop();
    /// # })
    /// ```
    pub fn stop(self) {
        self.scout.stop()
    }
}

fn scout(
    what: WhatAmIMatcher,
    config: zenoh_config::Config,
    callback: Callback<'static, Hello>,
) -> ZResult<ScoutInner> {
    log::trace!("scout({}, {})", what, &config);
    let default_addr = match ZN_MULTICAST_IPV4_ADDRESS_DEFAULT.parse() {
        Ok(addr) => addr,
        Err(e) => {
            bail!(
                "invalid default addr {}: {:?}",
                ZN_MULTICAST_IPV4_ADDRESS_DEFAULT,
                &e
            )
        }
    };
    let addr = config.scouting.multicast.address().unwrap_or(default_addr);
    let ifaces = config
        .scouting
        .multicast
        .interface()
        .as_ref()
        .map_or(ZN_MULTICAST_INTERFACE_DEFAULT, |s| s.as_ref());
    let (stop_sender, stop_receiver) = flume::bounded::<()>(1);
    let ifaces = Runtime::get_interfaces(ifaces);
    if !ifaces.is_empty() {
        let sockets: Vec<UdpSocket> = ifaces
            .into_iter()
            .filter_map(|iface| Runtime::bind_ucast_port(iface).ok())
            .collect();
        if !sockets.is_empty() {
            async_std::task::spawn(async move {
                let mut stop_receiver = stop_receiver.stream();
                let scout = Runtime::scout(&sockets, what, &addr, move |hello| {
                    let callback = callback.clone();
                    async move {
                        callback(hello);
                        Loop::Continue
                    }
                });
                let stop = async move {
                    stop_receiver.next().await;
                    log::trace!("stop scout({}, {})", what, &config);
                };
                async_std::prelude::FutureExt::race(scout, stop).await;
            });
        }
    }
    Ok(ScoutInner { stop_sender })
}
