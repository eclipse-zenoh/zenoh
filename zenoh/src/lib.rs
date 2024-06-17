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
//! use zenoh::prelude::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let session = zenoh::open(zenoh::config::default()).await.unwrap();
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
//!     let session = zenoh::open(zenoh::config::default()).await.unwrap();
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
//!     let session = zenoh::open(zenoh::config::default()).await.unwrap();
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

#[doc(inline)]
pub use {
    crate::{
        config::Config,
        core::{Error, Result},
        scouting::scout,
        session::{open, Session},
    },
    zenoh_util::{init_log_from_env_or, try_init_log_from_env},
};

pub mod prelude;

/// Zenoh core types
pub mod core {
    #[allow(deprecated)]
    pub use zenoh_core::{AsyncResolve, SyncResolve};
    pub use zenoh_core::{Resolvable, Resolve, Wait};
    pub use zenoh_result::ErrNo;
    /// A zenoh error.
    pub use zenoh_result::Error;
    /// A zenoh result.
    pub use zenoh_result::ZResult as Result;

    /// Zenoh message priority
    pub use crate::api::publisher::Priority;
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
    pub use zenoh_keyexpr::{keyexpr, OwnedKeyExpr};

    pub use crate::api::key_expr::{KeyExpr, KeyExprUndeclaration};
    // keyexpr format macro support
    #[zenoh_macros::unstable]
    pub mod format {
        pub use zenoh_keyexpr::format::*;
        pub use zenoh_macros::{kedefine, keformat, kewrite};
        pub mod macro_support {
            pub use zenoh_keyexpr::format::macro_support::*;
        }
    }
}

/// Zenoh [`Session`](crate::session::Session) and associated types
pub mod session {
    #[zenoh_macros::internal]
    pub use crate::api::session::{init, InitBuilder};
    pub use crate::api::{
        builders::publisher::{SessionDeleteBuilder, SessionPutBuilder},
        query::SessionGetBuilder,
        session::{open, OpenBuilder, Session, SessionDeclarations, SessionRef, Undeclarable},
    };
}

/// Tools to access information about the current zenoh [`Session`](crate::Session).
pub mod info {
    pub use zenoh_config::wrappers::{EntityGlobalId, ZenohId};
    pub use zenoh_protocol::core::EntityId;

    pub use crate::api::info::{
        PeersZenohIdBuilder, RoutersZenohIdBuilder, SessionInfo, ZenohIdBuilder,
    };
}

/// Sample primitives
pub mod sample {
    #[zenoh_macros::unstable]
    pub use crate::api::sample::Locality;
    #[zenoh_macros::unstable]
    pub use crate::api::sample::SourceInfo;
    pub use crate::api::{
        builders::sample::{
            EncodingBuilderTrait, QoSBuilderTrait, SampleBuilder, SampleBuilderAny,
            SampleBuilderDelete, SampleBuilderPut, SampleBuilderTrait, TimestampBuilderTrait,
        },
        sample::{Sample, SampleFields, SampleKind, SourceSn},
    };
}

/// Encoding support
pub mod encoding {
    pub use crate::api::encoding::Encoding;
}

/// Payload primitives
pub mod bytes {
    pub use crate::api::bytes::{
        Deserialize, OptionZBytes, Serialize, ZBytes, ZBytesIterator, ZBytesReader, ZBytesWriter,
        ZDeserializeError, ZSerde,
    };
}

/// [Selector](https://github.com/eclipse-zenoh/roadmap/tree/main/rfcs/ALL/Selectors) to issue queries
pub mod selector {
    pub use zenoh_protocol::core::Parameters;
    #[zenoh_macros::unstable]
    pub use zenoh_util::time_range::{TimeBound, TimeExpr, TimeRange};

    pub use crate::api::selector::Selector;
    #[zenoh_macros::unstable]
    pub use crate::api::selector::ZenohParameters;
}

/// Subscribing primitives
pub mod subscriber {
    /// The kind of reliability.
    pub use zenoh_protocol::core::Reliability;

    pub use crate::api::subscriber::{FlumeSubscriber, Subscriber, SubscriberBuilder};
}

/// Publishing primitives
pub mod publisher {
    pub use zenoh_protocol::core::CongestionControl;

    #[zenoh_macros::unstable]
    pub use crate::api::publisher::MatchingListener;
    #[zenoh_macros::unstable]
    pub use crate::api::publisher::MatchingListenerBuilder;
    #[zenoh_macros::unstable]
    pub use crate::api::publisher::MatchingListenerUndeclaration;
    #[zenoh_macros::unstable]
    pub use crate::api::publisher::MatchingStatus;
    #[zenoh_macros::unstable]
    pub use crate::api::publisher::PublisherDeclarations;
    #[zenoh_macros::unstable]
    pub use crate::api::publisher::PublisherRef;
    pub use crate::api::{
        builders::publisher::{
            PublicationBuilder, PublicationBuilderDelete, PublicationBuilderPut, PublisherBuilder,
            PublisherDeleteBuilder, PublisherPutBuilder,
        },
        publisher::{Publisher, PublisherUndeclaration},
    };
}

/// Get operation primitives
pub mod querier {
    // Later the `Querier` with `get`` operation will be added here, in addition to `Session::get`,
    // similarly to the `Publisher` with `put` operation and `Session::put`
}

/// Query and Reply primitives
pub mod query {
    #[zenoh_macros::unstable]
    pub use crate::api::query::ReplyKeyExpr;
    #[zenoh_macros::internal]
    pub use crate::api::queryable::ReplySample;
    pub use crate::api::{
        query::{ConsolidationMode, QueryConsolidation, QueryTarget, Reply, ReplyError},
        queryable::{Query, ReplyBuilder, ReplyBuilderDelete, ReplyBuilderPut, ReplyErrBuilder},
    };
}

/// Queryable primitives
pub mod queryable {
    pub use crate::api::queryable::{Queryable, QueryableBuilder, QueryableUndeclaration};
}

/// Callback handler trait
pub mod handlers {
    pub use crate::api::handlers::{
        locked, Callback, CallbackDrop, DefaultHandler, FifoChannel, IntoHandler, RingChannel,
        RingChannelHandler,
    };
}

/// Scouting primitives
pub mod scouting {
    pub use zenoh_config::wrappers::Hello;

    pub use crate::api::scouting::{scout, Scout, ScoutBuilder};
}

/// Liveliness primitives
#[zenoh_macros::unstable]
pub mod liveliness {
    pub use crate::api::liveliness::{
        Liveliness, LivelinessGetBuilder, LivelinessSubscriberBuilder, LivelinessToken,
        LivelinessTokenBuilder, LivelinessTokenUndeclaration,
    };
}

/// Timestamp support
pub mod time {
    pub use zenoh_protocol::core::{Timestamp, TimestampId, NTP64};

    pub use crate::api::time::new_timestamp;
}

/// Configuration to pass to [`open`](crate::session::open) and [`scout`](crate::scouting::scout) functions and associated constants
pub mod config {
    // pub use zenoh_config::{
    //     client, default, peer, Config, EndPoint, Locator, ModeDependentValue, PermissionsConf,
    //     PluginLoad, ValidatedMap, ZenohId,
    // };
    pub use zenoh_config::*;
}

#[cfg(all(feature = "internal", not(feature = "unstable")))]
compile_error!(
    "All internal functionality is unstable. The `unstable` feature must be enabled to use `internal`."
);
#[cfg(all(
    feature = "plugins",
    not(all(feature = "unstable", feature = "internal"))
))]
compile_error!(
    "The plugins support is internal and unstable. The `unstable` and `internal` features must be enabled to use `plugins`."
);

#[zenoh_macros::internal]
pub mod internal {
    pub use zenoh_core::{
        zasync_executor_init, zasynclock, zerror, zlock, zread, ztimeout, zwrite, ResolveFuture,
    };
    pub use zenoh_result::bail;
    pub use zenoh_sync::Condition;
    pub use zenoh_task::{TaskController, TerminatableTask};
    pub use zenoh_util::{zenoh_home, LibLoader, Timed, TimedEvent, Timer, ZENOH_HOME_ENV_VAR};

    /// A collection of useful buffers used by zenoh internally and exposed to the user to facilitate
    /// reading and writing data.
    pub mod buffers {
        pub use zenoh_buffers::{
            buffer::SplitBuffer,
            reader::{HasReader, Reader},
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

    pub use crate::api::value::Value;
}

#[zenoh_macros::unstable]
#[cfg(feature = "shared-memory")]
pub mod shm {
    pub use zenoh_shm::api::{
        buffer::{
            zshm::{zshm, ZShm},
            zshmmut::{zshmmut, ZShmMut},
        },
        client::{shm_client::ShmClient, shm_segment::ShmSegment},
        client_storage::{ShmClientStorage, GLOBAL_CLIENT_STORAGE},
        common::types::{ChunkID, ProtocolID, SegmentID},
        protocol_implementations::posix::{
            posix_shm_client::PosixShmClient,
            posix_shm_provider_backend::{
                LayoutedPosixShmProviderBackendBuilder, PosixShmProviderBackend,
                PosixShmProviderBackendBuilder,
            },
            protocol_id::POSIX_PROTOCOL_ID,
        },
        provider::{
            chunk::{AllocatedChunk, ChunkDescriptor},
            shm_provider::{
                AllocBuilder, AllocBuilder2, AllocLayout, AllocLayoutSizedBuilder, AllocPolicy,
                AsyncAllocPolicy, BlockOn, DeallocEldest, DeallocOptimal, DeallocYoungest,
                Deallocate, Defragment, DynamicProtocolID, ForceDeallocPolicy, GarbageCollect,
                JustAlloc, ProtocolIDSource, ShmProvider, ShmProviderBuilder,
                ShmProviderBuilderBackendID, ShmProviderBuilderID, StaticProtocolID,
            },
            shm_provider_backend::ShmProviderBackend,
            types::{
                AllocAlignment, BufAllocResult, BufLayoutAllocResult, ChunkAllocResult,
                MemoryLayout, ZAllocError, ZLayoutAllocError, ZLayoutError,
            },
        },
    };
}
