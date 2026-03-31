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
    cmp::min,
    ffi::c_void,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicPtr, AtomicUsize},
};

use libc::{mlock, mmap, MAP_ANON, MAP_NORESERVE, MAP_PRIVATE, PROT_READ, PROT_WRITE};
use zenoh_core::{bail, zerror};
use zenoh_result::ZResult;

#[derive(Debug)]
pub(crate) struct PageArena {
    pub(crate) memory: AtomicPtr<u8>,
    pub(crate) size: AtomicUsize,
    capacity: usize,
}

impl Deref for PageArena {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe {
            std::slice::from_raw_parts(
                self.memory.load(std::sync::atomic::Ordering::Relaxed),
                self.size.load(std::sync::atomic::Ordering::Relaxed),
            )
        }
    }
}

impl DerefMut for PageArena {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            std::slice::from_raw_parts_mut(
                self.memory.load(std::sync::atomic::Ordering::Relaxed),
                self.size.load(std::sync::atomic::Ordering::Relaxed),
            )
        }
    }
}

impl PageArena {
    pub(crate) fn new(size: usize, capacity: usize) -> ZResult<Self> {
        // reserve contagious address space without allocating phy pages!
        let ptr = {
            // TODO: probe to use 2MB huge pages and fallback to this if unsupported
            let ptr = unsafe {
                mmap(
                    std::ptr::null_mut(),
                    capacity,
                    PROT_READ | PROT_WRITE,
                    MAP_PRIVATE | MAP_ANON | MAP_NORESERVE,
                    -1,
                    0,
                )
            };
            if ptr == libc::MAP_FAILED {
                bail!(
                    "Error reserving address space of {capacity} bytes: mmap returned MAP_FAILED"
                );
            }

            ptr as *mut u8
        };

        let res = Self {
            memory: ptr.into(),
            size: AtomicUsize::new(0),
            capacity,
        };

        res.add_memory(size)
            .ok_or(zerror!("Unable to reserve initial {size} bytes"))?;

        Ok(res)
    }

    pub(crate) fn add_memory(&self, desired_additional_size: usize) -> Option<(*mut u8, usize)> {
        let size = self.size.load(std::sync::atomic::Ordering::Relaxed);

        if size == self.capacity {
            tracing::debug!(
                "Unable to reserve additional bytes in physical memory: capacity exceeded"
            );
            return None;
        }

        let real_new_size = min(size + desired_additional_size, self.capacity);
        let real_additional_size = real_new_size - size;

        unsafe {
            let addr = self
                .memory
                .load(std::sync::atomic::Ordering::Relaxed)
                .add(size);

            // prefetch and lock piece of memory in consecutive part of mmapped address space
            // TODO: in future, validate perf with mlock2 + MLOCK_ONFAULT
            let mlock_result = mlock(addr as *mut c_void, real_additional_size);
            if mlock_result != 0 {
                tracing::error!(
                    "Error reserving additional {real_additional_size} bytes in physical memory: mlock returned {mlock_result}"
                );
                return None;
            }

            self.size
                .fetch_add(real_additional_size, std::sync::atomic::Ordering::SeqCst);

            Some((addr, real_additional_size))
        }
    }

    pub(crate) unsafe fn as_slice_mut_unchecked(&self) -> &'static mut [u8] {
        std::slice::from_raw_parts_mut(
            self.memory.load(std::sync::atomic::Ordering::Relaxed),
            self.size.load(std::sync::atomic::Ordering::Relaxed),
        )
    }
}

impl Drop for PageArena {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(
                self.memory.load(std::sync::atomic::Ordering::Relaxed) as *mut libc::c_void,
                self.capacity,
            );
        }
    }
}
