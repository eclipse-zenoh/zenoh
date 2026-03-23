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
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, RwLock},
};

use crossbeam_queue::ArrayQueue;

use crate::api::protocol_implementations::posix::posix_shm_segment::PosixShmSegment;

/// Per-size LIFO pool of idle SHM segments.
///
/// Segments are keyed by their exact byte count. When a segment is returned (`give`),
/// it is pushed onto the per-size queue. When a segment is requested (`take`), the
/// most-recently-returned segment of that exact size is popped out — keeping TLB and
/// page-cache entries hot.
///
/// The hot path (`take` / `give` after warmup) requires only a read lock on the bucket
/// map (shared, non-blocking) and a single CAS on the [`ArrayQueue`] — no mutex waiting
/// even under concurrent access from multiple streams.
///
/// When the bucket is at capacity, `give` simply drops the segment, triggering the
/// normal `munmap + shm_unlink` path (backpressure path only).
pub(crate) struct ShmSegmentPool {
    /// Map from exact segment size → bounded lock-free MPMC queue.
    ///
    /// New entries are inserted lazily under a write lock; after warmup the map is
    /// stable and all accesses use the read lock (non-blocking, concurrent).
    buckets: RwLock<HashMap<NonZeroUsize, Arc<ArrayQueue<Arc<PosixShmSegment>>>>>,
    max_idle: usize,
}

impl ShmSegmentPool {
    /// Create a new pool with the given per-size idle capacity.
    pub(crate) fn new(max_idle: usize) -> Self {
        Self {
            buckets: RwLock::new(HashMap::new()),
            max_idle,
        }
    }

    /// Pop the most-recently-returned segment of exactly `size` bytes.
    /// Returns `None` if the bucket is empty.
    pub(crate) fn take(&self, size: NonZeroUsize) -> Option<Arc<PosixShmSegment>> {
        // Read path: shared lock, non-blocking.
        self.buckets.read().unwrap().get(&size)?.pop()
    }

    /// Return a segment to the pool. Drops (→ `munmap + shm_unlink`) if the bucket
    /// is already at capacity.
    pub(crate) fn give(&self, size: NonZeroUsize, seg: Arc<PosixShmSegment>) {
        // Fast path: bucket already exists — shared read lock only.
        {
            let buckets = self.buckets.read().unwrap();
            if let Some(queue) = buckets.get(&size) {
                let _ = queue.push(seg);
                // ArrayQueue::push returns Err(seg) when full → seg drops → munmap
                return;
            }
        }

        // Slow path: first segment of this size — insert a new queue under write lock.
        let mut buckets = self.buckets.write().unwrap();
        // Re-check after acquiring write lock (another thread may have inserted first).
        let queue = buckets
            .entry(size)
            .or_insert_with(|| Arc::new(ArrayQueue::new(self.max_idle)));
        let _ = queue.push(seg);
    }
}
