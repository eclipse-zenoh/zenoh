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
use std::{fmt, net::SocketAddr, ops::Deref, time::Duration};

use tokio::net::UdpSocket;
use zenoh_config::wrappers::Hello;
use zenoh_protocol::core::WhatAmIMatcher;
use zenoh_result::ZResult;
use zenoh_task::TerminatableTask;

use crate::{
    api::{
        builders::scouting::ScoutBuilder,
        handlers::{Callback, CallbackParameter, DefaultHandler},
    },
    net::runtime::{orchestrator::Loop, Runtime},
    Config,
};

impl CallbackParameter for Hello {
    type Message<'a> = Self;

    fn from_message(msg: Self::Message<'_>) -> Self {
        msg
    }
}

/// A scout that returns [`Hello`] messages through a callback.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::config::WhatAmI;
///
/// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, zenoh::Config::default())
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
    /// use zenoh::config::WhatAmI;
    ///
    /// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, zenoh::Config::default())
    ///     .callback(|hello| { println!("{}", hello); })
    ///     .await
    ///     .unwrap();
    /// scout.stop();
    /// # }
    /// ```
    pub fn stop(self) {
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

/// A scout that returns [`Hello`] messages through a [`Handler`](crate::handlers::IntoHandler).
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::config::WhatAmI;
///
/// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, zenoh::Config::default())
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
    /// use zenoh::config::WhatAmI;
    ///
    /// let scout = zenoh::scout(WhatAmI::Router, zenoh::Config::default())
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

pub(crate) fn _scout(
    what: WhatAmIMatcher,
    config: Config,
    callback: Callback<Hello>,
) -> ZResult<ScoutInner> {
    tracing::trace!("scout({}, {})", what, &config);
    let default_addr = SocketAddr::from(zenoh_config::defaults::scouting::multicast::address);
    let addr = config
        .0
        .scouting
        .multicast
        .address()
        .unwrap_or(default_addr);
    let default_multicast_ttl = zenoh_config::defaults::scouting::multicast::ttl;
    let multicast_ttl = config
        .0
        .scouting
        .multicast
        .ttl
        .unwrap_or(default_multicast_ttl);
    let ifaces = config.0.scouting.multicast.interface().as_ref().map_or(
        zenoh_config::defaults::scouting::multicast::interface,
        |s| s.as_ref(),
    );
    let ifaces = Runtime::get_interfaces(ifaces);
    if !ifaces.is_empty() {
        let sockets: Vec<UdpSocket> = ifaces
            .into_iter()
            .filter_map(|iface| Runtime::bind_ucast_port(iface, multicast_ttl).ok())
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
                            callback.call(hello.into());
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
/// Drop the returned [`Scout`] to stop the scouting task.
///
/// # Arguments
///
/// * `what` - The kind of zenoh process to scout for
/// * `config` - The configuration [`crate::Config`] to use for scouting
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::config::WhatAmI;
///
/// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, zenoh::Config::default())
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
