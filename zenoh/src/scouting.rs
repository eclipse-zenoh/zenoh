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
    prelude::{Callback, IntoHandler},
};
use async_std::net::UdpSocket;
use futures::StreamExt;
use std::{ops::Deref, sync::Arc};
use zenoh_config::{
    whatami::WhatAmIMatcher, ZN_MULTICAST_INTERFACE_DEFAULT, ZN_MULTICAST_IPV4_ADDRESS_DEFAULT,
};
use zenoh_core::Result as ZResult;
use zenoh_sync::Runnable;

/// Constants and helpers for zenoh `whatami` flags.
pub use zenoh_protocol_core::WhatAmI;

/// A zenoh Hello message.
pub use zenoh_protocol::proto::Hello;

#[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
#[derive(Debug, Clone)]
pub struct ScoutBuilder<IntoWhatAmI, TryIntoConfig>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    pub(crate) what: Option<IntoWhatAmI>,
    pub(crate) config: Option<TryIntoConfig>,
}

impl<IntoWhatAmI, TryIntoConfig> std::future::Future for ScoutBuilder<IntoWhatAmI, TryIntoConfig>
where
    IntoWhatAmI: Into<WhatAmIMatcher> + Unpin,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Unpin + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type Output = <Self as Runnable>::Output;

    #[inline]
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut async_std::task::Context<'_>,
    ) -> std::task::Poll<<Self as ::std::future::Future>::Output> {
        std::task::Poll::Ready(self.run())
    }
}

impl<IntoWhatAmI, TryIntoConfig> zenoh_sync::ZFuture for ScoutBuilder<IntoWhatAmI, TryIntoConfig>
where
    IntoWhatAmI: Into<WhatAmIMatcher> + Unpin + Send,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Unpin + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    #[inline]
    fn wait(mut self) -> Self::Output {
        self.run()
    }
}

impl<IntoWhatAmI, TryIntoConfig> Runnable for ScoutBuilder<IntoWhatAmI, TryIntoConfig>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type Output = ZResult<HandlerScout<flume::Receiver<Hello>>>;

    fn run(&mut self) -> Self::Output {
        let (callback, receiver) = flume::bounded(1).into_handler();
        scout(
            self.what.take().unwrap(),
            self.config.take().unwrap(),
            callback,
        )
        .map(|scout| HandlerScout { scout, receiver })
    }
}

#[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
#[derive(Debug, Clone)]
pub struct CallbackScoutBuilder<IntoWhatAmI, TryIntoConfig, Callback>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
    Callback: Fn(Hello) + Send + Sync + 'static,
{
    builder: ScoutBuilder<IntoWhatAmI, TryIntoConfig>,
    callback: Option<Callback>,
}

impl<IntoWhatAmI, TryIntoConfig, Callback> std::future::Future
    for CallbackScoutBuilder<IntoWhatAmI, TryIntoConfig, Callback>
where
    IntoWhatAmI: Into<WhatAmIMatcher> + Unpin,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Unpin + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
    Callback: Fn(Hello) + Unpin + Send + Sync + 'static,
{
    type Output = <Self as Runnable>::Output;

    #[inline]
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut async_std::task::Context<'_>,
    ) -> std::task::Poll<<Self as ::std::future::Future>::Output> {
        std::task::Poll::Ready(self.run())
    }
}

impl<IntoWhatAmI, TryIntoConfig, Callback> zenoh_sync::ZFuture
    for CallbackScoutBuilder<IntoWhatAmI, TryIntoConfig, Callback>
where
    IntoWhatAmI: Into<WhatAmIMatcher> + Unpin + Send,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Unpin + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
    Callback: Fn(Hello) + Unpin + Send + Sync + 'static,
{
    #[inline]
    fn wait(mut self) -> Self::Output {
        self.run()
    }
}

impl<IntoWhatAmI, TryIntoConfig, Callback> Runnable
    for CallbackScoutBuilder<IntoWhatAmI, TryIntoConfig, Callback>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
    Callback: Fn(Hello) + Send + Sync + 'static,
{
    type Output = ZResult<Scout>;

    fn run(&mut self) -> Self::Output {
        scout(
            self.builder.what.take().unwrap(),
            self.builder.config.take().unwrap(),
            Box::new(self.callback.take().unwrap()),
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

#[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
pub struct HandlerScoutBuilder<IntoWhatAmI, TryIntoConfig, Receiver>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    builder: ScoutBuilder<IntoWhatAmI, TryIntoConfig>,
    handler: Option<crate::prelude::Handler<Hello, Receiver>>,
}

impl<IntoWhatAmI, TryIntoConfig, Receiver> std::future::Future
    for HandlerScoutBuilder<IntoWhatAmI, TryIntoConfig, Receiver>
where
    IntoWhatAmI: Into<WhatAmIMatcher> + Unpin + Send,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Unpin + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
    Receiver: Unpin,
{
    type Output = <Self as Runnable>::Output;

    #[inline]
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut async_std::task::Context<'_>,
    ) -> std::task::Poll<<Self as ::std::future::Future>::Output> {
        std::task::Poll::Ready(self.run())
    }
}

impl<IntoWhatAmI, TryIntoConfig, Receiver> zenoh_sync::ZFuture
    for HandlerScoutBuilder<IntoWhatAmI, TryIntoConfig, Receiver>
where
    IntoWhatAmI: Into<WhatAmIMatcher> + Unpin + Send,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Unpin + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
    Receiver: Send + Sync + Unpin,
{
    #[inline]
    fn wait(mut self) -> Self::Output {
        self.run()
    }
}

impl<IntoWhatAmI, TryIntoConfig, Receiver> Runnable
    for HandlerScoutBuilder<IntoWhatAmI, TryIntoConfig, Receiver>
where
    IntoWhatAmI: Into<WhatAmIMatcher>,
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type Output = ZResult<HandlerScout<Receiver>>;

    fn run(&mut self) -> Self::Output {
        let (callback, receiver) = self.handler.take().unwrap();
        scout(
            self.builder.what.take().unwrap(),
            self.builder.config.take().unwrap(),
            callback,
        )
        .map(|scout| HandlerScout { scout, receiver })
    }
}

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
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    let what = what.into();
    let config: crate::config::Config = match config.try_into() {
        Ok(config) => config,
        Err(e) => bail!("invalid configuration {:?}", &e),
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
