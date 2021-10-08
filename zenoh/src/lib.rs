//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

//! The zenoh API.
//!
//! # Examples
//!
//! ### Publish
//! ```
//! use zenoh::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).await.unwrap();
//!     session.put("/resource/name", "value").await.unwrap();
//!     session.close().await.unwrap();
//! }
//! ```
//!
//! ### Subscribe
//! ```no_run
//! use futures::prelude::*;
//! use zenoh::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).await.unwrap();
//!     let mut subscriber = session.subscribe("/resource/name").await.unwrap();
//!     while let Some(sample) = subscriber.receiver().next().await {
//!         println!("Received : {}", sample);
//!     };
//! }
//! ```
//!
//! ### Query
//! ```
//! use futures::prelude::*;
//! use zenoh::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).await.unwrap();
//!     let mut replies = session.get("/resource/name").await.unwrap();
//!     while let Some(reply) = replies.next().await {
//!         println!(">> Received {}", reply.data);
//!     }
//! }
//! ```

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate zenoh_util;

use async_std::net::UdpSocket;
use flume::bounded;
use futures::prelude::*;
use git_version::git_version;
use log::{debug, trace};
use net::protocol::core::WhatAmI;
use net::protocol::proto::data_kind;
use net::runtime::orchestrator::Loop;
use net::runtime::Runtime;
use prelude::config::whatami::WhatAmIMatcher;
use prelude::*;
use sync::{zready, ZFuture};
use zenoh_util::properties::config::*;
use zenoh_util::sync::zpinbox;

const GIT_VERSION: &str = git_version!(prefix = "v", cargo_prefix = "v");

#[macro_use]
mod session;
pub use session::*;

#[doc(hidden)]
pub mod net;

pub mod config;
pub mod info;
pub mod prelude;
pub mod publisher;
pub mod query;
pub mod queryable;
pub mod subscriber;
pub mod utils;

/// Some zenoh buffers.
pub mod buf {
    /// A read-only bytes buffer.
    pub use super::net::protocol::io::ZBuf;

    /// A [`ZBuf`] slice.
    pub use super::net::protocol::io::ZSlice;

    /// A writable bytes buffer.
    pub use super::net::protocol::io::WBuf;

    #[cfg(feature = "shared-memory")]
    pub use super::net::protocol::io::SharedMemoryBuf;
    #[cfg(feature = "shared-memory")]
    pub use super::net::protocol::io::SharedMemoryBufInfo;
    #[cfg(feature = "shared-memory")]
    pub use super::net::protocol::io::SharedMemoryManager;
}

/// Time related types and functions.
pub mod time {
    pub use super::net::protocol::core::{Timestamp, TimestampId, NTP64};

    /// A time period.
    pub use super::net::protocol::core::Period;

    /// Generates a reception [`Timestamp`] with id=0x00.  
    /// This operation should be called if a timestamp is required for an incoming [`zenoh::Sample`](crate::Sample)
    /// that doesn't contain any data_info or timestamp within its data_info.
    pub fn new_reception_timestamp() -> Timestamp {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        Timestamp::new(
            now.into(),
            TimestampId::new(1, [0u8; TimestampId::MAX_SIZE]),
        )
    }
}

/// A map of key/value (String,String) properties.
pub mod properties {
    use super::prelude::Value;
    pub use zenoh_util::properties::Properties;

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
pub mod sync {
    pub use zenoh_util::sync::zready;
    pub use zenoh_util::sync::ZFuture;
    pub use zenoh_util::sync::ZPinBoxFuture;
    pub use zenoh_util::sync::ZReady;

    /// A multi-producer, multi-consumer channel that can be accessed synchronously or asynchronously.
    pub mod channel {
        pub use zenoh_util::sync::channel::Iter;
        pub use zenoh_util::sync::channel::Receiver;
        pub use zenoh_util::sync::channel::RecvError;
        pub use zenoh_util::sync::channel::RecvTimeoutError;
        pub use zenoh_util::sync::channel::TryIter;
        pub use zenoh_util::sync::channel::TryRecvError;
    }
}

/// Scouting primitives.
pub mod scouting {
    use crate::sync::channel::{
        Iter, Receiver, RecvError, RecvTimeoutError, TryIter, TryRecvError,
    };
    use flume::Sender;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// Constants and helpers for zenoh `whatami` flags.
    pub use super::net::protocol::core::WhatAmI;

    /// A zenoh Hello message.
    pub use super::net::protocol::proto::Hello;

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
pub fn scout<I: Into<WhatAmIMatcher>, IntoConfig>(
    what: I,
    config: IntoConfig,
) -> impl ZFuture<Output = ZResult<scouting::HelloReceiver>>
where
    IntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <IntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    let what = what.into();
    let config: crate::config::Config = config.try_into().unwrap();
    trace!("scout({}, {})", what, &config);
    let addr = config
        .scouting
        .multicast
        .address()
        .unwrap_or_else(|| ZN_MULTICAST_IPV4_ADDRESS_DEFAULT.parse().unwrap());
    let ifaces = config
        .scouting
        .multicast
        .interface()
        .as_ref()
        .map(|s| s.as_ref())
        .unwrap_or(ZN_MULTICAST_INTERFACE_DEFAULT);

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
/// * `config` - The [`ConfigProperties`] for the zenoh session
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
/// # Configuration Properties
///
/// [`ConfigProperties`] are a set of key/value (`u64`/`String`) pairs.
/// Constants for the accepted keys can be found in the [`config`] module.
/// Multiple values are coma separated.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
///
/// let mut config = config::peer();
/// config.insert(config::ZN_LOCAL_ROUTING_KEY, "false".to_string());
/// config.insert(config::ZN_PEER_KEY, "tcp/10.10.10.10:7447,tcp/11.11.11.11:7447".to_string());
///
/// let session = zenoh::open(config).await.unwrap();
/// # })
/// ```
///
/// [`ConfigProperties`] can be built set of key/value (`String`/`String`) set
/// of [`Properties`](crate::properties::Properties).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
///
/// let mut config = Properties::default();
/// config.insert("local_routing".to_string(), "false".to_string());
/// config.insert("peer".to_string(), "tcp/10.10.10.10:7447,tcp/11.11.11.11:7447".to_string());
///
/// let session = zenoh::open(config).await.unwrap();
/// # })
/// ```
pub fn open<IntoConfig>(config: IntoConfig) -> impl ZFuture<Output = ZResult<Session>>
where
    IntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <IntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    let config = config.try_into().unwrap();
    debug!("Zenoh Rust API {}", GIT_VERSION);
    debug!("Config: {:?}", &config);
    Session::new(config)
}

/// Initialize a Session with an existing Runtime.
/// This operation is used by the plugins to share the same Runtime than the router.
#[doc(hidden)]
pub fn init(runtime: Runtime) -> impl ZFuture<Output = ZResult<Session>> {
    zpinbox(async { Ok(Session::init(runtime, true, vec![], vec![]).await) })
}
