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

//! [Zenoh](https://zenoh.io) /zeno/ is a stack that unifies data in motion, data at
//! rest and computations. It elegantly blends traditional pub/sub with geo distributed
//! storage, queries and computations, while retaining a level of time and space efficiency
//! that is well beyond any of the mainstream stacks.
//!
//! Before delving into the examples, we need to introduce few **Zenoh** concepts.
//! First off, in Zenoh you will deal with **Resources**, where a resource is made up of a
//! key and a value.  The other concept you'll have to familiarize yourself with are
//! **key expressions**, such as ```robot/sensor/temp```, ```robot/sensor/*```, ```robot/**```, etc.
//! As you can gather, the above key expression denotes set of keys, while the ```*``` and ```**```
//! are wildcards representing respectively (1) an arbitrary string of characters, with the exclusion of the ```/```
//! separator, and (2) an arbitrary sequence of characters including separators.
//!
//! Below are some examples that highlight these key concepts and show how easy it is to get
//! started with.
//!
//! # Examples
//! ### Publishing Data
//! The example below shows how to produce a value for a key expression.
//! ```
//! use zenoh::prelude::r#async::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).res().await.unwrap();
//!     session.put("key/expression", "value").res().await.unwrap();
//!     session.close().res().await.unwrap();
//! }
//! ```
//!
//! ### Subscribe
//! The example below shows how to consume values for a key expresison.
//! ```no_run
//! use futures::prelude::*;
//! use zenoh::prelude::r#async::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).res().await.unwrap();
//!     let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
//!     while let Ok(sample) = subscriber.recv_async().await {
//!         println!("Received: {:?}", sample);
//!     };
//! }
//! ```
//!
//! ### Query
//! The example below shows how to make a distributed query to collect the values associated with the
//! resources whose key match the given *key expression*.
//! ```
//! use futures::prelude::*;
//! use zenoh::prelude::r#async::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).res().await.unwrap();
//!     let replies = session.get("key/expression").res().await.unwrap();
//!     while let Ok(reply) = replies.recv_async().await {
//!         println!(">> Received {:?}", reply.sample);
//!     }
//! }
//! ```
#[macro_use]
extern crate zenoh_core;
#[macro_use]
extern crate zenoh_result;

pub(crate) type Id = u32;

use git_version::git_version;
use handlers::DefaultHandler;
#[cfg(feature = "unstable")]
use net::runtime::Runtime;
use prelude::*;
use scouting::ScoutBuilder;
use std::future::Ready;
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};
pub use zenoh_macros::{ke, kedefine, keformat, kewrite};
use zenoh_protocol::core::WhatAmIMatcher;
use zenoh_result::{zerror, ZResult};
use zenoh_util::concat_enabled_features;

/// A zenoh error.
pub use zenoh_result::Error;
/// A zenoh result.
pub use zenoh_result::ZResult as Result;

const GIT_VERSION: &str = git_version!(prefix = "v", cargo_prefix = "v");

pub const FEATURES: &str = concat_enabled_features!(
    prefix = "zenoh",
    features = [
        "auth_pubkey",
        "auth_usrpwd",
        "shared-memory",
        "stats",
        "transport_multilink",
        "transport_quic",
        "transport_serial",
        "transport_unixpipe",
        "transport_tcp",
        "transport_tls",
        "transport_udp",
        "transport_unixsock-stream",
        "transport_ws",
        "transport_vsock",
        "unstable",
        "default"
    ]
);

pub mod key_expr {
    pub use crate::api::key_expr::keyexpr;
    pub use crate::api::key_expr::OwnedKeyExpr;
    pub use crate::api::key_expr::kedefine;
    pub use crate::api::key_expr::keformat;
    // keyexpr format macro support
    pub mod format {
        pub use crate::api::key_expr::format::*;
        pub mod macro_support {
            pub use crate::api::key_expr::format::macro_support::*;
        }
    }
}


mod admin;
#[macro_use]
mod session;
pub use session::*;

mod api;
pub(crate) mod net;
pub use net::runtime;
pub mod selector;
#[deprecated = "This module is now a separate crate. Use the crate directly for shorter compile-times"]
pub use zenoh_config as config;
pub(crate) mod encoding;
pub mod handlers;
pub mod info;
#[cfg(feature = "unstable")]
pub mod liveliness;
pub mod payload;
pub mod plugins;
pub mod prelude;
pub mod publication;
pub mod query;
pub mod queryable;
pub mod sample;
pub mod subscriber;
pub mod value;
#[cfg(feature = "shared-memory")]
pub use zenoh_shm as shm;

/// A collection of useful buffers used by zenoh internally and exposed to the user to facilitate
/// reading and writing data.
pub use zenoh_buffers as buffers;

/// Time related types and functions.
pub mod time {
    use std::convert::TryFrom;

    pub use zenoh_protocol::core::{Timestamp, TimestampId, NTP64};

    /// Generates a reception [`Timestamp`] with id=0x01.
    /// This operation should be called if a timestamp is required for an incoming [`zenoh::Sample`](crate::Sample)
    /// that doesn't contain any timestamp.
    pub fn new_reception_timestamp() -> Timestamp {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        Timestamp::new(now.into(), TimestampId::try_from([1]).unwrap())
    }
}

/// Scouting primitives.
pub mod scouting;

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
        handler: DefaultHandler,
    }
}

/// Open a zenoh [`Session`].
///
/// # Arguments
///
/// * `config` - The [`Config`] for the zenoh session
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// # }
/// ```
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use std::str::FromStr;
/// use zenoh::prelude::r#async::*;
///
/// let mut config = config::peer();
/// config.set_id(ZenohId::from_str("221b72df20924c15b8794c6bdb471150").unwrap());
/// config.connect.endpoints.extend("tcp/10.10.10.10:7447,tcp/11.11.11.11:7447".split(',').map(|s|s.parse().unwrap()));
///
/// let session = zenoh::open(config).res().await.unwrap();
/// # }
/// ```
pub fn open<TryIntoConfig>(config: TryIntoConfig) -> OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    OpenBuilder { config }
}

/// A builder returned by [`open`] used to open a zenoh [`Session`].
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    config: TryIntoConfig,
}

impl<TryIntoConfig> Resolvable for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type To = ZResult<Session>;
}

impl<TryIntoConfig> SyncResolve for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        let config: crate::config::Config = self
            .config
            .try_into()
            .map_err(|e| zerror!("Invalid Zenoh configuration {:?}", &e))?;
        Session::new(config).res_sync()
    }
}

impl<TryIntoConfig> AsyncResolve for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

/// Initialize a Session with an existing Runtime.
/// This operation is used by the plugins to share the same Runtime as the router.
#[doc(hidden)]
#[zenoh_macros::unstable]
pub fn init(runtime: Runtime) -> InitBuilder {
    InitBuilder {
        runtime,
        aggregated_subscribers: vec![],
        aggregated_publishers: vec![],
    }
}

/// A builder returned by [`init`] and used to initialize a Session with an existing Runtime.
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[doc(hidden)]
#[zenoh_macros::unstable]
pub struct InitBuilder {
    runtime: Runtime,
    aggregated_subscribers: Vec<OwnedKeyExpr>,
    aggregated_publishers: Vec<OwnedKeyExpr>,
}

#[zenoh_macros::unstable]
impl InitBuilder {
    #[inline]
    pub fn aggregated_subscribers(mut self, exprs: Vec<OwnedKeyExpr>) -> Self {
        self.aggregated_subscribers = exprs;
        self
    }

    #[inline]
    pub fn aggregated_publishers(mut self, exprs: Vec<OwnedKeyExpr>) -> Self {
        self.aggregated_publishers = exprs;
        self
    }
}

#[zenoh_macros::unstable]
impl Resolvable for InitBuilder {
    type To = ZResult<Session>;
}

#[zenoh_macros::unstable]
impl SyncResolve for InitBuilder {
    fn res_sync(self) -> <Self as Resolvable>::To {
        Ok(Session::init(
            self.runtime,
            self.aggregated_subscribers,
            self.aggregated_publishers,
        )
        .res_sync())
    }
}

#[zenoh_macros::unstable]
impl AsyncResolve for InitBuilder {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}
