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
#[cfg(feature = "unstable")]
use prelude::*;
pub use zenoh_macros::{ke, kedefine, keformat, kewrite};
use zenoh_result::ZResult;
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

mod admin;
#[macro_use]
mod session;
pub use session::*;

pub mod key_expr;
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
pub mod time;

/// Scouting primitives.
pub mod scouting;

pub use scouting::scout;
pub use session::open;
