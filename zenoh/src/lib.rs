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

#![cfg_attr(doc_auto_cfg, feature(doc_auto_cfg))]

//! [Zenoh](https://zenoh.io) /zeno/ is a stack that unifies data in motion, data at
//! rest, and computations. It elegantly blends traditional pub/sub with geo-distributed
//! storage, queries, and computations, while retaining a level of time and space efficiency
//! that is well beyond any of the mainstream stacks.
//!
//! # Components and concepts
//!
//! The main Zenoh components and concepts are described below.
//!  
//! ## Session
//! 
//! Each Zenoh node is a [Session](crate::session) object. This is the main object that maintains the state of
//! the connection to the Zenoh network and is used to declare publishers, subscribers, queriers, queryables, etc.
//! A session is created by the [open](crate::open) function, which takes a [Config](crate::config) object as an argument.
//!
//! The Zenoh protocol allows nodes to form a graph with an arbitrary topology, such as a mesh, a star, or a clique.
//! Data can be sent directly between nodes or routed through intermediate nodes.
//!
//! Zenoh supports two paradigms of communication: publish/subscribe and query/reply.
//!
//! ## Publish/Subscribe
//! 
//! In the publish/subscribe paradigm, data is produced by [Publishers](crate::pubsub::Publisher) 
//! and consumed by [Subscribers](crate::pubsub::Subscriber). See the [pubsub](crate::pubsub) API for details.
//!
//! ## Query/Reply
//! 
//! In the query/reply paradigm, data is made available by [Queryables](crate::query::Queryable) 
//! and requested by [Queriers](crate::query::Querier) or directly via [Session::get](crate::Session::get) operations.
//! More details are available in the [query](crate::query) API.
//!
//! ## Key Expressions
//! 
//! Data is associated with keys in the format of a slash-separated path, e.g., `robot/sensor/temp`.
//! The requesting side uses [key expressions](crate::key_expr) to address the data of interest. Key expressions can
//! contain wildcards, e.g., `robot/sensor/*` or `robot/**`.
//!
//! ## Data representation
//! 
//! Data is received as [Samples](crate::sample), which contain the payload and all metadata associated with the data.
//! The raw byte payload is represented as [ZBytes](crate::bytes), which provides mechanisms for zero-copy creation and access.
//! The [zenoh_ext](https://docs.rs/zenoh-ext/latest/zenoh_ext) crate also provides serialization and deserialization 
//! of basic types and structures for `ZBytes`.
//!
//! ## Other components
//! 
//! Other important functionalities of Zenoh are:
//! - [Scouting](crate::scouting) to discover Zenoh nodes in the network. Note that it's not necessary to explicitly 
//!   discover other nodes just to publish, subscribe, or query data.
//! - Monitor [liveliness](crate::liveliness) to get notified when a specified resource appears or disappears in the network.
//! - The [matching](crate::matching) API allows the active side of communication (publisher, querier) to know whether 
//!   there are any interested parties on the other side (subscriber, queryable) which allows to save bandwidth and CPU resources.
//!
//! ## Builders
//! 
//! Zenoh extensively uses the builder pattern. For example, to create a publisher, you first create a
//! [PublisherBuilder](crate::pubsub::PublisherBuilder) 
//! using the [declare_publisher](crate::session::Session::declare_publisher) method. The builder is
//! resolved to the [Publisher](crate::pubsub::Publisher) instance by awaiting it in an async context
//! or by calling the [wait](crate::Wait::wait) method in a synchronous context.
//!
//! ## Channels and callbacks
//! 
//! There are two ways to get sequential data from Zenoh primitives (e.g., a series of
//! [Sample](crate::sample::Sample)s from a [Subscriber](crate::pubsub::Subscriber)
//! or [Reply](crate::query::Reply)s from a [Query](crate::query::Query)): by channel or by callback.
//! 
//! In channel mode, methods like [recv_async](crate::handlers::fifo::FifoChannelHandler::recv_async)
//! become available on the subscriber or query object (through Deref coercion to the corresponding channel
//! handler type). By default, the [FifoChannel](crate::handlers::fifo::FifoChannel) is used.
//! 
//! The builders provide methods [with](crate::pubsub::SubscriberBuilder::with) to assign an arbitrary channel instead of
//! the default one, and [callback](crate::pubsub::SubscriberBuilder::callback) to assign a callback function.
//!
//! See more details in the [handlers](crate::handlers) module documentation.
//! 
//! # Usage examples
//! 
//! Below are basic examples of using Zenoh. More examples are available in the documentation for each module and in 
//! [zenoh-examples](https://github.com/zenoh-io/zenoh/tree/main/examples).
//! 
//! ## Publishing/Subscribing
//! The example below shows how to publish and subscribe to data using Zenoh.
//! 
//! Publishing data:
//! ```no_run
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(zenoh::Config::default()).await.unwrap();
//!     session.put("key/expression", "value").await.unwrap();
//!     session.close().await.unwrap();
//! }
//! ```
//! 
//! Subscribing to data:
//! ```no_run
//! use futures::prelude::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(zenoh::Config::default()).await.unwrap();
//!     let subscriber = session.declare_subscriber("key/expression").await.unwrap();
//!     while let Ok(sample) = subscriber.recv_async().await {
//!         println!("Received: {:?}", sample);
//!     };
//! }
//! ```
//!
//! ## Query/Reply
//! 
//! Declare a queryable:
//! ```no_run
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(zenoh::Config::default()).await.unwrap();
//!     let queryable = session.declare_queryable("key/expression").await.unwrap();
//!     while let Ok(query) = queryable.recv_async().await {
//!         let reply = query.reply("key/expression", "value").await.unwrap();
//!     }
//! }
//! ```
//!
//! Request data:
//! ```no_run
//! use futures::prelude::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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

#[cfg(feature = "internal")]
pub use api::admin::KE_ADV_PREFIX;
#[cfg(feature = "internal")]
pub use api::admin::KE_AT;
#[cfg(feature = "internal")]
pub use api::admin::KE_EMPTY;
#[cfg(feature = "internal")]
pub use api::admin::KE_PUB;
#[cfg(feature = "internal")]
pub use api::admin::KE_STAR;
#[cfg(feature = "internal")]
pub use api::admin::KE_STARSTAR;
#[cfg(feature = "internal")]
pub use api::admin::KE_SUB;

lazy_static::lazy_static!(
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
);

const GIT_VERSION: &str = git_version::git_version!(prefix = "v", cargo_prefix = "v");
#[doc(hidden)]
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

pub use zenoh_core::{Resolvable, Resolve, Wait};
/// A Zenoh error.
pub use zenoh_result::Error;
/// A Zenoh result.
pub use zenoh_result::ZResult as Result;
#[doc(inline)]
pub use zenoh_util::{init_log_from_env_or, try_init_log_from_env};

#[doc(inline)]
pub use crate::{
    config::Config,
    scouting::scout,
    session::{open, Session},
};

/// # Key Expressions
///
/// [Key expressions](https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Key%20Expressions.md) are Zenoh's address space.
///
/// In Zenoh, operations are performed on keys. To allow addressing multiple keys with a single operation, Zenoh uses Key Expressions (KEs).
/// KEs are a small language that expresses sets of keys through a glob-like syntax.
///
/// These semantics can be a bit difficult to implement, so this module provides the following facilities:
///
/// # Storing Key Expressions
/// This module provides three ways to store strings that have been validated to respect the KE syntax:
/// - [`keyexpr`](crate::key_expr::keyexpr) is the equivalent of a [`str`],
/// - [`OwnedKeyExpr`](crate::key_expr::OwnedKeyExpr) works like an [`std::sync::Arc<str>`],
/// - [`KeyExpr`](crate::key_expr::KeyExpr) works like a [`std::borrow::Cow<str>`], but also stores some additional context internal to Zenoh to optimize
///   routing and network usage.
///
/// All of these types implement [`Deref`](std::ops::Deref) to [`keyexpr`](crate::key_expr::keyexpr), which notably has methods to check whether a given key expression
/// [`intersects`](crate::key_expr::keyexpr::intersects) with another, or whether it [`includes`](crate::key_expr::keyexpr::includes) another.
///
/// # Tying values to Key Expressions
/// When storing values tied to Key Expressions, you might want something more specialized than a [`HashMap`](std::collections::HashMap) to respect
/// Key Expression semantics with high performance.
///
/// Enter [KeTrees](crate::key_expr::keyexpr_tree). These are data structures built to store KE–value pairs in a manner that supports the set semantics of KEs.
///
/// # Building and parsing Key Expressions
/// A common issue in REST APIs is assigning meaning to sections of the URL and respecting that API in a convenient manner.
/// The same issue arises naturally when designing a KE space, and [`KeFormat`](crate::key_expr::format::KeFormat) was designed to help with this,
/// both in constructing and parsing KEs that fit the formats you've defined.
///
/// [`kedefine`](crate::key_expr::format::kedefine) also lets you define formats at compile time, enabling a more performant—and, more importantly, safer and more convenient—use of said formats,
/// as the [`keformat`](crate::key_expr::format::keformat) and [`kewrite`](crate::key_expr::format::kewrite) macros will tell you if you're attempting to set fields of the format that do not exist.
pub mod key_expr {
    #[zenoh_macros::unstable]
    pub mod keyexpr_tree {
        pub use zenoh_keyexpr::keyexpr_tree::{
            impls::KeyedSetProvider,
            support::{NonWild, UnknownWildness},
            IKeyExprTree, IKeyExprTreeMut, KeBoxTree,
        };
    }
    #[zenoh_macros::unstable]
    pub use zenoh_keyexpr::SetIntersectionLevel;
    pub use zenoh_keyexpr::{
        canon::Canonize, keyexpr, nonwild_keyexpr, OwnedKeyExpr, OwnedNonWildKeyExpr,
    };

    pub use crate::api::key_expr::{KeyExpr, KeyExprUndeclaration};
    // keyexpr format macro support
    #[zenoh_macros::unstable]
    pub mod format {
        pub use zenoh_keyexpr::format::*;
        pub use zenoh_macros::{ke, kedefine, keformat, kewrite};
        pub mod macro_support {
            pub use zenoh_keyexpr::format::macro_support::*;
        }
    }
}

/// # Zenoh [`Session`] and associated types
///
/// The [`Session`] is the main component of Zenoh.
///
/// Each Zenoh node is represented by a [Session](crate::session::Session) object. This object
/// maintains the state of the connection to the Zenoh network and is used to declare publishers,
/// subscribers, queriers, queryables, etc.
///
/// All session parameters are specified in the [Config](crate::config) object passed to the
/// [open](crate::open) function.
///
/// Objects created by the session (for example, a [Publisher](crate::pubsub::Publisher) via
/// [declare_publisher](crate::session::Session::declare_publisher) or a
/// [Subscriber](crate::pubsub::Subscriber) via
/// [declare_subscriber](crate::session::Session::declare_subscriber)) have lifetimes independent
/// of the session, but they stop functioning when the session is closed or dropped.
///
/// Use the explicit [close](crate::session::Session::close) method to close the session and all
/// associated objects.
///
/// Because the session actually processes all instances of these objects, it is sometimes
/// convenient not to keep a reference to an object (for example, a [Queryable]) solely to keep it
/// alive, and instead run it in the background until the session is closed. To do this, create the
/// object with the [background](crate::query::QueryableBuilder::background) method on the
/// corresponding builder. This causes the builder to return `()` instead of the object instance and
/// keeps the instance alive while the session is alive.
///
pub mod session {
    #[zenoh_macros::unstable]
    pub use zenoh_config::wrappers::EntityGlobalId;
    pub use zenoh_config::wrappers::ZenohId;
    #[zenoh_macros::unstable]
    pub use zenoh_protocol::core::EntityId;

    #[zenoh_macros::internal]
    pub use crate::api::builders::session::{init, InitBuilder};
    pub use crate::api::{
        builders::{
            close::CloseBuilder,
            info::{PeersZenohIdBuilder, RoutersZenohIdBuilder, ZenohIdBuilder},
            publisher::{SessionDeleteBuilder, SessionPutBuilder},
            query::SessionGetBuilder,
            session::OpenBuilder,
        },
        info::SessionInfo,
        session::{open, Session, SessionClosedError, Undeclarable},
    };
}

/// # Sample primitives
///
/// The [`Sample`](crate::sample::Sample) structure is the data unit received from [`Subscriber`](crate::pubsub::Subscriber)
/// or [`Queryable`](crate::query::Queryable) instances. It contains the payload and all metadata associated with the data.
pub mod sample {
    #[zenoh_macros::unstable]
    pub use crate::api::sample::{SourceInfo, SourceSn};
    pub use crate::api::{
        builders::sample::{
            SampleBuilder, SampleBuilderAny, SampleBuilderDelete, SampleBuilderPut,
        },
        sample::{Locality, Sample, SampleFields, SampleKind},
    };
}

/// # Payload primitives
///
/// The [`ZBytes`](crate::bytes::ZBytes) type is Zenoh's representation of raw byte data.
/// It provides mechanisms for zero-copy creation and access (`From<Vec<u8>>` and
/// [`ZBytes::slices`](crate::bytes::ZBytes::slices)), as well as methods for sequential
/// reading/writing ([`ZBytes::reader`](crate::bytes::ZBytes::reader), [`ZBytes::writer`](crate::bytes::ZBytes::writer)).
///
/// The `zenoh_ext` crate provides serialization and deserialization of basic types and structures for `ZBytes` via
/// [`z_serialize`](../../zenoh_ext/fn.z_serialize.html) and
/// [`z_deserialize`](../../zenoh_ext/fn.z_deserialize.html).
/// 
/// # Examples
/// 
/// ### Creating ZBytes
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// let zbytes = zenoh::bytes::ZBytes::from("Hello, world!");
/// # assert_eq!(zbytes.try_to_string().unwrap(), "Hello, world!");
/// # }
/// ```
/// 
/// ### Converting ZBytes to String
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// # let zbytes = zenoh::bytes::ZBytes::from("Hello, world!");
/// let s = zbytes.try_to_string().unwrap();
/// assert_eq!(s, "Hello, world!");
/// # }
/// ```
/// 
/// ### Converting ZBytes to Vec<u8>
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// # let zbytes = zenoh::bytes::ZBytes::from("Hello, world!");
/// let vec = zbytes.to_bytes();
/// assert_eq!(vec.as_ref(), b"Hello, world!");
/// # }
/// ```
pub mod bytes {
    pub use crate::api::{
        bytes::{OptionZBytes, ZBytes, ZBytesReader, ZBytesSliceIterator, ZBytesWriter},
        encoding::Encoding,
    };
}

/// # Pub/sub primitives
///
/// This module provides the publish/subscribe API of Zenoh.
///
/// A [`Publisher`](crate::pubsub::Publisher) is declared by the
/// [`Session::declare_publisher`](crate::Session::declare_publisher) method.
///
/// [`Sample`](crate::sample::Sample) data is received by [`Subscriber`](crate::pubsub::Subscriber)s
/// declared with [`Session::declare_subscriber`](crate::Session::declare_subscriber).
///
/// # Examples:
/// ### Declaring a publisher and publishing data
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// # let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let publisher = session.declare_publisher("key/expression").await.unwrap();
/// publisher.put("value").await.unwrap();
/// # }
/// ```
/// 
/// ### Declaring a subscriber and receiving data
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// # let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let subscriber = session.declare_subscriber("key/expression").await.unwrap();
/// while let Ok(sample) = subscriber.recv_async().await {
///     println!(">> Received {}", sample.payload().try_to_string().unwrap());
/// }
/// # }
/// ```
pub mod pubsub {
    pub use crate::api::{
        builders::{
            publisher::{
                PublicationBuilder, PublicationBuilderDelete, PublicationBuilderPut,
                PublisherBuilder, PublisherDeleteBuilder, PublisherPutBuilder,
            },
            subscriber::SubscriberBuilder,
        },
        publisher::{Publisher, PublisherUndeclaration},
        subscriber::Subscriber,
    };
}

/// # Query/reply primitives
///
/// This module provides the query/reply API of Zenoh.
/// 
/// A [`Queryable`](crate::query::Queryable) is declared by the
/// [`Session::declare_queryable`](crate::Session::declare_queryable) method.
/// Data is requested via [`Session::get`](crate::Session::get) function or by
/// [Querier](crate::query::Querier) object. Each request returns zero or more
/// [`Reply`](crate::query::Reply) structures each one from each queryable that matches the request.
/// Each reply contains either the [Sample](crate::sample::Sample)
/// or [`ReplyError`](crate::query::ReplyError) structures.
/// 
/// # Examples:
/// ### Declaring a queryable
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// # let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let queryable = session.declare_queryable("key/expression").await.unwrap();
/// while let Ok(query) = queryable.recv_async().await {
///     let reply = query.reply("key/expression", "value").await.unwrap();
/// }
/// # } 
/// ```
/// 
/// ## Requesting data
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// # let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let replies = session.get("key/expression").await.unwrap();
/// while let Ok(reply) = replies.recv_async().await {
///     match reply.result() {
///         Ok(sample) => {
///             println!(">> Received {}", sample.payload().try_to_string().unwrap());
///         }
///         Err(err) => {
///             println!(">> Error {}", err.payload().try_to_string().unwrap());
///         }
///     }
/// # }
/// # }
/// ```
pub mod query {
    pub use zenoh_protocol::core::Parameters;
    #[zenoh_macros::unstable]
    pub use zenoh_util::time_range::{TimeBound, TimeExpr, TimeRange};

    #[zenoh_macros::internal]
    pub use crate::api::queryable::ReplySample;
    pub use crate::api::{
        builders::{
            querier::{QuerierBuilder, QuerierGetBuilder},
            queryable::QueryableBuilder,
            reply::{ReplyBuilder, ReplyBuilderDelete, ReplyBuilderPut, ReplyErrBuilder},
        },
        querier::Querier,
        query::{ConsolidationMode, QueryConsolidation, QueryTarget, Reply, ReplyError},
        queryable::{Query, Queryable, QueryableUndeclaration},
        selector::Selector,
    };
    #[zenoh_macros::unstable]
    pub use crate::api::{query::ReplyKeyExpr, selector::ZenohParameters};
}

/// # Matching primitives
///
/// The matching API allows the active side of communication (publisher, querier) to know
/// whether there are any interested parties on the other side (subscriber, queryable), which
/// can save bandwidth and CPU resources.
///
/// A [MatchingListener](crate::matching::MatchingListener) can be declared via the
/// [Publisher::matching_listener](crate::pubsub::Publisher::matching_listener) or
/// [Querier::matching_listener](crate::query::Querier::matching_listener) methods.
///
/// The matching listener behaves like a subscriber, but instead of producing data samples it
/// yields [MatchingStatus](crate::matching::MatchingStatus) instances whenever the matching
/// status changes, i.e., when the first matching subscriber or queryable appears, or when the
/// last one disappears.
/// 
/// # Example
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// # let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let publisher = session.declare_publisher("key/expression").await.unwrap();
/// let mut listener = publisher.matching_listener().await.unwrap();
/// while let Ok(status) = listener.recv_async().await {
///     if status.matching() {
///         println!(">> Publisher has at least one matching subscriber");
///     } else {
///         println!(">> Publisher has no matching subscribers");
///     }
/// }
/// # }
/// ```
pub mod matching {
    pub use crate::api::{
        builders::matching_listener::MatchingListenerBuilder,
        matching::{MatchingListener, MatchingListenerUndeclaration, MatchingStatus},
    };
}

/// # Callback handler trait.
///
/// Zenoh primitives that receive data (e.g., [`Subscriber`](crate::pubsub::Subscriber),
/// [`Query`](crate::query::Query), etc.) have a
/// [`with`](crate::pubsub::SubscriberBuilder::with) method that accepts a handler for the data.
///
/// The handler is a pair of a [`Callback`](crate::handlers::Callback) and an arbitrary `Handler`
/// object used to access data received by the callback. When the data is processed by the callback itself,
/// the handler type can be `()`. For convenience, the
/// [`callback`](crate::pubsub::SubscriberBuilder::callback) method, which accepts
/// only an `Fn(T)`, can be used in this case.
///
/// The [`with`](crate::pubsub::SubscriberBuilder::with) method accepts any type that
/// implements the [`IntoHandler`](crate::handlers::IntoHandler) trait, which provides a
/// conversion to a pair of [`Callback`](crate::handlers::Callback) and handler.
///
/// The `IntoHandler` implementations for channels [`FifoChannel`](crate::handlers::FifoChannel) and
/// [`RingChannel`](crate::handlers::RingChannel)
/// return a pair of ([`Callback`](crate::handlers::Callback), channel_handler).
///
/// The callback pushes data to the channel; the
/// channel handler ([`FifoChannelHandler`](crate::handlers::FifoChannelHandler) or
/// [`RingChannelHandler`](crate::handlers::RingChannelHandler)) allows taking data
/// from the channel.
///
/// The channel handler is stored
/// in the Zenoh object (e.g., a [`Subscriber`](crate::pubsub::Subscriber)). It can be accessed
/// via the [`handler`](crate::pubsub::Subscriber::handler) method or directly, by dereferencing the
/// Zenoh object. In practice this means that e.g. [recv_async](crate::handlers::fifo::FifoChannelHandler::recv_async)
/// can be called directly on the `Subscriber` object.
/// 
/// # Examples
/// 
/// ### Using a callback in a queryable
/// 
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// # let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let queryable = session.declare_queryable("key/expression")
///    .callback(|query| {
///         query.reply("key/expression", "value");
///     }).await.unwrap();
/// # }
/// ```
/// 
/// ### Use RingChannel to keep only the last 10 samples in a subscriber
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// # let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// use zenoh::handlers::RingChannel;
/// let subscriber = session.declare_subscriber("key/expression")
///     .with(RingChannel::new(10))
///     .await.unwrap();
/// # }
/// ```
///
pub mod handlers {
    #[zenoh_macros::internal]
    pub use crate::api::handlers::locked;
    #[zenoh_macros::internal]
    pub use crate::api::handlers::CallbackParameter;
    pub use crate::api::handlers::{
        Callback, CallbackDrop, DefaultHandler, FifoChannel, FifoChannelHandler, IntoHandler,
        RingChannel, RingChannelHandler,
    };
    pub mod fifo {
        pub use crate::api::handlers::{
            Drain, FifoChannel, FifoChannelHandler, IntoIter, Iter, RecvFut, RecvStream, TryIter,
        };
    }
}

/// # Quality of service primitives
/// 
/// This module provides types and enums to configure the quality of service (QoS) of Zenoh
/// operations, such as reliability and congestion control.
/// These parameters can be set via the corresponding builder methods, e.g.,
/// [reliability](crate::pubsub::PublisherBuilder::reliability), 
/// [priority](crate::pubsub::PublisherBuilder::priority) or
/// [congestion_control](crate::pubsub::PublisherBuilder::congestion_control).
///
/// # Example
/// 
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// # let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let publisher = session.declare_publisher("key/expression")
///   .reliability(zenoh::qos::Reliability::Reliable)
///   .priority(zenoh::qos::Priority::InteractiveHigh)
///   .congestion_control(zenoh::qos::CongestionControl::Block)
///   .await.unwrap();
/// # }
/// 
pub mod qos {
    pub use zenoh_protocol::core::CongestionControl;
    #[zenoh_macros::unstable]
    pub use zenoh_protocol::core::Reliability;

    pub use crate::api::publisher::Priority;
}

/// # Scouting primitives
///
/// Scouting is the process of discovering Zenoh nodes in the network.
/// The scouting process depends on the transport layer and the Zenoh configuration.
/// 
/// See more details at <https://zenoh.io/docs/getting-started/deployment/#scouting>.
/// 
pub mod scouting {
    pub use zenoh_config::wrappers::Hello;

    pub use crate::api::{
        builders::scouting::ScoutBuilder,
        scouting::{scout, Scout},
    };
}

/// # Liveliness primitives
///
/// Sometimes it's necessary to know whether a Zenoh node is available on the network.
/// It's possible to achieve this by declaring special publishers and queryables, but this task is
/// not straightforward, so a dedicated API is warranted.
///
/// The [liveliness](Session::liveliness) API allows a node to declare a 
/// [LivelinessToken](liveliness::LivelinessToken)
/// with a key expression assigned to it by [declare_token](liveliness::Liveliness::declare_token).
/// Other nodes can use the liveliness API to query this
/// key expression or subscribe to it to get notified when the token appears or disappears on the network
/// using corresponding functions [get](liveliness::Liveliness::get) and 
/// [declare_subscriber](liveliness::Liveliness::declare_subscriber).
/// 
/// # Examples
/// ### Declaring a token
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
///
/// ### Querying tokens
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let replies = session.liveliness().get("key/**").await.unwrap();
/// while let Ok(reply) = replies.recv_async().await {
///     if let Ok(sample) = reply.result() {
///         println!(">> Liveliness token {}", sample.key_expr());
///     }
/// }
/// # }
/// ```
///
/// ### Subscribing to liveliness changes
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::sample::SampleKind;
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let subscriber = session.liveliness().declare_subscriber("key/**").await.unwrap();
/// while let Ok(sample) = subscriber.recv_async().await {
///     match sample.kind() {
///         SampleKind::Put => println!("New liveliness: {}", sample.key_expr()),
///         SampleKind::Delete => println!("Lost liveliness: {}", sample.key_expr()),
///     }
/// }
/// # }
/// ```
pub mod liveliness {
    pub use crate::api::liveliness::{
        Liveliness, LivelinessGetBuilder, LivelinessSubscriberBuilder, LivelinessToken,
        LivelinessTokenBuilder, LivelinessTokenUndeclaration,
    };
}

/// Timestamp support
/// 
/// Each [`Sample`](crate::sample::Sample) has an optional [`Timestamp`](crate::time::Timestamp) associated with it.
/// The timestamp can be set using the 
/// [PublicationBuilder::timestamp](crate::pubsub::PublicationBuilder::timestamp) method when performing a
/// [`put`](crate::pubsub::Publisher::put) operation or by
/// [ReplyBuilder::timestamp](crate::query::ReplyBuilder::timestamp) when replying to a query with 
/// [reply](crate::query::Query::reply).
/// 
/// The timestamp consists of the time value itself and unique
/// [clock](https://docs.rs/uhlc/latest/uhlc/) identifier. Each
/// [Session](crate::session::Session) has its own clock, so the [new_timestamp](crate::session::Session::new_timestamp)
/// method can be used to create a new timestamp with the session's identifier.
pub mod time {
    pub use zenoh_protocol::core::{Timestamp, TimestampId, NTP64};
}

/// Configuration to pass to [`open`] and [`scout`] functions and associated constants.
///
/// The [`Config`](crate::config::Config) object contains all parameters necessary to configure
/// a Zenoh session or the scouting process. Usually a configuration file is stored in the json or
/// yaml format and loaded using the [`Config::from_file`](crate::config::Config::from_file) method.
/// It's also possible to read or
/// modify individual elements of the [`Config`] with the [`Config::insert_json5`](crate::config::Config::insert_json5)
/// and [`Config::get_json`](crate::config::Config::get_json) methods.
///
/// An example configuration file is
/// [available](https://github.com/eclipse-zenoh/zenoh/blob/release/1.0.0/DEFAULT_CONFIG.json5)
/// in the Zenoh repository.
///
pub mod config {
    pub use zenoh_config::{EndPoint, Locator, WhatAmI, WhatAmIMatcher, ZenohId};

    pub use crate::api::config::Config;
    #[zenoh_macros::unstable]
    pub use crate::api::config::Notifier;
}

#[cfg(all(
    feature = "plugins",
    not(all(feature = "unstable", feature = "internal"))
))]
compile_error!(
    "Plugin support is internal and unstable. The `unstable` and `internal` features must be enabled to use `plugins`."
);

#[zenoh_macros::internal]
pub mod internal {
    #[zenoh_macros::unstable]
    pub mod builders {
        pub mod close {
            pub use crate::api::builders::close::{BackgroundCloseBuilder, NolocalJoinHandle};
        }
    }
    pub mod traits {
        pub use crate::api::builders::sample::{
            EncodingBuilderTrait, QoSBuilderTrait, SampleBuilderTrait, TimestampBuilderTrait,
        };
    }
    pub use zenoh_core::{
        zasync_executor_init, zasynclock, zerror, zlock, zread, ztimeout, zwrite, ResolveFuture,
    };
    pub use zenoh_result::bail;
    pub use zenoh_sync::Condition;
    pub use zenoh_task::{TaskController, TerminatableTask};
    pub use zenoh_util::{
        zenoh_home, LibLoader, Timed, TimedEvent, TimedHandle, Timer, ZENOH_HOME_ENV_VAR,
    };

    /// A collection of useful buffers used by Zenoh internally and exposed to the user to facilitate
    /// reading and writing data.
    pub mod buffers {
        pub use zenoh_buffers::{
            buffer::{Buffer, SplitBuffer},
            reader::{
                AdvanceableReader, BacktrackableReader, DidntRead, DidntSiphon, HasReader, Reader,
                SiphonableReader,
            },
            writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
            ZBuf, ZBufReader, ZSlice, ZSliceBuffer,
        };
    }
    /// Initialize a Session with an existing Runtime.
    /// This operation is used by the plugins to share the same Runtime as the router.
    #[zenoh_macros::internal]
    pub mod runtime {
        pub use zenoh_runtime::ZRuntime;

        pub use crate::net::runtime::{AdminSpace, Runtime, RuntimeBuilder};
    }
    /// Plugins support
    #[cfg(feature = "plugins")]
    pub mod plugins {
        pub use crate::api::plugins::{
            PluginsManager, Response, RunningPlugin, RunningPluginTrait, ZenohPlugin, PLUGIN_PREFIX,
        };
    }

    pub use zenoh_result::ErrNo;
}

/// Shared memory.
#[zenoh_macros::unstable]
#[cfg(feature = "shared-memory")]
pub mod shm {
    pub use zenoh_shm::api::{
        buffer::{
            traits::{
                BufferRelayoutError, OwnedShmBuf, ResideInShm, ShmBuf, ShmBufIntoImmut, ShmBufMut,
                ShmBufUnsafeMut,
            },
            typed::Typed,
            zshm::{zshm, ZShm},
            zshmmut::{zshmmut, ZShmMut},
        },
        cleanup::cleanup_orphaned_shm_segments,
        client::{shm_client::ShmClient, shm_segment::ShmSegment},
        client_storage::{ShmClientStorage, GLOBAL_CLIENT_STORAGE},
        common::{
            types::{ChunkID, ProtocolID, PtrInSegment, SegmentID},
            with_id::WithProtocolID,
        },
        protocol_implementations::posix::{
            posix_shm_client::PosixShmClient, posix_shm_provider_backend::*,
            posix_shm_provider_backend_binary_heap::*, posix_shm_provider_backend_buddy::*,
            posix_shm_provider_backend_talc::*,
        },
        provider::{
            chunk::{AllocatedChunk, ChunkDescriptor},
            memory_layout::{
                BufferLayout, BuildLayout, LayoutForType, MemLayout, MemoryLayout, StaticLayout,
                TryIntoMemoryLayout,
            },
            shm_provider::{
                AllocBuilder, AllocLayout, AllocPolicy, AsyncAllocPolicy, BlockOn, DeallocEldest,
                DeallocOptimal, DeallocYoungest, Deallocate, Defragment, ForceDeallocPolicy,
                GarbageCollect, JustAlloc, LayoutAllocBuilder, ProviderAllocBuilder, ShmProvider,
                ShmProviderBuilder,
            },
            shm_provider_backend::ShmProviderBackend,
            types::{
                AllocAlignment, BufAllocResult, BufLayoutAllocResult, ChunkAllocResult,
                TypedBufAllocResult, TypedBufLayoutAllocResult, ZAllocError, ZLayoutAllocError,
                ZLayoutError,
            },
        },
    };
}

#[cfg(test)]
mod tests;
