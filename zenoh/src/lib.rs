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
//! #[async_std::main]
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
//! #[async_std::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).res().await.unwrap();
//!     let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
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
//! use zenoh::prelude::r#async::*;
//!
//! #[async_std::main]
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

use handlers::DefaultHandler;
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};

use git_version::git_version;
use net::runtime::Runtime;
use prelude::config::whatami::WhatAmIMatcher;
use prelude::*;
use scouting::ScoutBuilder;
use zenoh_core::{zerror, Result as ZResult};

/// A zenoh result.
pub use zenoh_core::Error;
pub use zenoh_core::Result;

const GIT_VERSION: &str = git_version!(prefix = "v", cargo_prefix = "v");

#[macro_use]
mod session;
pub use session::*;

pub mod key_expr;
pub(crate) mod net;
pub use net::runtime;
pub mod selector;
#[deprecated = "This module is now a separate crate. Use the crate directly for shorter compile-times"]
pub use zenoh_config as config;
pub mod handlers;
pub mod info;
pub mod plugins;
pub mod prelude;
pub mod publication;
pub mod query;
pub mod queryable;
pub mod sample;
pub mod subscriber;
pub mod utils;
pub mod value;

/// A collection of useful buffers used by zenoh internally and exposed to the user to facilitate
/// reading and writing data.
pub use zenoh_buffers as buffers;

/// Time related types and functions.
pub mod time {
    use std::convert::TryFrom;

    pub use zenoh_protocol_core::{Timestamp, TimestampId, NTP64};

    /// Generates a reception [`Timestamp`] with id=0x01.  
    /// This operation should be called if a timestamp is required for an incoming [`zenoh::Sample`](crate::Sample)
    /// that doesn't contain any timestamp.
    pub fn new_reception_timestamp() -> Timestamp {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        Timestamp::new(now.into(), TimestampId::try_from([1]).unwrap())
    }
}

/// A map of key/value (String,String) properties.
pub mod properties {
    use super::prelude::Value;
    pub use zenoh_cfg_properties::Properties;

    /// Convert a set of [`Properties`] into a [`Value`].  
    /// For instance, Properties: `[("k1", "v1"), ("k2, v2")]`  
    /// is converted into Json: `{ "k1": "v1", "k2": "v2" }`
    pub fn properties_to_json_value(props: &Properties) -> Value {
        let json_map = props
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect::<serde_json::map::Map<String, serde_json::Value>>();
        serde_json::Value::Object(json_map).into()
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
/// * `config` - The configuration [`Properties`](crate::properties::Properties) to use for scouting
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
/// use zenoh::scouting::WhatAmI;
///
/// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, config::default()).res().await.unwrap();
/// while let Ok(hello) = receiver.recv_async().await {
///     println!("{}", hello);
/// }
/// # })
/// ```
pub fn scout<I: Into<WhatAmIMatcher>, TryIntoConfig>(
    what: I,
    config: TryIntoConfig,
) -> ScoutBuilder<DefaultHandler>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: Into<zenoh_core::Error>,
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
/// * `config` - The [`Config`](crate::config::Config) for the zenoh session
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// # })
/// ```
///
/// ```
/// # async_std::task::block_on(async {
/// use std::str::FromStr;
/// use zenoh::prelude::r#async::*;
///
/// let mut config = config::peer();
/// config.set_id(ZenohId::from_str("F000").unwrap());
/// config.connect.endpoints.extend("tcp/10.10.10.10:7447,tcp/11.11.11.11:7447".split(',').map(|s|s.parse().unwrap()));
///
/// let session = zenoh::open(config).res().await.unwrap();
/// # })
/// ```
pub fn open<TryIntoConfig>(config: TryIntoConfig) -> OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    OpenBuilder {
        config,
        publications_destination: Locality::default(),
        subscribers_origin: Locality::default(),
        queries_destination: Locality::default(),
        queryables_origin: Locality::default(),
    }
}

/// A builder returned by [`open`] used to open a zenoh [`Session`].
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// # })
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    config: TryIntoConfig,
    publications_destination: Locality,
    subscribers_origin: Locality,
    queries_destination: Locality,
    queryables_origin: Locality,
}

impl<TryIntoConfig> OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    /// Restrict the matching subscribers that will receive data published from this [`Session`]
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn publications_allowed_destination(mut self, destination: Locality) -> Self {
        self.publications_destination = destination;
        self
    }

    /// Restrict the matching publications that will be receive by the [`Subscribers`](crate::subscriber::Subscriber)
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn subscribers_allowed_origin(mut self, origin: Locality) -> Self {
        self.subscribers_origin = origin;
        self
    }
    /// Restrict the matching queryables that will receive queries from this [`Session`]
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn queries_allowed_destination(mut self, destination: Locality) -> Self {
        self.publications_destination = destination;
        self
    }

    /// Restrict the matching queries that will be receive by the [`Queryables`](crate::queryable::Queryable)
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn queriables_allowed_origin(mut self, origin: Locality) -> Self {
        self.queryables_origin = origin;
        self
    }
}

impl<TryIntoConfig> Resolvable for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type Output = ZResult<Session>;
}

impl<TryIntoConfig> SyncResolve for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    #[inline]
    fn res_sync(self) -> Self::Output {
        let config: crate::config::Config = self
            .config
            .try_into()
            .map_err(|e| zerror!("Invalid Zenoh configuration {:?}", &e))?;
        Session::new(
            config,
            self.publications_destination,
            self.subscribers_origin,
            self.queries_destination,
            self.queryables_origin,
        )
        .res_sync()
    }
}

impl<TryIntoConfig> AsyncResolve for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type Future = zenoh_sync::PinBoxFuture<Self::Output>;

    fn res_async(self) -> Self::Future {
        zenoh_sync::pinbox(async move {
            let config: crate::config::Config = self
                .config
                .try_into()
                .map_err(|e| zerror!("Invalid Zenoh configuration {:?}", &e))?;
            Session::new(
                config,
                self.publications_destination,
                self.subscribers_origin,
                self.queries_destination,
                self.queryables_origin,
            )
            .res_async()
            .await
        })
    }
}

/// Initialize a Session with an existing Runtime.
/// This operation is used by the plugins to share the same Runtime as the router.
#[doc(hidden)]
pub fn init(runtime: Runtime) -> InitBuilder {
    InitBuilder {
        runtime,
        publications_destination: Locality::default(),
        subscribers_origin: Locality::default(),
        queries_destination: Locality::default(),
        queryables_origin: Locality::default(),
        aggregated_subscribers: vec![],
        aggregated_publishers: vec![],
    }
}

/// A builder returned by [`init`] and used to initialize a Session with an existing Runtime.
#[doc(hidden)]
pub struct InitBuilder {
    runtime: Runtime,
    publications_destination: Locality,
    subscribers_origin: Locality,
    queries_destination: Locality,
    queryables_origin: Locality,
    aggregated_subscribers: Vec<OwnedKeyExpr>,
    aggregated_publishers: Vec<OwnedKeyExpr>,
}

impl InitBuilder {
    /// Restrict the matching subscribers that will receive data published from this [`Session`]
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn publications_allowed_destination(mut self, destination: Locality) -> Self {
        self.publications_destination = destination;
        self
    }

    /// Restrict the matching publications that will be receive by the [`Subscribers`](crate::subscriber::Subscriber)
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn subscribers_allowed_origin(mut self, origin: Locality) -> Self {
        self.subscribers_origin = origin;
        self
    }
    /// Restrict the matching queryables that will receive queries from this [`Session`]
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn queries_allowed_destination(mut self, destination: Locality) -> Self {
        self.publications_destination = destination;
        self
    }

    /// Restrict the matching queries that will be receive by the [`Queryables`](crate::queryable::Queryable)
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn queriables_allowed_origin(mut self, origin: Locality) -> Self {
        self.queryables_origin = origin;
        self
    }

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

impl Resolvable for InitBuilder {
    type Output = ZResult<Session>;
}
impl SyncResolve for InitBuilder {
    #[inline]
    fn res_sync(self) -> Self::Output {
        Ok(Session::init(
            self.runtime,
            self.publications_destination,
            self.subscribers_origin,
            self.queries_destination,
            self.queryables_origin,
            self.aggregated_subscribers,
            self.aggregated_publishers,
        )
        .res_sync())
    }
}

impl AsyncResolve for InitBuilder {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}
