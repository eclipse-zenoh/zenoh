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
//! use zenoh::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = open(config::default()).await.unwrap();
//!     session.put("/resource/name", "value").await.unwrap();
//!     session.close().await.unwrap();
//! }
//! ```
//!
//! ### Subscribe
//! ```no_run
//! use zenoh::*;
//! use futures::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = open(config::default()).await.unwrap();
//!     let mut subscriber = session.subscribe("/resource/name").await.unwrap();
//!     while let Some(sample) = subscriber.receiver().next().await {
//!         println!("Received : {:?}", sample);
//!     };
//! }
//! ```
//!
//! ### Query
//! ```
//! use zenoh::*;
//! use futures::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     let session = open(config::default()).await.unwrap();
//!     let mut replies = session.get("/resource/name").await.unwrap();
//!     while let Some(reply) = replies.next().await {
//!         println!(">> Received {:?}", reply.data);
//!     }
//! }
//! ```

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate zenoh_util;

#[doc(hidden)]
pub mod net;

use async_std::net::UdpSocket;
use flume::bounded;
use futures::prelude::*;
use log::{debug, trace};
use net::protocol::core::WhatAmI;
use net::protocol::proto::data_kind;
use net::runtime::orchestrator::Loop;
use net::runtime::Runtime;
use zenoh_util::properties::config::*;
use zenoh_util::sync::zpinbox;
// Shared memory and zero-copy
#[cfg(feature = "zero-copy")]
pub use net::protocol::io::{SharedMemoryBuf, SharedMemoryBufInfo, SharedMemoryManager};

pub mod utils;

#[macro_use]
mod types;
use git_version::git_version;
pub use types::*;

mod selector;
pub use selector::*;

pub mod info;

#[macro_use]
mod session;
pub use session::*;

pub use net::protocol::core::{Timestamp, TimestampId};
pub use net::protocol::proto::encoding;

pub mod queryable {
    #[doc(inline)]
    pub use super::net::protocol::core::queryable::*;
}

pub mod config;
pub use zenoh_util::properties::config::ConfigProperties;
pub use zenoh_util::properties::Properties;

const GIT_VERSION: &str = git_version!(prefix = "v", cargo_prefix = "v");

/// Scout for routers and/or peers.
///
/// [scout](scout) spawns a task that periodically sends scout messages and returns
/// a [HelloReceiver](HelloReceiver) : a stream of received [Hello](Hello) messages.
///
/// Drop the returned [HelloReceiver](HelloReceiver) to stop the scouting task.
///
/// # Arguments
///
/// * `what` - The kind of zenoh process to scout for
/// * `config` - The configuration [Properties](super::Properties) to use for scouting
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::*;
/// use futures::prelude::*;
///
/// let mut receiver = scout(whatami::PEER | whatami::ROUTER, config::default()).await.unwrap();
/// while let Some(hello) = receiver.next().await {
///     println!("{}", hello);
/// }
/// # })
/// ```
pub fn scout(
    what: WhatAmI,
    config: ConfigProperties,
) -> impl ZFuture<Output = ZResult<HelloReceiver>> {
    trace!("scout({}, {})", what, &config);
    let addr = config
        .get_or(&ZN_MULTICAST_ADDRESS_KEY, ZN_MULTICAST_ADDRESS_DEFAULT)
        .parse()
        .unwrap();
    let ifaces = config.get_or(&ZN_MULTICAST_INTERFACE_KEY, ZN_MULTICAST_INTERFACE_DEFAULT);

    let (hello_sender, hello_receiver) = bounded::<Hello>(1);
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

    zready(Ok(HelloReceiver::new(stop_sender, hello_receiver)))
}

/// Open a zenoh-net [Session](Session).
///
/// # Arguments
///
/// * `config` - The [ConfigProperties](ConfigProperties) for the zenoh-net session
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::*;
///
/// let session = open(config::peer()).await.unwrap();
/// # })
/// ```
///
/// # Configuration Properties
///
/// [ConfigProperties](ConfigProperties) are a set of key/value (`u64`/`String`) pairs.
/// Constants for the accepted keys can be found in the [config](config) module.
/// Multiple values are coma separated.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::*;
///
/// let mut config = config::peer();
/// config.insert(config::ZN_LOCAL_ROUTING_KEY, "false".to_string());
/// config.insert(config::ZN_PEER_KEY, "tcp/10.10.10.10:7447,tcp/11.11.11.11:7447".to_string());
///
/// let session = open(config).await.unwrap();
/// # })
/// ```
///
/// [ConfigProperties](ConfigProperties) can be built set of key/value (`String`/`String`) set
/// of [Properties](super::Properties).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::*;
///
/// let mut config = Properties::default();
/// config.insert("local_routing".to_string(), "false".to_string());
/// config.insert("peer".to_string(), "tcp/10.10.10.10:7447,tcp/11.11.11.11:7447".to_string());
///
/// let session = open(config).await.unwrap();
/// # })
/// ```
pub fn open<IntoConfigProperties>(
    config: IntoConfigProperties,
) -> impl ZFuture<Output = ZResult<Session>>
where
    IntoConfigProperties: Into<ConfigProperties>,
{
    let config = config.into();
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
