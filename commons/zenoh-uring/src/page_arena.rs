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
    ffi::c_void,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicPtr, AtomicUsize},
};
use zenoh_result::ZResult;

use libc::{mlock, mmap, MAP_ANON, MAP_NORESERVE, MAP_PRIVATE, PROT_READ, PROT_WRITE};
use zenoh_core::bail;

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
        // reserve address space
        let ptr = {
            // TODO: attempt to use huge pages
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

        res.add_memory(size)?;

        Ok(res)
    }

    pub(crate) fn add_memory(&self, additional_size: usize) -> ZResult<*mut u8> {
        if self.size.load(std::sync::atomic::Ordering::Relaxed) + additional_size > self.capacity {
            bail!("Error allocating additional {additional_size} bytes: capacity exceeded");
        }

        unsafe {
            let addr = self
                .memory
                .load(std::sync::atomic::Ordering::Relaxed)
                .add(self.size.load(std::sync::atomic::Ordering::Relaxed));

            // prefetch and lock piece of memory in consecutive part of mmapped address space
            // TODO: in future, validate perf with mlock2 + MLOCK_ONFAULT 
            let mlock_result = mlock(addr as *mut c_void, additional_size);
            if mlock_result != 0 {
                bail!(
                    "Error allocating additional {additional_size} bytes: mlock returned {mlock_result}"
                );
            }

            self.size
                .fetch_add(additional_size, std::sync::atomic::Ordering::SeqCst);

            Ok(addr)
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
                self.size.load(std::sync::atomic::Ordering::Relaxed),
            );
        }
    }
}
