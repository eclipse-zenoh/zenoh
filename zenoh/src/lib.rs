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

use git_version::git_version;
use zenoh_util::concat_enabled_features;

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
mod api;
mod net;
mod plugins;

pub mod prelude;
// Reexport useful types from external zenoh crates into root namespace
// pub use prelude::common::*;

//
// Explicitly define zenoh API split to logical modules
//

pub mod core {
    pub use zenoh_core::{zlock, ztimeout, AsyncResolve, Result, SyncResolve};
    pub use zenoh_result::bail;
}

pub mod session {
    pub use crate::api::session::{open, Session, SessionDeclarations};
}

pub mod key_expr {
    pub use crate::api::key_expr::KeyExpr;
    pub use zenoh_keyexpr::key_expr::format::KeFormat;
    pub use zenoh_keyexpr::keyexpr;
    pub use zenoh_keyexpr::OwnedKeyExpr;
    pub use zenoh_macros::{kedefine, keformat};
}

pub mod sample {
    pub use crate::api::{
        sample::{
            builder::{
                QoSBuilderTrait, SampleBuilderTrait, TimestampBuilderTrait, ValueBuilderTrait,
            },
            Sample, SampleKind,
        },
        value::Value,
    };

    #[zenoh_macros::unstable]
    pub use crate::api::sample::attachment::Attachment;
    #[zenoh_macros::unstable]
    pub use crate::api::sample::Locality;
    #[zenoh_macros::unstable]
    pub use crate::api::sample::SourceInfo;

    pub use zenoh_protocol::core::CongestionControl;
}

pub mod queryable {
    pub use crate::api::queryable::Query;
}

pub mod query {
    pub use crate::api::query::Reply;
}

pub mod publication {
    pub use crate::api::publication::Priority;
}

pub mod handlers {
    pub use crate::api::handlers::IntoHandler;
    pub use crate::api::handlers::RingBuffer;
}

pub mod config {
    pub use zenoh_config::{
        client, default, peer, Config, ConnectionRetryConf, EndPoint, Locator, ModeDependentValue,
        ValidatedMap, WhatAmI, WhatAmIMatcher,
    };
}
