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

use std::sync::Arc;

use crate::{
    api::types::BufferCount,
    batch_arena::{BatchArena, Batches},
    reader::submission::SubmissionIface,
    types::BufferId,
};

pub(crate) struct ReservableArenaInner {
    arena: BatchArena,
    submitter: SubmissionIface,
    recycled_batches: atomic_queue::Queue<u16>,
}

impl std::fmt::Debug for ReservableArenaInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReservableArenaInner")
            .field("arena", &self.arena)
            .field("submitter", &self.submitter)
            .finish()
    }
}

impl ReservableArenaInner {
    fn new(arena: BatchArena, submitter: SubmissionIface) -> Self {
        let recycled_batches = atomic_queue::Queue::new(u16::MAX as usize);
        Self {
            arena,
            submitter,
            recycled_batches,
        }
    }

    pub fn arena(&self) -> &BatchArena {
        &self.arena
    }

    pub(crate) fn batch_size(&self) -> usize {
        self.arena.batch_size()
    }

    pub fn pop_batches(&self, count: BufferCount) -> Vec<Batches> {
        let mut result = Vec::with_capacity(count as usize);

        // recycle batches from the recycled_batches queue first
        while let Some(buf_id) = self.recycled_batches.pop() {
            let data = unsafe { self.arena.index_mut_unchecked(buf_id as usize) };
            result.push(Batches {
                addr: data.as_mut_ptr(),
                nbufs: 1,
                start_bid: buf_id as BufferId,
            });
            if result.len() == count as usize {
                break;
            }
        }

        // allocate more memory if needed
        let batches_to_allocate = count as usize - result.len();
        if batches_to_allocate > 0 {
            if let Some(additional_batches) = self.arena.allocate_more_batches() {
                let (primary, to_recycle) = additional_batches
                    .split(batches_to_allocate as BufferCount, self.arena.batch_size());

                // push the primary batch to the result
                result.push(primary);

                // recycle the leftover batches
                if let Some(to_recycle) = to_recycle {
                    for buf_id in to_recycle.start_bid..to_recycle.start_bid + to_recycle.nbufs {
                        self.recycle_batch(buf_id);
                    }
                }
            }
        }

        result
    }

    pub fn recycle_batch(&self, buf_id: u16) {
        assert!(self.recycled_batches.push(buf_id));
    }
}

pub(crate) struct ReservableArena {
    pub(crate) inner: Arc<ReservableArenaInner>,
}

impl ReservableArena {
    pub fn new(arena: BatchArena, submitter: SubmissionIface) -> Self {
        let inner = Arc::new(ReservableArenaInner::new(arena, submitter));
        Self { inner }
    }
}
