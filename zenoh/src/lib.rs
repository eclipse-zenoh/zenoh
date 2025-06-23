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
//!
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(zenoh::Config::default()).await.unwrap();
//!     session.put("key/expression", "value").await.unwrap();
//!     session.close().await.unwrap();
//! }
//! ```
//!
//! ### Subscribe
//! The example below shows how to consume values for a key expressions.
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
//! ### Query
//! The example below shows how to make a distributed query to collect the values associated with the
//! resources whose key match the given *key expression*.
//! ```
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
/// A zenoh error.
pub use zenoh_result::Error;
/// A zenoh result.
pub use zenoh_result::ZResult as Result;
#[doc(inline)]
pub use zenoh_util::{init_log_from_env_or, try_init_log_from_env};

#[doc(inline)]
pub use crate::{
    config::Config,
    scouting::scout,
    session::{open, Session},
};

/// [Key expression](https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Key%20Expressions.md) are Zenoh's address space.
///
/// In Zenoh, operations are performed on keys. To allow addressing multiple keys with a single operation, we use Key Expressions (KE).
/// KEs are a small language that express sets of keys through a glob-like language.
///
/// These semantics can be a bit difficult to implement, so this module provides the following facilities:
///
/// # Storing Key Expressions
/// This module provides 3 flavours to store strings that have been validated to respect the KE syntax:
/// - [`keyexpr`](crate::key_expr::keyexpr) is the equivalent of a [`str`],
/// - [`OwnedKeyExpr`](crate::key_expr::OwnedKeyExpr) works like an [`std::sync::Arc<str>`],
/// - [`KeyExpr`](crate::key_expr::KeyExpr) works like a [`std::borrow::Cow<str>`], but also stores some additional context internal to Zenoh to optimize
///   routing and network usage.
///
/// All of these types [`Deref`](std::ops::Deref) to [`keyexpr`](crate::key_expr::keyexpr), which notably has methods to check whether a given [`intersects`](crate::key_expr::keyexpr::includes) with another,
/// or even if a [`includes`](crate::key_expr::keyexpr::includes) another.
///
/// # Tying values to Key Expressions
/// When storing values tied to Key Expressions, you might want something more specialized than a [`HashMap`](std::collections::HashMap) if you want to respect
/// the Key Expression semantics with high performance.
///
/// Enter [KeTrees](crate::key_expr::keyexpr_tree). These are data-structures specially built to store KE-value pairs in a manner that supports the set-semantics of KEs.
///
/// # Building and parsing Key Expressions
/// A common issue in REST API is the association of meaning to sections of the URL, and respecting that API in a convenient manner.
/// The same issue arises naturally when designing a KE space, and [`KeFormat`](crate::key_expr::format::KeFormat) was designed to help you with this,
/// both in constructing and in parsing KEs that fit the formats you've defined.
///
/// [`kedefine`](crate::key_expr::format::kedefine) also allows you to define formats at compile time, allowing a more performant, but more importantly safer and more convenient use of said formats,
/// as the [`keformat`](crate::key_expr::format::keformat) and [`kewrite`](crate::key_expr::format::kewrite) macros will be able to tell you if you're attempting to set fields of the format that do not exist.
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

/// Zenoh [`Session`] and associated types
///
/// The [`Session`] is the main component of the Zenoh.
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

/// Sample primitives
///
/// The [`Sample`](crate::sample::Sample) structure is the data unit received from [`Subscriber`](crate::pubsub::Subscriber)
/// or [`Queryable`](crate::query::Queryable) instances. It contains the payload and all the metadata associated with the data.
pub mod sample {
    #[zenoh_macros::unstable]
    pub use crate::api::sample::Locality;
    #[zenoh_macros::unstable]
    pub use crate::api::sample::{SourceInfo, SourceSn};
    pub use crate::api::{
        builders::sample::{
            SampleBuilder, SampleBuilderAny, SampleBuilderDelete, SampleBuilderPut,
        },
        sample::{Sample, SampleFields, SampleKind},
    };
}

/// Payload primitives
///
/// The [`ZBytes`](crate::bytes::ZBytes) type is Zenoh's representation of raw byte data.
/// It provides mechanisms for zero-copy creation and access (`From<Vec<u8>>` and
/// [`ZBytes::slices`](crate::bytes::ZBytes::slices)), as well as methods for sequential
/// reading/writing ([`ZBytes::reader`](crate::bytes::ZBytes::reader), [`ZBytes::writer`](crate::bytes::ZBytes::writer)).
///
/// The`zenoh_ext` crate provides serialization and deserialization of basic types and structures for `ZBytes`
/// [`z_serialize`](../../zenoh_ext/fn.z_serialize.html) /
/// [`z_deserialize`](../../zenoh_ext/fn.z_deserialize.html).
pub mod bytes {
    pub use crate::api::{
        bytes::{OptionZBytes, ZBytes, ZBytesReader, ZBytesSliceIterator, ZBytesWriter},
        encoding::Encoding,
    };
}

/// Pub/sub primitives
///
/// The [`Publisher`](crate::pubsub::Publisher) instance is declared by a
/// [`Session::declare_publisher`](crate::Session::declare_publisher) method.
///
/// The data [`Sample`](crate::sample::Sample)s
/// are received by [`Subscriber`](crate::pubsub::Subscriber)s
/// declared by a [`Session::declare_subscriber`](crate::Session::declare_subscriber)
///
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

/// Query/reply primitives
///
/// The [`Queryable`](crate::query::Queryable) instance is declared by a
/// [`Session::declare_queryable`](crate::Session::declare_queryable) method.
/// It is requested by a [`Session::get`](crate::Session::get) operation which
/// receives data in [`Reply`](crate::query::Reply) structures.
///
pub mod query {
    pub use zenoh_protocol::core::Parameters;
    #[zenoh_macros::unstable]
    pub use zenoh_util::time_range::{TimeBound, TimeExpr, TimeRange};

    #[zenoh_macros::internal]
    pub use crate::api::queryable::ReplySample;
    #[zenoh_macros::unstable]
    pub use crate::api::{
        builders::querier::{QuerierBuilder, QuerierGetBuilder},
        querier::Querier,
        query::ReplyKeyExpr,
        selector::ZenohParameters,
    };
    pub use crate::api::{
        builders::{
            queryable::QueryableBuilder,
            reply::{ReplyBuilder, ReplyBuilderDelete, ReplyBuilderPut, ReplyErrBuilder},
        },
        query::{ConsolidationMode, QueryConsolidation, QueryTarget, Reply, ReplyError},
        queryable::{Query, Queryable, QueryableUndeclaration},
        selector::Selector,
    };
}

#[zenoh_macros::unstable]
pub mod matching {
    pub use crate::api::{
        builders::matching_listener::MatchingListenerBuilder,
        matching::{MatchingListener, MatchingListenerUndeclaration, MatchingStatus},
    };
}

/// Callback handler trait.
///
/// Zenoh primitives that receive data (e.g., [`Subscriber`](crate::pubsub::Subscriber),
/// [`Query`](crate::query::Query), etc.) have a
/// [`with`](crate::pubsub::SubscriberBuilder::with) method which accepts a handler for the data.
///
/// The handler is a pair of a [`Callback`](crate::handlers::Callback) and an arbitrary `Handler`
/// object used to access data received by the callback. When the data is processed by the callback itself
/// the handler type can be `()`. For convenience, the
/// [`callback`](crate::pubsub::SubscriberBuilder::callback) method, which accepts
/// a `Fn(T)` only can be used in this case.
///
/// The [`with`](crate::pubsub::SubscriberBuilder::with) method accepts any type that
/// implements the [`IntoHandler`](crate::handlers::IntoHandler) trait, which provides a
/// conversion to a pair of [`Callback`](crate::handlers::Callback) and handler.
///
/// The `IntoHandler` for channels [`FifoChannel`](crate::handlers::FifoChannel) and
/// [`RingChannel`](crate::handlers::RingChannel)
/// return a pair of ([`Callback`](crate::handlers::Callback), channel_handler).
///
/// The callback pushes data to the channel, the
/// channel handler [`FifoChannelHandler`](crate::handlers::FifoChannelHandler) or
/// [`RingChannelHandler`](crate::handlers::RingChannelHandler) allows to take data
/// from the channel.
///
/// The channel handler is stored
/// in the Zenoh object (e.g., [`Subscriber`](crate::pubsub::Subscriber)). It can be accessed
/// by [`handler`](crate::pubsub::Subscriber::handler) method or just directly by dereferencing the
/// Zenoh object.
pub mod handlers {
    #[zenoh_macros::internal]
    pub use crate::api::handlers::locked;
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

/// Quality of service primitives
pub mod qos {
    pub use zenoh_protocol::core::CongestionControl;
    #[zenoh_macros::unstable]
    pub use zenoh_protocol::core::Reliability;

    pub use crate::api::publisher::Priority;
}

/// Scouting primitives
///
/// Scouting is the process of discovering Zenoh nodes in the network.
/// The scouting process depends on the transport layer and on the zenoh configuration.
/// See more details at <https://zenoh.io/docs/getting-started/deployment/#scouting>.
///
pub mod scouting {
    pub use zenoh_config::wrappers::Hello;

    pub use crate::api::{
        builders::scouting::ScoutBuilder,
        scouting::{scout, Scout},
    };
}

/// Liveliness primitives
///
/// A [`LivelinessToken`](liveliness::LivelinessToken) is a token which liveliness is tied
/// to the Zenoh [`Session`] and can be monitored by remote applications.
///
/// # Examples
/// ### Declaring a token
/// ```
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
/// ```
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
pub mod time {
    pub use zenoh_protocol::core::{Timestamp, TimestampId, NTP64};
}

/// Configuration to pass to [`open`] and [`scout`] functions and associated constants.
///
/// The zenoh configurattion is stored in a JSON file. The [`Config`] can be constructed from it using
/// the corresponding `from_...` methods. It's also possible to read or
/// modify individual elements of the [`Config`] with [`Config::insert_json5`](crate::config::Config::insert_json5)
/// and [`Config::get_json`](crate::config::Config::get_json) methods.
///
/// The example of the configuration file is
/// [available](https://github.com/eclipse-zenoh/zenoh/blob/release/1.0.0/DEFAULT_CONFIG.json5)
/// in the zenoh repository.
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
    "The plugins support is internal and unstable. The `unstable` and `internal` features must be enabled to use `plugins`."
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

    /// A collection of useful buffers used by zenoh internally and exposed to the user to facilitate
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

        pub use crate::net::runtime::{AdminSpace, DynamicRuntime, Runtime, RuntimeBuilder};
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
            posix_shm_client::PosixShmClient,
            posix_shm_provider_backend::{
                LayoutedPosixShmProviderBackendBuilder, PosixShmProviderBackend,
                PosixShmProviderBackendBuilder,
            },
        },
        provider::{
            chunk::{AllocatedChunk, ChunkDescriptor},
            shm_provider::{
                AllocLayout, AllocLayoutSizedBuilder, AllocPolicy, AsyncAllocPolicy, BlockOn,
                DeallocEldest, DeallocOptimal, DeallocYoungest, Deallocate, Defragment,
                ForceDeallocPolicy, GarbageCollect, JustAlloc, LayoutAllocBuilder,
                ProviderAllocBuilder, ShmProvider, ShmProviderBuilder,
            },
            shm_provider_backend::ShmProviderBackend,
            types::{
                AllocAlignment, BufAllocResult, BufLayoutAllocResult, ChunkAllocResult,
                MemoryLayout, ZAllocError, ZLayoutAllocError, ZLayoutError,
            },
        },
    };
}

#[cfg(test)]
mod tests;
