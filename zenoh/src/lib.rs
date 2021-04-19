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

//! The crate of the zenoh API.
//!
//! See the [Zenoh] struct for details.
//!
//! # Quick start examples
//!
//! ### Put a key/value into zenoh
//! ```
//! use zenoh::*;
//! use std::convert::TryInto;
//!
//! #[async_std::main]
//! async fn main() {
//!     let zenoh = Zenoh::new(net::config::default()).await.unwrap();
//!     let workspace = zenoh.workspace(None).await.unwrap();
//!     workspace.put(
//!         &"/demo/example/hello".try_into().unwrap(),
//!         "Hello World!".into()
//!     ).await.unwrap();
//!     zenoh.close().await.unwrap();
//! }
//! ```
//!
//! ### Subscribe for keys/values changes from zenoh
//! ```no_run
//! use zenoh::*;
//! use futures::prelude::*;
//! use std::convert::TryInto;
//!
//! #[async_std::main]
//! async fn main() {
//!     let zenoh = Zenoh::new(net::config::default()).await.unwrap();
//!     let workspace = zenoh.workspace(None).await.unwrap();
//!     let mut change_stream =
//!         workspace.subscribe(&"/demo/example/**".try_into().unwrap()).await.unwrap();
//!     while let Some(change) = change_stream.next().await {
//!         println!(">> {:?} for {} : {:?} at {}",
//!             change.kind, change.path, change.value, change.timestamp
//!         )
//!     }
//!     change_stream.close().await.unwrap();
//!     zenoh.close().await.unwrap();
//! }
//! ```
//!
//! ### Get keys/values from zenoh
//! ```no_run
//! use zenoh::*;
//! use futures::prelude::*;
//! use std::convert::TryInto;
//!
//! #[async_std::main]
//! async fn main() {
//!     let zenoh = Zenoh::new(net::config::default()).await.unwrap();
//!     let workspace = zenoh.workspace(None).await.unwrap();
//!     let mut data_stream = workspace.get(&"/demo/example/**".try_into().unwrap()).await.unwrap();
//!     while let Some(data) = data_stream.next().await {
//!         println!(">> {} : {:?} at {}",
//!             data.path, data.value, data.timestamp
//!         )
//!     }
//!     zenoh.close().await.unwrap();
//! }
//! ```
#![doc(
    html_logo_url = "http://zenoh.io/img/zenoh-dragon.png",
    html_favicon_url = "http://zenoh.io/favicon-32x32.png",
    html_root_url = "https://eclipse-zenoh.github.io/zenoh/zenoh/"
)]
#![feature(async_closure)]
#![feature(map_into_keys_values)]
#![feature(can_vector)]
#![recursion_limit = "256"]

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate zenoh_util;

extern crate async_std;
extern crate uuid;

use log::debug;

pub mod net;

use net::info::ZN_INFO_ROUTER_PID_KEY;
use net::runtime::Runtime;
use net::Session;
pub use net::{ZError, ZErrorKind, ZResult};

mod workspace;
pub use workspace::*;

mod path;
pub use path::{path, Path};
mod pathexpr;
pub use pathexpr::{pathexpr, PathExpr};
mod selector;
pub use selector::{selector, Selector};
mod values;
pub use values::*;

// pub mod config;
pub mod utils;

pub use net::protocol::core::{Timestamp, TimestampId};
pub use zenoh_util::properties::config::ConfigProperties;
pub use zenoh_util::properties::Properties;

/// The zenoh client API.
pub struct Zenoh {
    session: Session,
}

impl Zenoh {
    /// Creates a zenoh API, establishing a zenoh-net session with discovered peers and/or routers.
    ///
    /// # Arguments
    ///
    /// * `config` - The [ConfigProperties](net::config::ConfigProperties) for the zenoh session
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let zenoh = Zenoh::new(net::config::default()).await.unwrap();
    /// # })
    /// ```
    ///
    /// # Configuration Properties
    ///
    /// [ConfigProperties](net::config::ConfigProperties) are a set of key/value (`u64`/`String`) pairs.
    /// Constants for the accepted keys can be found in the [config](net::config) module.
    /// Multiple values are coma separated.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let mut config = net::config::peer();
    /// config.insert(net::config::ZN_LOCAL_ROUTING_KEY, "false".to_string());
    /// config.insert(net::config::ZN_PEER_KEY, "tcp/10.10.10.10:7447,tcp/11.11.11.11:7447".to_string());
    ///
    /// let zenoh = Zenoh::new(config).await.unwrap();
    /// # })
    /// ```
    ///
    /// [ConfigProperties](net::config::ConfigProperties) can be built set of key/value (`String`/`String`) set
    /// of [Properties](Properties).
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
    /// let zenoh = Zenoh::new(config.into()).await.unwrap();
    /// # })
    /// ```
    pub async fn new(config: ConfigProperties) -> ZResult<Zenoh> {
        Ok(Zenoh {
            session: net::open(config).await?,
        })
    }

    /// Creates a Zenoh API with an existing Runtime.
    /// This operation is used by the plugins to share the same Runtime than the router.
    #[doc(hidden)]
    pub async fn init(runtime: Runtime) -> Zenoh {
        Zenoh {
            session: Session::init(runtime, true, vec![], vec![]).await,
        }
    }

    /// Returns the zenoh-net [Session](net::Session) used by this zenoh session.
    /// This is for advanced use cases requiring fine usage of the zenoh-net API.
    #[inline(always)]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Returns the PeerId of the zenoh router this zenoh API is connected to (if any).
    /// This calls [Session::info()](net::Session::info) and returns the first router pid from
    /// the ZN_INFO_ROUTER_PID_KEY property.
    pub async fn router_pid(&self) -> Option<String> {
        match self.session().info().await.remove(&ZN_INFO_ROUTER_PID_KEY) {
            None => None,
            Some(s) if s.is_empty() => None,
            Some(s) if !s.contains(',') => Some(s),
            Some(s) => Some(s.split(',').next().unwrap().to_string()),
        }
    }

    /// Creates a [`Workspace`] with an optional [`Path`] as `prefix`.
    /// All relative [`Path`] or [`Selector`] used with this Workspace will be relative to the
    /// specified prefix. Not specifying a prefix is equivalent to specifying "/" as prefix,
    /// meaning in this case that all relative paths/selectors will be prependend with "/".
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use std::convert::TryInto;
    ///
    /// let zenoh = Zenoh::new(net::config::default()).await.unwrap();
    /// let workspace = zenoh.workspace(Some("/demo/example".try_into().unwrap())).await.unwrap();
    /// // The following it equivalent to a PUT on "/demo/example/hello".
    /// workspace.put(
    ///     &"hello".try_into().unwrap(),
    ///     "Hello World!".into()
    /// ).await.unwrap();
    /// # })
    /// ```
    pub async fn workspace(&self, prefix: Option<Path>) -> ZResult<Workspace<'_>> {
        debug!("New workspace with prefix: {:?}", prefix);
        Workspace::new(&self, prefix).await
    }

    /// Closes the zenoh API and the associated zenoh-net session.
    ///
    /// Note that on drop, the zenoh-net session is also automatically closed.
    /// But you may want to use this function to handle errors or
    /// close the session synchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let zenoh = Zenoh::new(net::config::default()).await.unwrap();
    /// zenoh.close();
    /// # })
    /// ```
    pub async fn close(self) -> ZResult<()> {
        self.session.close().await
    }
}

impl From<Session> for Zenoh {
    fn from(session: Session) -> Self {
        Zenoh { session }
    }
}

impl From<&Session> for Zenoh {
    fn from(s: &Session) -> Self {
        Zenoh { session: s.clone() }
    }
}
