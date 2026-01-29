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

use crate::{page_arena::PageArena, BUF_SIZE};

#[derive(Debug)]
pub(crate) struct BatchArena {
    arena: PageArena,
}

impl Index<usize> for BatchArena {
    type Output = [u8];

    fn index(&self, index: usize) -> &Self::Output {
        let start = index * BUF_SIZE;
        let end = start + BUF_SIZE;
        &self.arena[start..end]
    }
}

impl IndexMut<usize> for BatchArena {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let start = index * BUF_SIZE;
        let end = start + BUF_SIZE;
        &mut self.arena[start..end]
    }
}

impl BatchArena {
    pub(crate) fn new(buf_count: usize) -> Self {
        let size = BUF_SIZE * buf_count;
        let arena = PageArena::new(size);
        Self { arena }
    }

    pub(crate) unsafe fn index_mut_unchecked(&self, index: usize) -> &'static mut [u8] {
        let start = index * BUF_SIZE;
        let end = start + BUF_SIZE;
        &mut self.arena.as_slice_mut_unchecked()[start..end]
    }

    pub(crate) fn provide_root_buffers(&self) -> io_uring::squeue::Entry {
        self.arena.provide_buffers()
    }

    pub(crate) fn register_buffers(&self) -> Vec<libc::iovec> {
        self.arena.register_buffers()
    }
}
