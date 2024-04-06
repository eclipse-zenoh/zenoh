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

mod api;
mod net;

use git_version::git_version;
#[cfg(feature = "unstable")]
use prelude::*;
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

pub use crate::api::session::open;

/// A collection of useful buffers used by zenoh internally and exposed to the user to facilitate
/// reading and writing data.
pub mod buffers {
    pub use zenoh_buffers::buffer::SplitBuffer;
    pub use zenoh_buffers::{ZBuf, ZSlice};
}

pub mod key_expr {
    pub mod keyexpr_tree {
        pub use zenoh_keyexpr::keyexpr_tree::impls::KeyedSetProvider;
        pub use zenoh_keyexpr::keyexpr_tree::{
            support::NonWild, support::UnknownWildness, KeBoxTree,
        };
        pub use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut};
    }
    pub use crate::api::key_expr::KeyExpr;
    pub use zenoh_keyexpr::keyexpr;
    pub use zenoh_keyexpr::OwnedKeyExpr;
    pub use zenoh_macros::{kedefine, keformat, kewrite};
    // keyexpr format macro support
    pub mod format {
        pub use zenoh_keyexpr::format::*;
        pub mod macro_support {
            pub use zenoh_keyexpr::format::macro_support::*;
        }
    }
}

pub mod session {
    pub use crate::api::builders::publication::SessionDeleteBuilder;
    pub use crate::api::builders::publication::SessionPutBuilder;
    #[zenoh_macros::unstable]
    pub use crate::api::session::init;
    pub use crate::api::session::open;
    pub use crate::api::session::Session;
    pub use crate::api::session::SessionDeclarations;
    pub use crate::api::session::SessionRef;
}

pub mod sample {
    pub use crate::api::builders::sample::QoSBuilderTrait;
    pub use crate::api::builders::sample::SampleBuilder;
    pub use crate::api::builders::sample::SampleBuilderTrait;
    pub use crate::api::builders::sample::TimestampBuilderTrait;
    pub use crate::api::builders::sample::ValueBuilderTrait;
    #[zenoh_macros::unstable]
    pub use crate::api::sample::Attachment;
    #[zenoh_macros::unstable]
    pub use crate::api::sample::Locality;
    pub use crate::api::sample::Sample;
    pub use crate::api::sample::SampleKind;
    #[zenoh_macros::unstable]
    pub use crate::api::sample::SourceInfo;
}

pub mod value {
    pub use crate::api::value::Value;
}

pub mod encoding {
    pub use crate::api::encoding::Encoding;
}

pub mod payload {
    pub use crate::api::payload::Deserialize;
    pub use crate::api::payload::Payload;
    pub use crate::api::payload::PayloadReader;
    pub use crate::api::payload::Serialize;
    pub use crate::api::payload::StringOrBase64;
}

pub mod selector {
    pub use crate::api::selector::Parameter;
    pub use crate::api::selector::Parameters;
    pub use crate::api::selector::Selector;
    pub use crate::api::selector::TIME_RANGE_KEY;
}

pub mod subscriber {
    pub use crate::api::subscriber::FlumeSubscriber;
    pub use crate::api::subscriber::Reliability;
    pub use crate::api::subscriber::Subscriber;
    pub use crate::api::subscriber::SubscriberBuilder;
}

pub mod publication {
    pub use crate::api::builders::publication::PublisherBuilder;
    pub use crate::api::publication::CongestionControl;
    pub use crate::api::publication::Priority;
    pub use crate::api::publication::Publisher;
    #[zenoh_macros::unstable]
    pub use crate::api::publication::PublisherDeclarations;
}

pub mod query {
    pub use crate::api::query::Mode;
    pub use crate::api::query::Reply;
    #[zenoh_macros::unstable]
    pub use crate::api::query::ReplyKeyExpr;
    #[zenoh_macros::unstable]
    pub use crate::api::query::REPLY_KEY_EXPR_ANY_SEL_PARAM;
    pub use crate::api::query::{ConsolidationMode, QueryConsolidation, QueryTarget};
}

pub mod queryable {
    pub use crate::api::queryable::Query;
    pub use crate::api::queryable::Queryable;
    pub use crate::api::queryable::QueryableBuilder;
}

pub mod handlers {
    pub use crate::api::handlers::locked;
    pub use crate::api::handlers::DefaultHandler;
    pub use crate::api::handlers::IntoHandler;
    pub use crate::api::handlers::RingBuffer;
}

pub mod scouting {
    pub use crate::api::scouting::scout;
    pub use crate::api::scouting::ScoutBuilder;
    pub use crate::api::scouting::WhatAmI;
}

#[cfg(feature = "unstable")]
pub mod liveliness {
    pub use crate::api::liveliness::Liveliness;
    pub use crate::api::liveliness::LivelinessSubscriberBuilder;
}

pub mod time {
    pub use crate::api::time::new_reception_timestamp;
    pub use zenoh_protocol::core::{Timestamp, TimestampId, NTP64};
}

pub mod runtime {
    pub use crate::net::runtime::{AdminSpace, Runtime};
}

pub mod config {
    pub use zenoh_config::{
        client, default, peer, Config, EndPoint, Locator, ModeDependentValue, PermissionsConf,
        PluginLoad, ValidatedMap, ZenohId,
    };
}

pub mod plugins {
    pub use crate::api::plugins::PluginsManager;
    pub use crate::api::plugins::Response;
    pub use crate::api::plugins::RunningPlugin;
    pub use crate::api::plugins::{RunningPluginTrait, ZenohPlugin};
}

#[cfg(feature = "shared-memory")]
pub mod shm {
    pub use zenoh_shm::SharedMemoryManager;
}

pub mod prelude;
