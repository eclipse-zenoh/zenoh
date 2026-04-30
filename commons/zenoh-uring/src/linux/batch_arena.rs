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

use zenoh_result::ZResult;

use super::page_arena::PageArena;
use crate::{api::types::BufferCount, types::BufferId};

pub(crate) struct Batches {
    pub addr: *mut u8,
    pub nbufs: BufferCount,
    pub start_bid: BufferId,
}

impl Batches {
    pub fn split(self, count: BufferCount, batch_size: usize) -> (Self, Option<Self>) {
        if count >= self.nbufs {
            (self, None)
        } else {
            let remaining = Self {
                addr: unsafe { self.addr.add(count as usize * batch_size) },
                nbufs: self.nbufs - count,
                start_bid: self.start_bid + count,
            };
            let allocated = Self {
                addr: self.addr,
                nbufs: count,
                start_bid: self.start_bid,
            };
            (allocated, Some(remaining))
        }
    }
}

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
        batch_count: BufferCount,
        max_batch_count: BufferCount,
    ) -> ZResult<Self> {
        let size = batch_size * batch_count as usize;
        let capacity = batch_size * max_batch_count as usize;
        let arena = PageArena::new(size, capacity)?;
        Ok(Self { arena, batch_size })
    }

    pub(crate) fn allocate_more_batches(&self) -> Option<Batches> {
        tracing::debug!("Add batches");

        let (addr, size) = self
            .arena
            .add_memory(self.arena.size.load(std::sync::atomic::Ordering::Relaxed))?;
        let additional_batch_count = size / self.batch_size;
        if additional_batch_count == 0 {
            return None;
        }

        let bid = unsafe {
            addr.byte_offset_from(self.arena.memory.load(std::sync::atomic::Ordering::Relaxed))
        } as usize
            / self.batch_size;

        Some(Batches {
            addr,
            nbufs: additional_batch_count as BufferCount,
            start_bid: bid as BufferId,
        })
    }

    pub(crate) fn batch_count(&self) -> usize {
        self.arena.size.load(std::sync::atomic::Ordering::Relaxed) / self.batch_size
    }

    pub(crate) unsafe fn index_mut_unchecked(&self, index: usize) -> &'static mut [u8] {
        let start = index * self.batch_size;
        let end = start + self.batch_size;
        &mut self.arena.as_slice_mut_unchecked()[start..end]
    }

    pub(crate) fn register_buffers(&self) -> Vec<libc::iovec> {
        let batch_count = self.batch_count();

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

    pub(crate) fn batch_size(&self) -> usize {
        self.batch_size
    }
}
