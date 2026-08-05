//
// Copyright (c) 2024 ZettaScale Technology
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

use crate::{
    api::client_storage::GLOBAL_CLIENT_STORAGE,
    cleanup::CLEANUP,
    metadata::{storage::GLOBAL_METADATA_STORAGE, subscription::GLOBAL_METADATA_SUBSCRIPTION},
    watchdog::{confirmator::GLOBAL_CONFIRMATOR, validator::GLOBAL_VALIDATOR},
};

pub fn init() {
    CLEANUP.init();
    GLOBAL_CLIENT_STORAGE.init();
    GLOBAL_METADATA_STORAGE.init();
    GLOBAL_METADATA_SUBSCRIPTION.init();
    GLOBAL_CONFIRMATOR.init();
    GLOBAL_VALIDATOR.init();
}

/// Raise `RLIMIT_NOFILE` soft limit to hard limit.
///
/// Call from SHM segment fan-in: [create], [open], [ensure_not_persistent]; not [init].
/// Covers eager/lazy and direct/indirect SHM use.
/// Idempotent (through [Once]); Unix-only, no-op elsewhere and in tests.
///
/// [create]: crate::shm::Segment::create
/// [open]: crate::shm::Segment::open
/// [ensure_not_persistent]: crate::shm::Segment::ensure_not_persistent
/// [init]: crate::init::init
/// [Once]: std::sync::Once
pub(crate) fn raise_nofile_soft_limit_to_hard_limit_once() {
    #[cfg(all(unix, not(test), not(feature = "test")))]
    {
        static RAISE_NOFILE_LIMIT: std::sync::Once = std::sync::Once::new();

        RAISE_NOFILE_LIMIT.call_once(|| {
            if let Err(error) = rlimit::increase_nofile_limit(u64::MAX) {
                tracing::warn!("failed to raise NOFILE soft limit to hard limit: {error}");
            }
        });
    }
}
