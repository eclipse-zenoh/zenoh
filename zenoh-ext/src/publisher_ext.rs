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

use crate::{advanced_cache::CacheConfig, AdvancedPublisherBuilder};

/// Some extensions to the [`zenoh::publication::PublisherBuilder`](zenoh::publication::PublisherBuilder)
#[zenoh_macros::unstable]
pub trait PublisherBuilderExt<'a, 'b> {
    /// Allow matching Subscribers to detect lost samples and
    /// optionally ask for retransimission.
    ///
    /// Retransmission can only be achieved if history is enabled.
    fn cache(self, config: CacheConfig) -> AdvancedPublisherBuilder<'a, 'b>;

    /// Allow this publisher to be detected by subscribers.
    ///
    /// This allows Subscribers to retrieve the local history.
    fn late_joiner_detection(self) -> AdvancedPublisherBuilder<'a, 'b>;
}

impl<'a, 'b> PublisherBuilderExt<'a, 'b> for PublisherBuilder<'a, 'b> {
    /// Allow matching Subscribers to detect lost samples and
    /// optionally ask for retransimission.
    ///
    /// Retransmission can only be achieved if history is enabled.
    fn cache(self, config: CacheConfig) -> AdvancedPublisherBuilder<'a, 'b> {
        AdvancedPublisherBuilder::new(self.session, self.key_expr).cache(config)
    }

    /// Allow this publisher to be detected by subscribers.
    ///
    /// This allows Subscribers to retrieve the local history.
    fn late_joiner_detection(self) -> AdvancedPublisherBuilder<'a, 'b> {
        AdvancedPublisherBuilder::new(self.session, self.key_expr).late_joiner_detection()
    }
}
