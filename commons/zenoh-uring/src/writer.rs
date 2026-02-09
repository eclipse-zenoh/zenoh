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

use std::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use io_uring::{cqueue, opcode, types, IoUring};

use crate::{batch_arena::BatchArena, BUF_COUNT, BUF_SIZE};

/*
|____________________ARENA______________________|
|__Segment__|__Segment__|__Segment__|__Segment__|
|buf|buf|buf|buf|buf|buf|buf|buf|buf|buf|buf|buf|

Ringbuf reclamation happens on Segment basis

*/

/*
struct BatchSegment {
}

struct BatchStorage {
    arena: BatchArena,
    segments: Vec<Arc<BatchSegment>>
}
*/

enum WriterUserData {
    WriteFixed(u64),
}

pub struct BorrowedBuffer<'a> {
    index: u16,
    arena: &'a mut BatchArena,
}

impl<'a> Deref for BorrowedBuffer<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.arena[self.index as usize]
    }
}

impl<'a> DerefMut for BorrowedBuffer<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.arena[self.index as usize]
    }
}

impl<'a> BorrowedBuffer<'a> {
    fn new(
        arena: &'a mut BatchArena,
        available_buffers: &atomic_queue::Queue<u16>,
    ) -> Option<Self> {
        available_buffers
            .pop()
            .map(|index| Self::from_index(arena, index))
    }

    fn from_index(arena: &'a mut BatchArena, index: u16) -> Self {
        Self { index, arena }
    }
}

struct BufferPool {
    arena: UnsafeCell<BatchArena>,
    available_buffers: atomic_queue::Queue<u16>,
}

impl BufferPool {
    fn new(ring: &IoUring) -> Self {
        let mut arena = UnsafeCell::new(BatchArena::new(BUF_SIZE, BUF_COUNT));
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

    fn reuse_busy_buffer(&'_ self, index: u16) -> BorrowedBuffer<'_> {
        let mutable_arena = unsafe { &mut *self.arena.get() };
        BorrowedBuffer::from_index(mutable_arena, index)
    }

    fn try_select_available_buffer(&'_ self) -> Option<BorrowedBuffer<'_>> {
        let mutable_arena = unsafe { &mut *self.arena.get() };
        BorrowedBuffer::new(mutable_arena, &self.available_buffers)
    }
}

unsafe impl Send for BufferPool {}
unsafe impl Sync for BufferPool {}

pub struct Writer {
    ring: IoUring,
    pool: BufferPool,
}

impl Writer {
    pub fn new() -> Self {
        let ring = IoUring::builder()
        .setup_submit_all()
        //.setup_sqpoll(1)
        //.setup_sqpoll_cpu(0)
        //.setup_iopoll()
        .setup_coop_taskrun()
        //.setup_defer_taskrun()
        //.setup_single_issuer()
        .build((BUF_COUNT * 2).try_into().unwrap())
        .unwrap();

        let pool = BufferPool::new(&ring);

        Self { ring, pool }
    }

    pub fn select_buffer(&'_ self) -> BorrowedBuffer<'_> {
        loop {
            {
                let mut cq = unsafe { self.ring.completion_shared() };
                while let Some(e) = cq.next() {
                    if !cqueue::more(e.flags()) {
                        let user_data: WriterUserData =
                            unsafe { std::mem::transmute(e.user_data()) };
                        match user_data {
                            WriterUserData::WriteFixed(index) => {
                                return self.pool.reuse_busy_buffer(index as u16);
                            }
                        }
                    }
                }
            }

            if let Some(borrowed) = self.pool.try_select_available_buffer() {
                return borrowed;
            }

            //println!("Waiting for buffer!");
            //self.ring.submit().unwrap();
            self.ring.submit_and_wait(1).unwrap();
        }
    }

    pub fn write(&self, fd: types::Fd, buffer: BorrowedBuffer, actual_len: usize) {
        let user_data =
            unsafe { std::mem::transmute(WriterUserData::WriteFixed(buffer.index as u64)) };

        let send = opcode::SendZc::new(fd, buffer.deref().as_ptr() , actual_len as u32)
            .buf_index(Some(buffer.index))
            .build()
            .user_data(user_data)
            .flags(io_uring::squeue::Flags::ASYNC)
            //.flags(Flags::SKIP_SUCCESS);
            //.flags(Flags::IO_DRAIN);
            //.flags(Flags::IO_LINK)
            ;

        while unsafe { self.ring.submission_shared().push(&send) }.is_err() {
            self.ring.submitter().squeue_wait().unwrap();
        }

        //self.ring.submit_and_wait(1).unwrap();
        self.ring.submit().unwrap();
    }
}
