//
// Copyright (c) 2026 ZettaScale Technology
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

use std::{
    num::NonZeroUsize,
    sync::{Arc, Weak},
};

use crate::api::protocol_implementations::posix::posix_shm_segment::PosixShmSegment;

use super::pool::ShmSegmentPool;

/// A wrapper around [`Arc<PosixShmSegment>`] whose `Drop` implementation returns the
/// segment to the pool instead of unmapping it.
///
/// When the last [`ZSlice`] referencing a message drops, this wrapper's `Drop` fires.
/// If the parent [`ShmSegmentPool`] is still alive (the `Weak` reference upgrades), the
/// segment is pushed back to the pool for reuse on the next allocation of the same size.
/// If the pool has been dropped (the `Weak` upgrade fails), the inner `Arc` simply drops
/// normally, triggering `munmap + shm_unlink`.
pub(crate) struct PooledPosixShmSegment {
    pub(crate) inner: Arc<PosixShmSegment>,
    size: NonZeroUsize,
    pool: Weak<ShmSegmentPool>,
}

impl PooledPosixShmSegment {
    pub(crate) fn new(inner: Arc<PosixShmSegment>, size: NonZeroUsize, pool: Weak<ShmSegmentPool>) -> Self {
        Self { inner, size, pool }
    }
}

impl Drop for PooledPosixShmSegment {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            pool.give(self.size, self.inner.clone());
        }
        // If pool was dropped: inner Arc drops naturally → munmap + shm_unlink
    }
}
