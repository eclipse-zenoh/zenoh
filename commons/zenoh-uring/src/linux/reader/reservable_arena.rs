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

use io_uring::{opcode, squeue::Flags};

use crate::{
    api::reader::rx_buffer::RxBuffer, batch_arena::BatchArena, reader::submission::SubmissionIface,
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

    pub fn recycle_batch(&self, buf_id: u16) {
        assert!(self.recycled_batches.push(buf_id));
    }

    pub fn pop_recycled_batch(&self) -> Option<(io_uring::squeue::Entry, usize)> {
        match self.recycled_batches.pop() {
            Some(buf_id) => {
                let data = unsafe { &mut self.arena.index_mut_unchecked(buf_id as usize)[0..1] };
                Some((
                    opcode::ProvideBuffers::new(
                        data.as_mut_ptr(),
                        self.arena.batch_size() as i32,
                        1,
                        0,
                        buf_id,
                    )
                    .build()
                    .flags(Flags::SKIP_SUCCESS),
                    1,
                ))
            }
            None => self.arena.allocate_more_batches(),
        }
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

    pub(crate) unsafe fn buffer(&self, buf_id: u16, buf_len: usize) -> RxBuffer {
        let data = &mut self.inner.arena.index_mut_unchecked(buf_id as usize)[0..buf_len];
        RxBuffer::new(data, buf_id, self.inner.clone())
    }
}
