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
//! [Zenoh](https://zenoh.io) /zeno/ is a stack that unifies data in motion, data at
//! rest and computations. It elegantly blends traditional pub/sub with geo distributed
//! storage, queries and computations, while retaining a level of time and space efficiency
//! that is well beyond any of the mainstream stacks.
//!
//! Below are some examples that highlight the its key comcepts and show how easy it is to get
//! started with it.
//!
//! # Examples
//! Before delving into the examples, we need to introduce few **zenoh** concepts.
//! First off, in zenoh you will deal with **Resources**, where a resource is made up of a
//! key and a value.  The other concept you'll have to familiarize yourself with are
//! **key expressions**, such as ```/robot/sensor/temp```, ```/robot/sensor/*```, ```/robot/**```, etc.
//! As you can gather,  the above key expression denotes set of keys, while the ```*``` and ```**```
//! are wildcards representing respectively (1) an arbirary string of characters, with the exclusion of the ```/```
//! separator, and (2) an arbitrary sequence of characters including separators.
//!
//! ### Publishing Data
//! The example below shows how to produce a value for a key expression.
//! ```
//! use zenoh::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).await.unwrap();
//!     session.put("/key/expression", "value").await.unwrap();
//!     session.close().await.unwrap();
//! }
//! ```
//!
//! ### Subscribe
//! The example below shows how to consume values for a key expresison.
//! ```no_run
//! use futures::prelude::*;
//! use zenoh::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).await.unwrap();
//!     let subscriber = session.subscribe("/key/expression").await.unwrap();
//!     while let Ok(sample) = subscriber.recv_async().await {
//!         println!("Received : {}", sample);
//!     };
//! }
//! ```
//!
//! ### Query
//! The example below shows how to make a distributed query to collect the values associated with the
//! resources whose key match the given *key expression*.
//! ```
//! use futures::prelude::*;
//! use zenoh::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).await.unwrap();
//!     let replies = session.get("/key/expression").await.unwrap();
//!     while let Ok(reply) = replies.recv_async().await {
//!         println!(">> Received {:?}", reply.sample);
//!     }
//! }
//! ```
#[macro_use]
extern crate zenoh_core;

use async_std::net::UdpSocket;
use async_std::task;
use flume::bounded;
use futures::prelude::*;
use git_version::git_version;
use log::trace;
use net::protocol::proto::data_kind;
use net::runtime::orchestrator::Loop;
use net::runtime::Runtime;
use prelude::config::whatami::WhatAmIMatcher;
use prelude::*;
use sync::{zready, ZFuture};
use zenoh_cfg_properties::config::*;
use zenoh_core::{zerror, Result as ZResult};
use zenoh_sync::Runnable;

/// A zenoh result.
pub use zenoh_core::Result;

const GIT_VERSION: &str = git_version!(prefix = "v", cargo_prefix = "v");

#[macro_use]
mod session;
pub use session::*;

#[doc(hidden)]
pub mod net;

#[deprecated = "This module is now a separate crate. Use the crate directly for shorter compile-times"]
pub use zenoh_config as config;
pub mod info;
pub mod prelude;
pub mod publication;
pub mod query;
pub mod queryable;
pub mod subscriber;
pub mod utils;

pub mod plugins;

/// A collection of useful buffers used by zenoh internally and exposed to the user to facilitate
/// reading and writing data.
pub use zenoh_buffers as buf;

/// Time related types and functions.
pub mod time {
    pub use zenoh_protocol_core::{Timestamp, TimestampId, NTP64};

    /// A time period.
    pub use zenoh_protocol_core::Period;

    /// Generates a reception [`Timestamp`] with id=0x00.  
    /// This operation should be called if a timestamp is required for an incoming [`zenoh::Sample`](crate::Sample)
    /// that doesn't contain any timestamp.
    pub fn new_reception_timestamp() -> Timestamp {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        Timestamp::new(
            now.into(),
            TimestampId::new(1, [0_u8; TimestampId::MAX_SIZE]),
        )
    }
}

/// A map of key/value (String,String) properties.
pub mod properties {
    use super::prelude::Value;
    pub use zenoh_cfg_properties::Properties;

    /// Convert a set of [`Properties`] into a [`Value`].  
    /// For instance such Properties: `[("k1", "v1"), ("k2, v2")]`  
    /// are converted into such Json: `{ "k1": "v1", "k2": "v2" }`
    pub fn properties_to_json_value(props: &Properties) -> Value {
        let json_map = props
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect::<serde_json::map::Map<String, serde_json::Value>>();
        serde_json::Value::Object(json_map).into()
    }
}

#[allow(clippy::needless_doctest_main)]
/// Synchronisation primitives.
///
/// This module provides some traits that provide some syncronous accessors to some outputs :
/// [`ZFuture`] for a single output and [`channel::Receiver`](crate::sync::channel::Receiver) for multiple outputs.
///
/// Most zenoh types that provide a single output both implment [`ZFuture`] and [`futures::Future`]
/// and allow users to access their output synchronously via [`ZFuture::wait()`] or asynchronously
/// via `.await`.
///
/// Most zenoh types that provide multiple outputs both implment [`channel::Receiver`](crate::sync::channel::Receiver) and
/// [`futures::Stream`] and allow users to access their output synchronously via [`channel::Receiver::recv()`](crate::sync::channel::Receiver::recv)
/// or asynchronously via `.next().await`.
///
/// # Examples
///
/// ### Sync
/// ```no_run
/// use zenoh::prelude::*;
/// use zenoh::scouting::WhatAmI;
///
/// fn main() {
///     let mut receiver = zenoh::scout(WhatAmI::Router, config::default()).wait().unwrap();
///     while let Ok(hello) = receiver.recv() {
///         println!("{}", hello);
///     }
/// }
/// ```
///
/// ### Async
/// ```no_run
/// use futures::prelude::*;
/// use zenoh::prelude::*;
/// use zenoh::scouting::WhatAmI;
///
/// #[async_std::main]
/// async fn main() {
///     let mut receiver = zenoh::scout(WhatAmI::Router, config::default()).await.unwrap();
///     while let Some(hello) = receiver.next().await {
///         println!("{}", hello);
///     }
/// }
/// ```
#[deprecated = "This module is now a separate crate. Use the crate directly for shorter compile-times"]
pub mod sync {
    pub use zenoh_sync::zready;
    pub use zenoh_sync::ZFuture;
    pub use zenoh_sync::ZPinBoxFuture;
    pub use zenoh_sync::ZReady;

    /// A multi-producer, multi-consumer channel that can be accessed synchronously or asynchronously.
    pub mod channel {
        pub use zenoh_sync::channel::Iter;
        pub use zenoh_sync::channel::Receiver;
        pub use zenoh_sync::channel::RecvError;
        pub use zenoh_sync::channel::RecvFut;
        pub use zenoh_sync::channel::RecvTimeoutError;
        pub use zenoh_sync::channel::TryIter;
        pub use zenoh_sync::channel::TryRecvError;
    }
}

/// Scouting primitives.
pub mod scouting {
    use crate::sync::channel::{
        Iter, Receiver, RecvError, RecvFut, RecvTimeoutError, TryIter, TryRecvError,
    };
    use flume::Sender;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use zenoh_sync::zreceiver;

    /// Constants and helpers for zenoh `whatami` flags.
    pub use zenoh_protocol_core::WhatAmI;

    /// A zenoh Hello message.
    pub use zenoh_protocol::proto::Hello;

    zreceiver! {
        /// A [`Receiver`] of [`Hello`] messages returned by the [`scout`](crate::scout) operation.
        #[derive(Clone)]
        pub struct HelloReceiver : Receiver<Hello> {
            pub(crate) stop_sender: Sender<()>,
        }
    }
}

/// Scout for routers and/or peers.
///
/// [`scout`] spawns a task that periodically sends scout messages and returns a
/// [`HelloReceiver`](crate::scouting::HelloReceiver) : a stream of received [`Hello`](crate::scouting::Hello) messages.
///
/// Drop the returned [`HelloReceiver`](crate::scouting::HelloReceiver) to stop the scouting task.
///
/// # Arguments
///
/// * `what` - The kind of zenoh process to scout for
/// * `config` - The configuration [`Properties`](crate::properties::Properties) to use for scouting
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use futures::prelude::*;
/// use zenoh::prelude::*;
/// use zenoh::scouting::WhatAmI;
///
/// let mut receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default()).await.unwrap();
/// while let Some(hello) = receiver.next().await {
///     println!("{}", hello);
/// }
/// # })
/// ```
pub fn scout<I: Into<WhatAmIMatcher>, TryIntoConfig>(
    what: I,
    config: TryIntoConfig,
) -> impl ZFuture<Output = ZResult<scouting::HelloReceiver>>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    let what = what.into();
    let config: crate::config::Config = match config.try_into() {
        Ok(config) => config,
        Err(e) => return zready(Err(zerror!("invalid configuration {:?}", &e).into())),
    };

    trace!("scout({}, {})", what, &config);

    let default_addr = match ZN_MULTICAST_IPV4_ADDRESS_DEFAULT.parse() {
        Ok(addr) => addr,
        Err(e) => {
            return zready(Err(zerror!(
                "invalid default addr {}: {:?}",
                ZN_MULTICAST_IPV4_ADDRESS_DEFAULT,
                &e
            )
            .into()))
        }
    };

    let addr = config.scouting.multicast.address().unwrap_or(default_addr);
    let ifaces = config
        .scouting
        .multicast
        .interface()
        .as_ref()
        .map_or(ZN_MULTICAST_INTERFACE_DEFAULT, |s| s.as_ref());

    let (hello_sender, hello_receiver) = bounded::<scouting::Hello>(1);
    let (stop_sender, stop_receiver) = bounded::<()>(1);

    let ifaces = Runtime::get_interfaces(ifaces);
    if !ifaces.is_empty() {
        let sockets: Vec<UdpSocket> = ifaces
            .into_iter()
            .filter_map(|iface| Runtime::bind_ucast_port(iface).ok())
            .collect();
        if !sockets.is_empty() {
            async_std::task::spawn(async move {
                let hello_sender = &hello_sender;
                let mut stop_receiver = stop_receiver.stream();
                let scout = Runtime::scout(&sockets, what, &addr, move |hello| async move {
                    let _ = hello_sender.send_async(hello).await;
                    Loop::Continue
                });
                let stop = async move {
                    stop_receiver.next().await;
                    trace!("stop scout({}, {})", what, &config);
                };
                async_std::prelude::FutureExt::race(scout, stop).await;
            });
        }
    }

    zready(Ok(scouting::HelloReceiver::new(
        stop_sender,
        hello_receiver,
    )))
}

/// Open a zenoh [`Session`].
///
/// # Arguments
///
/// * `config` - The [`Config`](crate::config::Config) for the zenoh session
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(config::peer()).await.unwrap();
/// # })
/// ```
///
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
///
/// let mut config = config::peer();
/// config.set_local_routing(Some(false));
/// config.connect.endpoints.extend("tcp/10.10.10.10:7447,tcp/11.11.11.11:7447".split(',').map(|s|s.parse().unwrap()));
///
/// let session = zenoh::open(config).await.unwrap();
/// # })
/// ```
#[must_use = "OpenBuilder does nothing unless you `.wait()`, `.await` or poll it"]
pub fn open<TryIntoConfig>(config: TryIntoConfig) -> OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    OpenBuilder {
        config: Some(config),
    }
}

pub struct OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    config: Option<TryIntoConfig>,
}

impl<TryIntoConfig> Runnable for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type Output = ZResult<Session>;

    #[inline]
    fn run(&mut self) -> Self::Output {
        match self.config.take() {
            Some(c) => {
                let config: crate::config::Config = c
                    .try_into()
                    .map_err(|e| zerror!("Invalid Zenoh configuration {:?}", &e))?;
                task::block_on(Session::new(config))
            }
            None => {
                bail!("You can not run the same OpenBuilder more than once.")
            }
        }
    }
}

impl<TryIntoConfig> ::std::future::Future for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + Unpin + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type Output = <Self as Runnable>::Output;

    #[inline]
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut task::Context<'_>,
    ) -> std::task::Poll<<Self as ::std::future::Future>::Output> {
        std::task::Poll::Ready(self.run())
    }
}

impl<TryIntoConfig> zenoh_sync::ZFuture for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + Unpin + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    #[inline]
    fn wait(mut self) -> Self::Output {
        self.run()
    }
}

/// Initialize a Session with an existing Runtime.
/// This operation is used by the plugins to share the same Runtime than the router.
#[doc(hidden)]
#[must_use = "InitBuilder does nothing unless you `.wait()`, `.await` or poll it"]
pub fn init(runtime: Runtime) -> InitBuilder {
    InitBuilder {
        runtime: Some(runtime),
    }
}

pub struct InitBuilder {
    runtime: Option<Runtime>,
}

impl Runnable for InitBuilder {
    type Output = ZResult<Session>;

    #[inline]
    fn run(&mut self) -> Self::Output {
        match self.runtime.take() {
            Some(r) => Ok(task::block_on(Session::init(r, true, vec![], vec![]))),
            None => {
                bail!("You can not run the same InitBuilder more than once.")
            }
        }
    }
}

impl ::std::future::Future for InitBuilder {
    type Output = <Self as Runnable>::Output;

    #[inline]
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut task::Context<'_>,
    ) -> std::task::Poll<<Self as ::std::future::Future>::Output> {
        std::task::Poll::Ready(self.run())
    }
}

impl zenoh_sync::ZFuture for InitBuilder {
    #[inline]
    fn wait(mut self) -> Self::Output {
        self.run()
    }
}
