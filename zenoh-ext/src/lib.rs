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
//! rest, and computations. It elegantly blends traditional pub/sub with geo-distributed
//! storage, queries, and computations, while retaining a level of time and space efficiency
//! that is well beyond any of the mainstream stacks.
//!
//! This crate provides components extending the core Zenoh functionalities.
//!
//! These components include
//!
//! # Serialization
//!
//! The base zenoh library allows to send/receive data as raw bytes payload. But in order to
//! simplify the library's usability and, more importantly, to ensure interoperability
//! between zenoh-based applications, this crate provides serialization/deserialization
//! functionalities.
//!
//! The key functions are [`z_serialize`] and [`z_deserialize`] that allows to
//! serialize/deserialize any data structure implementing the [`Serialize`] and
//! [`Deserialize`] traits respectively.
//!
//! # Advanced Pub/Sub
//!
//! The [`AdvancedPublisher`] and [`AdvancedSubscriber`] provide advanced pub/sub
//! functionalities, including support for message history, recovery, and more.
#[cfg(feature = "unstable")]
mod advanced_cache;
#[cfg(feature = "unstable")]
mod advanced_publisher;
#[cfg(feature = "unstable")]
mod advanced_subscriber;
#[cfg(feature = "unstable")]
pub mod group;
#[cfg(feature = "unstable")]
mod publication_cache;
#[cfg(feature = "unstable")]
mod publisher_ext;
#[cfg(feature = "unstable")]
mod querying_subscriber;
mod serialization;
#[cfg(feature = "unstable")]
mod session_ext;
#[cfg(feature = "unstable")]
mod subscriber_ext;

#[cfg(feature = "internal")]
pub use crate::serialization::VarInt;
pub use crate::serialization::{
    z_deserialize, z_serialize, Deserialize, Serialize, ZDeserializeError, ZDeserializer,
    ZReadIter, ZSerializer,
};
#[cfg(feature = "unstable")]
#[allow(deprecated)]
pub use crate::{
    advanced_cache::{CacheConfig, RepliesConfig},
    advanced_publisher::{
        AdvancedPublicationBuilder, AdvancedPublisher, AdvancedPublisherBuilder,
        AdvancedPublisherDeleteBuilder, AdvancedPublisherPutBuilder, MissDetectionConfig,
    },
    advanced_subscriber::{
        AdvancedSubscriber, AdvancedSubscriberBuilder, HistoryConfig, Miss, RecoveryConfig,
        SampleMissHandlerUndeclaration, SampleMissListener, SampleMissListenerBuilder,
    },
    publication_cache::{PublicationCache, PublicationCacheBuilder},
    publisher_ext::AdvancedPublisherBuilderExt,
    querying_subscriber::{
        ExtractSample, FetchingSubscriber, FetchingSubscriberBuilder, KeySpace, LivelinessSpace,
        QueryingSubscriberBuilder, UserSpace,
    },
    session_ext::SessionExt,
    subscriber_ext::{AdvancedSubscriberBuilderExt, SubscriberBuilderExt, SubscriberForward},
};
