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
    collections::HashSet,
    rc::Rc,
    sync::{atomic::AtomicU16, Arc, Mutex},
};

use io_uring::{opcode, squeue::Flags, SubmissionQueue};
use rand::Rng;
use zenoh_core::zlock;
use zenoh_result::ZResult;

use crate::{
    api::{reader::rx_buffer::RxBuffer, types::BufferCount},
    batch_arena::Batches,
    reader::reservable_arena::{ReservableArena, ReservableArenaInner},
    types::BufferGroupId,
};

// Absolute maximum ratings:
// As long as our buffer index is u16, we cannot have mora than 65k buffers per segment.
// As long as we want at least 2 buffers per group, we cannot have more than 32k groups.
// BufferGroupId is u16, so it can address at most 65k groups
// As the result, we fill only 50% of the addressable buffer groups space
// To save on bookkeeping, we can use random number generation to pick the next group
#[derive(Default, Debug)]
struct BufferGroupIndex {
    used: Mutex<HashSet<BufferGroupId>>,
}

impl BufferGroupIndex {
    pub fn allocate(&self) -> BufferGroupId {
        let mut lock = zlock!(self.used);

        // If we have used all possible groups, we cannot allocate a new one.
        // NOTE: Violating this rule will deadlock the algo below.
        // NOTE: If this happened, the "Absolute maximum ratings" condition is violated
        // which means hard software bug.
        if lock.len() > BufferGroupId::MAX as usize {
            panic!("BufferGroupIndex: all possible groups are used");
        }

        // generate candidate group using entropy from hint's generation
        let mut candidate_group = rand::thread_rng().gen();

        // try to find a hole in the used set starting from candidate_group
        while !lock.insert(candidate_group) {
            candidate_group = rand::thread_rng().gen();
        }

        candidate_group
    }

    pub fn free(&self, group_id: BufferGroupId) {
        zlock!(self.used).remove(&group_id);
    }
}

#[derive(Debug)]
struct GroupedArenaInner {
    arena: Arc<ReservableArenaInner>,
    index: BufferGroupIndex,
}

pub(crate) struct GroupedArena {
    inner: Rc<GroupedArenaInner>,
}

impl GroupedArena {
    pub fn new(arena: ReservableArena) -> Self {
        Self {
            inner: Rc::new(GroupedArenaInner {
                arena: arena.inner.clone(),
                index: BufferGroupIndex::default(),
            }),
        }
    }

    pub fn recycle_batch(&self, buf_id: u16) {
        self.inner.arena.recycle_batch(buf_id);
    }
}

#[derive(Debug)]
pub(crate) struct BufferGroup {
    id: BufferGroupId,
    arena: Rc<GroupedArenaInner>,
    buffers_missing: AtomicU16,
}

impl Drop for BufferGroup {
    fn drop(&mut self) {
        tracing::debug!("Freeing buffer group {}", self.id);
        self.arena.index.free(self.id);
    }
}

impl BufferGroup {
    pub(crate) fn new(
        arena: &GroupedArena,
        required_buffers_in_ring: BufferCount,
        sq: &mut SubmissionQueue<'_>,
    ) -> ZResult<Self> {
        // allocate unique group id
        let id = arena.inner.index.allocate();
        tracing::debug!("Allocated buffer group {}", id);

        let group = Self {
            id,
            arena: arena.inner.clone(),
            buffers_missing: AtomicU16::new(required_buffers_in_ring),
        };

        group.supply_batches(sq)?;

        Ok(group)
    }

    pub(crate) fn id(&self) -> BufferGroupId {
        self.id
    }

    pub(crate) fn read_buffer(
        &self,
        buf_id: u16,
        buf_len: usize,
        sq: &mut SubmissionQueue<'_>,
    ) -> ZResult<RxBuffer> {
        self.buffers_missing
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.supply_batches(sq)?;

        let data = &mut unsafe {
            self.arena
                .arena
                .arena()
                .index_mut_unchecked(buf_id as usize)
        }[0..buf_len];
        Ok(RxBuffer::new(data, buf_id, self.arena.arena.clone()))
    }

    fn supply_batches(&self, sq: &mut SubmissionQueue<'_>) -> ZResult<()> {
        // take batches from arena...
        let batches = self.arena.arena.pop_batches(
            self.buffers_missing
                .load(std::sync::atomic::Ordering::Relaxed),
        );
        // ...and provide them to the ring until we have enough buffers in the ring.
        batches
            .into_iter()
            .try_for_each(|batches| self.provide_batches_to_ring(batches, sq))
    }

    fn provide_batches_to_ring(
        &self,
        batches: Batches,
        sq: &mut SubmissionQueue<'_>,
    ) -> ZResult<()> {
        let entry = opcode::ProvideBuffers::new(
            batches.addr,
            self.arena.arena.batch_size() as i32,
            batches.nbufs,
            self.id,
            batches.start_bid,
        )
        .build()
        .flags(Flags::SKIP_SUCCESS);

        unsafe {
            sq.push(&entry)?;
        }

        self.buffers_missing
            .fetch_sub(batches.nbufs, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}
