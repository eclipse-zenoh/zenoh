//
// Copyright (c) 2022 ZettaScale Technology
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
use crate::{
    net::runtime::{orchestrator::Loop, Runtime},
    prelude::{locked, Callback, IntoHandler},
};
use async_std::net::UdpSocket;
use futures::StreamExt;
use std::{fmt, marker::PhantomData, ops::Deref, sync::Arc};
use zenoh_config::{
    whatami::WhatAmIMatcher, ZN_MULTICAST_INTERFACE_DEFAULT, ZN_MULTICAST_IPV4_ADDRESS_DEFAULT,
};
use zenoh_core::{AsyncResolve, Resolvable, Result as ZResult, SyncResolve};

/// Constants and helpers for zenoh `whatami` flags.
pub use zenoh_protocol_core::WhatAmI;

/// A zenoh Hello message.
pub use zenoh_protocol::proto::Hello;

#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug, Clone)]
pub struct ScoutBuilder<IntoWhatAmI, TryIntoConfig>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
{
    pub(crate) what: IntoWhatAmI,
    pub(crate) config: TryIntoConfig,
}

impl<IntoWhatAmI, TryIntoConfig> Resolvable for ScoutBuilder<IntoWhatAmI, TryIntoConfig>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
{
    type Output = ZResult<HandlerScout<flume::Receiver<Hello>>>;
}

impl<IntoWhatAmI, TryIntoConfig> ScoutBuilder<IntoWhatAmI, TryIntoConfig>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
{
    /// Receive the [`Hello`] messages from this scout with a callback.
    #[inline]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> CallbackScoutBuilder<IntoWhatAmI, TryIntoConfig, Callback>
    where
        Callback: Fn(Hello) + Send + Sync + 'static,
    {
        CallbackScoutBuilder {
            builder: self,
            callback,
        }
    }

    /// Receive the [`Hello`] messages from this scout with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](ScoutBuilder::callback) method, we suggest you use it instead of `callback_mut`
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> CallbackScoutBuilder<IntoWhatAmI, TryIntoConfig, impl Fn(Hello) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Hello) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the [`Hello`] messages from this scout with a [`Handler`](crate::prelude::Handler).
    #[inline]
    pub fn with<IntoHandler, Receiver>(
        self,
        handler: IntoHandler,
    ) -> HandlerScoutBuilder<IntoWhatAmI, TryIntoConfig, IntoHandler, Receiver>
    where
        IntoHandler: crate::prelude::IntoHandler<Hello, Receiver>,
    {
        HandlerScoutBuilder {
            builder: self,
            handler,
            receiver: PhantomData,
        }
    }
}

impl<IntoWhatAmI, TryIntoConfig> AsyncResolve for ScoutBuilder<IntoWhatAmI, TryIntoConfig>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: 'static + Send + std::convert::TryInto<crate::config::Config>,
{
    type Future = futures::future::Ready<Self::Output>;

    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl<IntoWhatAmI, TryIntoConfig> SyncResolve for ScoutBuilder<IntoWhatAmI, TryIntoConfig>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
{
    fn res_sync(self) -> Self::Output {
        let (callback, receiver) = flume::bounded(1).into_handler();
        scout(self.what, self.config, callback).map(|scout| HandlerScout { scout, receiver })
    }
}

#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug, Clone)]
pub struct CallbackScoutBuilder<IntoWhatAmI, TryIntoConfig, Callback>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    Callback: Fn(Hello) + Send + Sync + 'static,
{
    builder: ScoutBuilder<IntoWhatAmI, TryIntoConfig>,
    callback: Callback,
}
impl<IntoWhatAmI, TryIntoConfig, Callback> Resolvable
    for CallbackScoutBuilder<IntoWhatAmI, TryIntoConfig, Callback>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    Callback: Fn(Hello) + Send + Sync + 'static,
{
    type Output = ZResult<Scout>;
}

impl<IntoWhatAmI, TryIntoConfig, Callback> AsyncResolve
    for CallbackScoutBuilder<IntoWhatAmI, TryIntoConfig, Callback>
where
    Callback: 'static + Fn(Hello) + Send + Sync,
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: 'static + Send + std::convert::TryInto<crate::config::Config>,
{
    type Future = futures::future::Ready<Self::Output>;

    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl<IntoWhatAmI, TryIntoConfig, Callback> SyncResolve
    for CallbackScoutBuilder<IntoWhatAmI, TryIntoConfig, Callback>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    Callback: Fn(Hello) + Send + Sync + 'static,
{
    fn res_sync(self) -> Self::Output {
        scout(
            self.builder.what,
            self.builder.config,
            Box::new(self.callback),
        )
    }
}

pub struct Scout {
    #[allow(dead_code)]
    pub(crate) stop_sender: flume::Sender<()>,
}

impl Scout {
    pub fn stop(self) {
        // drop
    }
}

impl fmt::Debug for Scout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Scout").finish()
    }
}

#[derive(Debug, Clone)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct HandlerScoutBuilder<IntoWhatAmI, TryIntoConfig, IntoHandler, Receiver>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    IntoHandler: crate::prelude::IntoHandler<Hello, Receiver>,
{
    builder: ScoutBuilder<IntoWhatAmI, TryIntoConfig>,
    handler: IntoHandler,
    receiver: PhantomData<Receiver>,
}

impl<IntoWhatAmI, TryIntoConfig, IntoHandler, Receiver> Resolvable
    for HandlerScoutBuilder<IntoWhatAmI, TryIntoConfig, IntoHandler, Receiver>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    IntoHandler: crate::prelude::IntoHandler<Hello, Receiver>,
{
    type Output = ZResult<HandlerScout<Receiver>>;
}

impl<IntoWhatAmI, TryIntoConfig, IntoHandler, Receiver> AsyncResolve
    for HandlerScoutBuilder<IntoWhatAmI, TryIntoConfig, IntoHandler, Receiver>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: 'static + Send + std::convert::TryInto<crate::config::Config>,
    IntoHandler: crate::prelude::IntoHandler<Hello, Receiver>,
    Receiver: Send,
{
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl<IntoWhatAmI, TryIntoConfig, IntoHandler, Receiver> SyncResolve
    for HandlerScoutBuilder<IntoWhatAmI, TryIntoConfig, IntoHandler, Receiver>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    IntoHandler: crate::prelude::IntoHandler<Hello, Receiver>,
    Receiver: Send,
{
    fn res_sync(self) -> Self::Output {
        let (callback, receiver) = self.handler.into_handler();
        scout(self.builder.what, self.builder.config, callback)
            .map(|scout| HandlerScout { scout, receiver })
    }
}

#[derive(Debug)]
pub struct HandlerScout<Receiver> {
    pub scout: Scout,
    pub receiver: Receiver,
}

impl<Receiver> Deref for HandlerScout<Receiver> {
    type Target = Receiver;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl<Receiver> HandlerScout<Receiver> {
    pub fn stop(self) {
        self.scout.stop()
    }
}

impl crate::prelude::IntoHandler<Hello, flume::Receiver<Hello>>
    for (flume::Sender<Hello>, flume::Receiver<Hello>)
{
    fn into_handler(self) -> crate::prelude::Handler<Hello, flume::Receiver<Hello>> {
        let (sender, receiver) = self;
        (
            Box::new(move |s| {
                if let Err(e) = sender.send(s) {
                    log::warn!("Error sending Hello into flume channel: {}", e)
                }
            }),
            receiver,
        )
    }
}

fn scout<IntoWhatAmI, TryIntoConfig>(
    what: IntoWhatAmI,
    config: TryIntoConfig,
    callback: Callback<Hello>,
) -> ZResult<Scout>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
{
    let what = what.into();
    let config: crate::config::Config = match config.try_into() {
        Ok(config) => config,
        Err(_) => bail!("invalid configuration"),
    };

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

    let callback = Arc::from(callback);
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
                    let callback = (&callback as &Arc<dyn Fn(Hello) + Send + Sync>).clone();
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

    Ok(Scout { stop_sender })
}
