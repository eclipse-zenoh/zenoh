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
    ops::{Deref, DerefMut},
    sync::atomic::AtomicPtr,
};

use io_uring::opcode;
use libc::{mlock, mmap, MAP_ANON, MAP_PRIVATE, PROT_READ, PROT_WRITE};

use crate::BUF_SIZE;

#[derive(Debug)]
pub(crate) struct PageArena {
    pub(crate) memory: AtomicPtr<u8>,
    size: usize,
}

impl Deref for PageArena {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe {
            std::slice::from_raw_parts(
                self.memory.load(std::sync::atomic::Ordering::Relaxed),
                self.size,
            )
        }
    }
}

impl DerefMut for PageArena {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            std::slice::from_raw_parts_mut(
                self.memory.load(std::sync::atomic::Ordering::Relaxed),
                self.size,
            )
        }
    }
}

impl PageArena {
    pub(crate) fn new(size: usize) -> Self {
        // mmap page-aligned memory
        let memory = {
            unsafe {
                let memory = mmap(
                    std::ptr::null_mut(),
                    size,
                    PROT_READ | PROT_WRITE,
                    MAP_PRIVATE | MAP_ANON,
                    -1,
                    0,
                );
                assert!(memory != libc::MAP_FAILED);

                let mlock_result = mlock(memory, size);
                assert!(mlock_result == 0);

                memory as *mut u8
            }
        };
        Self {
            memory: memory.into(),
            size,
        }
    }

    pub(crate) unsafe fn as_slice_mut_unchecked(&self) -> &'static mut [u8] {
        std::slice::from_raw_parts_mut(
            self.memory.load(std::sync::atomic::Ordering::Relaxed),
            self.size,
        )
    }

    pub(crate) fn provide_buffers(&self) -> io_uring::squeue::Entry {
        opcode::ProvideBuffers::new(
            self.memory.load(std::sync::atomic::Ordering::Relaxed),
            BUF_SIZE.try_into().unwrap(),
            (self.size / BUF_SIZE).try_into().unwrap(),
            0,
            0,
        )
        .build()
    }

    pub(crate) fn register_buffers(&self) -> Vec<libc::iovec> {
        let buf_count = (self.size / BUF_SIZE).try_into().unwrap();

        let mut bufs = Vec::with_capacity(buf_count);

        for i in 0..buf_count {
            let ptr = unsafe {
                self.memory
                    .load(std::sync::atomic::Ordering::Relaxed)
                    .add(i * BUF_SIZE)
            };
            bufs.push(libc::iovec {
                iov_base: ptr as *mut libc::c_void,
                iov_len: BUF_SIZE,
            });
        }

        bufs
    }
}

impl Drop for PageArena {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(
                self.memory.load(std::sync::atomic::Ordering::Relaxed) as *mut libc::c_void,
                self.size,
            );
        }
    }
}
