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
pub mod group;
mod publication_cache;
mod querying_subscriber;
mod session_ext;
mod subscriber_ext;
pub use publication_cache::{PublicationCache, PublicationCacheBuilder};
pub use querying_subscriber::{
    FetchingSubscriber, FetchingSubscriberBuilder, QueryingSubscriberBuilder,
};
pub use session_ext::SessionExt;
pub use subscriber_ext::SubscriberBuilderExt;
pub use subscriber_ext::SubscriberForward;
use zenoh::query::Reply;
use zenoh::{sample::Sample, Result as ZResult};
use zenoh_core::zerror;

/// The space of keys to use in a [`FetchingSubscriber`].
pub enum KeySpace {
    User,
    Liveliness,
}

/// The key space for user data.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct UserSpace;

impl From<UserSpace> for KeySpace {
    fn from(_: UserSpace) -> Self {
        KeySpace::User
    }
}

/// The key space for liveliness tokens.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct LivelinessSpace;

impl From<LivelinessSpace> for KeySpace {
    fn from(_: LivelinessSpace) -> Self {
        KeySpace::Liveliness
    }
}

pub trait ExtractSample {
    fn extract(self) -> ZResult<Sample>;
}

impl ExtractSample for Reply {
    fn extract(self) -> ZResult<Sample> {
        self.into_result().map_err(|e| zerror!("{:?}", e).into())
    }
}
