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
//! use zenoh::prelude::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).await.unwrap();
//!     session.put("key/expression", "value").await.unwrap();
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
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).await.unwrap();
//!     let subscriber = session.declare_subscriber("key/expression").await.unwrap();
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
//! use zenoh::prelude::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(config::default()).await.unwrap();
//!     let replies = session.get("key/expression").await.unwrap();
//!     while let Ok(reply) = replies.recv_async().await {
//!         println!(">> Received {:?}", reply.result());
//!     }
//! }
//! ```
#[macro_use]
extern crate zenoh_core;
#[macro_use]
extern crate zenoh_result;

mod api;
mod net;

lazy_static::lazy_static!(
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
);

const GIT_VERSION: &str = git_version::git_version!(prefix = "v", cargo_prefix = "v");
pub const FEATURES: &str = zenoh_util::concat_enabled_features!(
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

// Expose some functions directly to root `zenoh::`` namespace for convenience
pub use crate::api::scouting::scout;
pub use crate::api::session::open;

pub mod prelude;

/// Zenoh core types
pub mod core {
    #[allow(deprecated)]
    pub use zenoh_core::AsyncResolve;
    pub use zenoh_core::Resolvable;
    pub use zenoh_core::Resolve;
    #[allow(deprecated)]
    pub use zenoh_core::SyncResolve;
    pub use zenoh_core::Wait;
    /// A zenoh error.
    pub use zenoh_result::Error;
    /// A zenoh result.
    pub use zenoh_result::ZResult as Result;
    pub use zenoh_util::core::zresult::ErrNo;
    pub use zenoh_util::try_init_log_from_env;
}

/// A collection of useful buffers used by zenoh internally and exposed to the user to facilitate
/// reading and writing data.
pub mod buffers {
    pub use zenoh_buffers::buffer::SplitBuffer;
    pub use zenoh_buffers::reader::HasReader;
    pub use zenoh_buffers::reader::Reader;
    pub use zenoh_buffers::ZBufReader;
    pub use zenoh_buffers::{ZBuf, ZSlice, ZSliceBuffer};
}

/// [Key expression](https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Key%20Expressions.md) are Zenoh's address space.
///
/// In Zenoh, operations are performed on keys. To allow addressing multiple keys with a single operation, we use Key Expressions (KE).
/// KEs are a small language that express sets of keys through a glob-like language.
///
/// These semantics can be a bit difficult to implement, so this module provides the following facilities:
///
/// # Storing Key Expressions
/// This module provides 3 flavours to store strings that have been validated to respect the KE syntax:
/// - [`keyexpr`] is the equivalent of a [`str`],
/// - [`OwnedKeyExpr`] works like an [`std::sync::Arc<str>`],
/// - [`KeyExpr`] works like a [`std::borrow::Cow<str>`], but also stores some additional context internal to Zenoh to optimize
/// routing and network usage.
///
/// All of these types [`Deref`](core::ops::Deref) to [`keyexpr`], which notably has methods to check whether a given [`keyexpr::intersects`] with another,
/// or even if a [`keyexpr::includes`] another.
///
/// # Tying values to Key Expressions
/// When storing values tied to Key Expressions, you might want something more specialized than a [`HashMap`](std::collections::HashMap) if you want to respect
/// the Key Expression semantics with high performance.
///
/// Enter [KeTrees](keyexpr_tree). These are data-structures specially built to store KE-value pairs in a manner that supports the set-semantics of KEs.
///
/// # Building and parsing Key Expressions
/// A common issue in REST API is the association of meaning to sections of the URL, and respecting that API in a convenient manner.
/// The same issue arises naturally when designing a KE space, and [`KeFormat`](format::KeFormat) was designed to help you with this,
/// both in constructing and in parsing KEs that fit the formats you've defined.
///
/// [`kedefine`] also allows you to define formats at compile time, allowing a more performant, but more importantly safer and more convenient use of said formats,
/// as the [`keformat`] and [`kewrite`] macros will be able to tell you if you're attempting to set fields of the format that do not exist.
pub mod key_expr {
    pub mod keyexpr_tree {
        pub use zenoh_keyexpr::keyexpr_tree::impls::KeyedSetProvider;
        pub use zenoh_keyexpr::keyexpr_tree::{
            support::NonWild, support::UnknownWildness, KeBoxTree,
        };
        pub use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut};
    }
    pub use crate::api::key_expr::KeyExpr;
    pub use crate::api::key_expr::KeyExprUndeclaration;
    pub use zenoh_keyexpr::keyexpr;
    pub use zenoh_keyexpr::OwnedKeyExpr;
    pub use zenoh_keyexpr::SetIntersectionLevel;
    pub use zenoh_macros::{kedefine, keformat, kewrite};
    // keyexpr format macro support
    pub mod format {
        pub use zenoh_keyexpr::format::*;
        pub mod macro_support {
            pub use zenoh_keyexpr::format::macro_support::*;
        }
    }
}

/// Zenoh [`Session`](crate::session::Session) and associated types
pub mod session {
    pub use crate::api::builders::publication::SessionDeleteBuilder;
    pub use crate::api::builders::publication::SessionPutBuilder;
    #[zenoh_macros::unstable]
    #[doc(hidden)]
    pub use crate::api::session::init;
    pub use crate::api::session::open;
    #[zenoh_macros::unstable]
    #[doc(hidden)]
    pub use crate::api::session::InitBuilder;
    pub use crate::api::session::OpenBuilder;
    pub use crate::api::session::Session;
    pub use crate::api::session::SessionDeclarations;
    pub use crate::api::session::SessionRef;
    pub use crate::api::session::Undeclarable;
}

/// Tools to access information about the current zenoh [`Session`](crate::Session).
pub mod info {
    pub use crate::api::info::PeersZenohIdBuilder;
    pub use crate::api::info::RoutersZenohIdBuilder;
    pub use crate::api::info::SessionInfo;
    pub use crate::api::info::ZenohIdBuilder;
}

/// Sample primitives
pub mod sample {
    pub use crate::api::builders::sample::QoSBuilderTrait;
    pub use crate::api::builders::sample::SampleBuilder;
    pub use crate::api::builders::sample::SampleBuilderAny;
    pub use crate::api::builders::sample::SampleBuilderDelete;
    pub use crate::api::builders::sample::SampleBuilderPut;
    pub use crate::api::builders::sample::SampleBuilderTrait;
    pub use crate::api::builders::sample::TimestampBuilderTrait;
    pub use crate::api::builders::sample::ValueBuilderTrait;
    #[zenoh_macros::unstable]
    pub use crate::api::sample::Locality;
    pub use crate::api::sample::Sample;
    pub use crate::api::sample::SampleFields;
    pub use crate::api::sample::SampleKind;
    #[zenoh_macros::unstable]
    pub use crate::api::sample::SourceInfo;
    pub use crate::api::sample::SourceSn;
}

/// Value primitives
pub mod value {
    pub use crate::api::value::Value;
}

/// Encoding support
pub mod encoding {
    pub use crate::api::encoding::Encoding;
}

/// Payload primitives
pub mod bytes {
    pub use crate::api::bytes::Deserialize;
    pub use crate::api::bytes::OptionZBytes;
    pub use crate::api::bytes::Serialize;
    pub use crate::api::bytes::StringOrBase64;
    pub use crate::api::bytes::ZBytes;
    pub use crate::api::bytes::ZBytesIterator;
    pub use crate::api::bytes::ZBytesReader;
    pub use crate::api::bytes::ZBytesWriter;
    pub use crate::api::bytes::ZDeserializeError;
    pub use crate::api::bytes::ZSerde;
}

/// [Selector](https://github.com/eclipse-zenoh/roadmap/tree/main/rfcs/ALL/Selectors) to issue queries
pub mod selector {
    pub use crate::api::selector::Parameters;
    pub use crate::api::selector::Selector;
    pub use crate::api::selector::TIME_RANGE_KEY;
    pub use zenoh_protocol::core::Properties;
    pub use zenoh_util::time_range::{TimeBound, TimeExpr, TimeRange};
}

/// Subscribing primitives
pub mod subscriber {
    pub use crate::api::subscriber::FlumeSubscriber;
    pub use crate::api::subscriber::Subscriber;
    pub use crate::api::subscriber::SubscriberBuilder;
    /// The kind of reliability.
    pub use zenoh_protocol::core::Reliability;
}

/// Publishing primitives
pub mod publication {
    pub use crate::api::builders::publication::PublicationBuilderDelete;
    pub use crate::api::builders::publication::PublicationBuilderPut;
    pub use crate::api::builders::publication::PublisherBuilder;
    pub use crate::api::builders::publication::PublisherDeleteBuilder;
    #[zenoh_macros::unstable]
    pub use crate::api::publication::MatchingListener;
    #[zenoh_macros::unstable]
    pub use crate::api::publication::MatchingListenerBuilder;
    #[zenoh_macros::unstable]
    pub use crate::api::publication::MatchingListenerUndeclaration;
    #[zenoh_macros::unstable]
    pub use crate::api::publication::MatchingStatus;
    pub use crate::api::publication::Priority;
    pub use crate::api::publication::Publisher;
    #[zenoh_macros::unstable]
    pub use crate::api::publication::PublisherDeclarations;
    #[zenoh_macros::unstable]
    pub use crate::api::publication::PublisherRef;
    pub use crate::api::publication::PublisherUndeclaration;
    pub use zenoh_protocol::core::CongestionControl;
}

/// Query primitives
pub mod query {
    pub use crate::api::query::GetBuilder;
    pub use crate::api::query::Reply;
    #[zenoh_macros::unstable]
    pub use crate::api::query::ReplyKeyExpr;
    #[zenoh_macros::unstable]
    pub use crate::api::query::REPLY_KEY_EXPR_ANY_SEL_PARAM;
    pub use crate::api::query::{ConsolidationMode, QueryConsolidation, QueryTarget};
}

/// Queryable primitives
pub mod queryable {
    pub use crate::api::queryable::Query;
    pub use crate::api::queryable::Queryable;
    pub use crate::api::queryable::QueryableBuilder;
    pub use crate::api::queryable::QueryableUndeclaration;
    pub use crate::api::queryable::ReplyBuilder;
    pub use crate::api::queryable::ReplyBuilderDelete;
    pub use crate::api::queryable::ReplyBuilderPut;
    pub use crate::api::queryable::ReplyErrBuilder;
    #[zenoh_macros::unstable]
    pub use crate::api::queryable::ReplySample;
}

/// Callback handler trait
pub mod handlers {
    pub use crate::api::handlers::locked;
    pub use crate::api::handlers::Callback;
    pub use crate::api::handlers::CallbackDrop;
    pub use crate::api::handlers::DefaultHandler;
    pub use crate::api::handlers::FifoChannel;
    pub use crate::api::handlers::IntoHandler;
    pub use crate::api::handlers::RingChannel;
    pub use crate::api::handlers::RingChannelHandler;
}

/// Scouting primitives
pub mod scouting {
    pub use crate::api::scouting::scout;
    pub use crate::api::scouting::Scout;
    pub use crate::api::scouting::ScoutBuilder;
    /// Constants and helpers for zenoh `whatami` flags.
    pub use zenoh_protocol::core::WhatAmI;
    /// A zenoh Hello message.
    pub use zenoh_protocol::scouting::Hello;
}

/// Liveliness primitives
#[cfg(feature = "unstable")]
pub mod liveliness {
    pub use crate::api::liveliness::Liveliness;
    pub use crate::api::liveliness::LivelinessGetBuilder;
    pub use crate::api::liveliness::LivelinessSubscriberBuilder;
    pub use crate::api::liveliness::LivelinessToken;
    pub use crate::api::liveliness::LivelinessTokenBuilder;
    pub use crate::api::liveliness::LivelinessTokenUndeclaration;
}

/// Timestamp support
pub mod time {
    pub use crate::api::time::new_reception_timestamp;
    pub use zenoh_protocol::core::{Timestamp, TimestampId, NTP64};
}

/// Initialize a Session with an existing Runtime.
/// This operation is used by the plugins to share the same Runtime as the router.
#[doc(hidden)]
pub mod runtime {
    pub use crate::net::runtime::RuntimeBuilder;
    pub use crate::net::runtime::{AdminSpace, Runtime};
    pub use zenoh_runtime::ZRuntime;
}

/// Configuration to pass to [`open`](crate::session::open) and [`scout`](crate::scouting::scout) functions and associated constants
pub mod config {
    // pub use zenoh_config::{
    //     client, default, peer, Config, EndPoint, Locator, ModeDependentValue, PermissionsConf,
    //     PluginLoad, ValidatedMap, ZenohId,
    // };
    pub use zenoh_config::*;
}

#[doc(hidden)]
#[cfg(all(feature = "unstable", feature = "plugins"))]
pub mod plugins {
    pub use crate::api::plugins::PluginsManager;
    pub use crate::api::plugins::Response;
    pub use crate::api::plugins::RunningPlugin;
    pub use crate::api::plugins::PLUGIN_PREFIX;
    pub use crate::api::plugins::{RunningPluginTrait, ZenohPlugin};
}

#[doc(hidden)]
pub mod internal {
    pub use zenoh_core::zasync_executor_init;
    pub use zenoh_core::zerror;
    pub use zenoh_core::zlock;
    pub use zenoh_core::ztimeout;
    pub use zenoh_result::bail;
    pub use zenoh_sync::Condition;
    pub use zenoh_task::TaskController;
    pub use zenoh_task::TerminatableTask;
    pub use zenoh_util::core::ResolveFuture;
    pub use zenoh_util::LibLoader;
    pub use zenoh_util::{zenoh_home, Timed, TimedEvent, Timer, ZENOH_HOME_ENV_VAR};
}

#[cfg(all(feature = "unstable", feature = "shared-memory"))]
pub mod shm {
    pub use zenoh_shm::api::client_storage::SharedMemoryClientStorage;
    pub use zenoh_shm::api::provider::shared_memory_provider::{BlockOn, GarbageCollect};
    pub use zenoh_shm::api::provider::shared_memory_provider::{Deallocate, Defragment};
    pub use zenoh_shm::api::provider::types::AllocAlignment;
    pub use zenoh_shm::api::provider::types::MemoryLayout;
    pub use zenoh_shm::api::slice::zsliceshm::{zsliceshm, ZSliceShm};
    pub use zenoh_shm::api::slice::zsliceshmmut::{zsliceshmmut, ZSliceShmMut};
    pub use zenoh_shm::api::{
        protocol_implementations::posix::{
            posix_shared_memory_provider_backend::PosixSharedMemoryProviderBackend,
            protocol_id::POSIX_PROTOCOL_ID,
        },
        provider::shared_memory_provider::SharedMemoryProviderBuilder,
    };
}
