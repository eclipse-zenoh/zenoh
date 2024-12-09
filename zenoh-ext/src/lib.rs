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
    advanced_publisher::{AdvancedPublisher, AdvancedPublisherBuilder},
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
