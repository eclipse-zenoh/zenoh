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
use zenoh::pubsub::PublisherBuilder;

use crate::{advanced_cache::CacheConfig, AdvancedPublisherBuilder, MissDetectionConfig};

/// Some extensions to the [`zenoh::publication::PublisherBuilder`](zenoh::publication::PublisherBuilder)
#[zenoh_macros::unstable]
pub trait AdvancedPublisherBuilderExt<'a, 'b, 'c> {
    /// Allow matching [`AdvancedSubscribers`](crate::AdvancedSubscriber) to recover history and/or missed samples.
    #[zenoh_macros::unstable]
    fn cache(self, config: CacheConfig) -> AdvancedPublisherBuilder<'a, 'b, 'c>;

    /// Allow matching [`AdvancedSubscribers`](crate::AdvancedSubscriber) to detect lost samples
    /// and optionally ask for retransimission.
    ///
    /// Retransmission can only be achieved if [`cache`](crate::AdvancedPublisherBuilder::cache) is also enabled.
    #[zenoh_macros::unstable]
    fn sample_miss_detection(
        self,
        config: MissDetectionConfig,
    ) -> AdvancedPublisherBuilder<'a, 'b, 'c>;

    /// Allow this publisher to be detected by [`AdvancedSubscribers`](crate::AdvancedSubscriber).
    ///
    /// This allows [`AdvancedSubscribers`](crate::AdvancedSubscriber) to retrieve the local history.
    #[zenoh_macros::unstable]
    fn publisher_detection(self) -> AdvancedPublisherBuilder<'a, 'b, 'c>;

    /// Turn this [`Publisher`](zenoh::publication::Publisher) into an [`AdvancedPublisher`](crate::AdvancedPublisher).
    #[zenoh_macros::unstable]
    fn advanced(self) -> AdvancedPublisherBuilder<'a, 'b, 'c>;
}

#[zenoh_macros::unstable]
impl<'a, 'b, 'c> AdvancedPublisherBuilderExt<'a, 'b, 'c> for PublisherBuilder<'a, 'b> {
    /// Allow matching [`AdvancedSubscribers`](crate::AdvancedSubscriber) to recover history and/or missed samples.
    #[zenoh_macros::unstable]
    fn cache(self, config: CacheConfig) -> AdvancedPublisherBuilder<'a, 'b, 'c> {
        AdvancedPublisherBuilder::new(self).cache(config)
    }

    /// Allow matching [`AdvancedSubscribers`](crate::AdvancedSubscriber) to detect lost samples
    /// and optionally ask for retransimission.
    ///
    /// Retransmission can only be achieved if [`cache`](crate::AdvancedPublisherBuilder::cache) is also enabled.
    #[zenoh_macros::unstable]
    fn sample_miss_detection(
        self,
        config: MissDetectionConfig,
    ) -> AdvancedPublisherBuilder<'a, 'b, 'c> {
        AdvancedPublisherBuilder::new(self).sample_miss_detection(config)
    }

    /// Allow this publisher to be detected by [`AdvancedSubscribers`](crate::AdvancedSubscriber).
    ///
    /// This allows [`AdvancedSubscribers`](crate::AdvancedSubscriber) to retrieve the local history.
    #[zenoh_macros::unstable]
    fn publisher_detection(self) -> AdvancedPublisherBuilder<'a, 'b, 'c> {
        AdvancedPublisherBuilder::new(self).publisher_detection()
    }

    /// Turn this [`Publisher`](zenoh::publication::Publisher) into an [`AdvancedPublisher`](crate::AdvancedPublisher).
    #[zenoh_macros::unstable]
    fn advanced(self) -> AdvancedPublisherBuilder<'a, 'b, 'c> {
        AdvancedPublisherBuilder::new(self)
    }
}
