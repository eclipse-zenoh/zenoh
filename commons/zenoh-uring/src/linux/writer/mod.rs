//
// Copyright (c) 2025 ZettaScale Technology
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

use std::cell::UnsafeCell;

use io_uring::IoUring;

use crate::{api::writer::BorrowedBuffer, batch_arena::BatchArena};

pub(crate) enum WriterUserData {
    WriteFixed(u64),
}

pub(crate) struct BufferPool {
    arena: UnsafeCell<BatchArena>,
    available_buffers: atomic_queue::Queue<u16>,
}

impl BufferPool {
    pub(crate) fn new(ring: &IoUring, batch_size: usize, batch_count: usize) -> Self {
        let mut arena =
            UnsafeCell::new(BatchArena::new(batch_size, batch_count, batch_count).unwrap());
        let write_buffers = arena.get_mut().register_buffers();
        unsafe { ring.submitter().register_buffers(&write_buffers).unwrap() };

        let available_buffers = atomic_queue::bounded(write_buffers.len());
        for i in 0..write_buffers.len() {
            available_buffers.push(i as u16);
        }

        Self {
            arena,
            available_buffers,
        }
    }

    pub(crate) fn reuse_busy_buffer(&'_ self, index: u16) -> BorrowedBuffer<'_> {
        let mutable_arena = unsafe { &mut *self.arena.get() };
        BorrowedBuffer::from_index(mutable_arena, index)
    }

    pub(crate) fn try_select_available_buffer(&'_ self) -> Option<BorrowedBuffer<'_>> {
        let mutable_arena = unsafe { &mut *self.arena.get() };
        BorrowedBuffer::new(mutable_arena, &self.available_buffers)
    }
}

unsafe impl Send for BufferPool {}
unsafe impl Sync for BufferPool {}
