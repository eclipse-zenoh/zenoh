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

use std::ops::{Index, IndexMut};

use io_uring::opcode;

use crate::page_arena::PageArena;

use zenoh_result::ZResult;

#[derive(Debug)]
pub(crate) struct BatchArena {
    arena: PageArena,
    batch_size: usize,
}

impl Index<usize> for BatchArena {
    type Output = [u8];

    fn index(&self, index: usize) -> &Self::Output {
        let start = index * self.batch_size;
        let end = start + self.batch_size;
        &self.arena[start..end]
    }
}

impl IndexMut<usize> for BatchArena {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let start = index * self.batch_size;
        let end = start + self.batch_size;
        &mut self.arena[start..end]
    }
}

impl BatchArena {
    pub(crate) fn new(
        batch_size: usize,
        batch_count: usize,
        max_batch_count: usize,
    ) -> ZResult<Self> {
        let size = batch_size * batch_count;
        let capacity = batch_size * max_batch_count;
        let arena = PageArena::new(size, capacity)?;
        Ok(Self { arena, batch_size })
    }

    pub(crate) fn allocate_more_batches(
        &self,
        additional_batch_count: usize,
    ) -> ZResult<io_uring::squeue::Entry> {
        //println!("Add batches");

        let size = self.batch_size * additional_batch_count;
        let addr = self.arena.add_memory(size)?;

        let bid = unsafe {
            addr.byte_offset_from(self.arena.memory.load(std::sync::atomic::Ordering::Relaxed))
        } as usize / self.batch_size;

        Ok(opcode::ProvideBuffers::new(
            addr,
            self.batch_size as i32,
            additional_batch_count.try_into().unwrap(),
            0,
            bid as u16,
        )
        .build())
    }

    pub(crate) fn batch_count(&self) -> usize {
        self.arena.size.load(std::sync::atomic::Ordering::Relaxed) / self.batch_size
    }

    pub(crate) unsafe fn index_mut_unchecked(&self, index: usize) -> &'static mut [u8] {
        let start = index * self.batch_size;
        let end = start + self.batch_size;
        &mut self.arena.as_slice_mut_unchecked()[start..end]
    }

    pub(crate) fn provide_root_buffers(&self) -> io_uring::squeue::Entry {
        opcode::ProvideBuffers::new(
            self.arena.memory.load(std::sync::atomic::Ordering::Relaxed),
            self.batch_size as i32,
            self.batch_count().try_into().unwrap(),
            0,
            0,
        )
        .build()
    }

    pub(crate) fn register_buffers(&self) -> Vec<libc::iovec> {
        let batch_count = self.batch_count().try_into().unwrap();

        let mut batches = Vec::with_capacity(batch_count);

        for i in 0..batch_count {
            let ptr = unsafe {
                self.arena
                    .memory
                    .load(std::sync::atomic::Ordering::Relaxed)
                    .add(i * self.batch_size)
            };
            batches.push(libc::iovec {
                iov_base: ptr as *mut libc::c_void,
                iov_len: self.batch_size,
            });
        }

        batches
    }
}
