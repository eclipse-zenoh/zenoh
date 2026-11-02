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

/// Extend a [`PublisherBuilder`] with advanced publishing options.
#[zenoh_macros::unstable]
pub trait AdvancedPublisherBuilderExt<'a, 'b, 'c> {
    /// Attach a history/recovery cache; see [`AdvancedPublisherBuilder::cache`] for prerequisites.
    #[zenoh_macros::unstable]
    fn cache(self, config: CacheConfig) -> AdvancedPublisherBuilder<'a, 'b, 'c>;

    /// Enable loss detection; see [`AdvancedPublisherBuilder::sample_miss_detection`].
    #[zenoh_macros::unstable]
    fn sample_miss_detection(
        self,
        config: MissDetectionConfig,
    ) -> AdvancedPublisherBuilder<'a, 'b, 'c>;

    /// Advertise publisher presence; see [`AdvancedPublisherBuilder::publisher_detection`].
    #[zenoh_macros::unstable]
    fn publisher_detection(self) -> AdvancedPublisherBuilder<'a, 'b, 'c>;

    /// Convert this builder into an [`AdvancedPublisherBuilder`].
    #[zenoh_macros::unstable]
    fn advanced(self) -> AdvancedPublisherBuilder<'a, 'b, 'c>;
}

#[zenoh_macros::unstable]
impl<'a, 'b, 'c> AdvancedPublisherBuilderExt<'a, 'b, 'c> for PublisherBuilder<'a, 'b> {
    /// Allow matching [`AdvancedSubscribers`](crate::AdvancedSubscriber) to retrieve history and recover missed publications.
    ///
    /// See [`AdvancedPublisherBuilder::cache`] for timestamping requirements.
    #[zenoh_macros::unstable]
    fn cache(self, config: CacheConfig) -> AdvancedPublisherBuilder<'a, 'b, 'c> {
        AdvancedPublisherBuilder::new(self).cache(config)
    }

    /// Allow matching [`AdvancedSubscribers`](crate::AdvancedSubscriber) to detect missed publications.
    ///
    /// Recovery also requires [`cache`](AdvancedPublisherBuilder::cache).
    #[zenoh_macros::unstable]
    fn sample_miss_detection(
        self,
        config: MissDetectionConfig,
    ) -> AdvancedPublisherBuilder<'a, 'b, 'c> {
        AdvancedPublisherBuilder::new(self).sample_miss_detection(config)
    }

    /// Advertise this publisher's presence through liveliness.
    ///
    /// Subscribers using [`HistoryConfig::detect_late_publishers`](crate::HistoryConfig::detect_late_publishers)
    /// can retrieve its history when [`cache`](AdvancedPublisherBuilder::cache) is also enabled.
    #[zenoh_macros::unstable]
    fn publisher_detection(self) -> AdvancedPublisherBuilder<'a, 'b, 'c> {
        AdvancedPublisherBuilder::new(self).publisher_detection()
    }

    /// Convert this [`PublisherBuilder`] into an [`AdvancedPublisherBuilder`].
    #[zenoh_macros::unstable]
    fn advanced(self) -> AdvancedPublisherBuilder<'a, 'b, 'c> {
        AdvancedPublisherBuilder::new(self)
    }
}
