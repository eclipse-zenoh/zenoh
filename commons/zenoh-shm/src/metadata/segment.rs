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

use std::sync::atomic::AtomicU64;

use zenoh_result::ZResult;

use super::descriptor::{MetadataIndex, MetadataSegmentID, OwnedWatchdog};
use crate::{header::chunk_header::ChunkHeaderType, posix_shm::struct_in_shm::StructInSHM};

#[derive(Debug)]
#[stabby::stabby]
pub struct Metadata<const S: usize> {
    headers: [ChunkHeaderType; S],
    watchdogs: [AtomicU64; S], // TODO: replace with (S + 63) / 64 when Rust supports it
}

// SAFETY: all fields are atomic types which are Send
// This prevents the Rust trait solver to out of recursion budget
unsafe impl<const S: usize> Send for Metadata<S> {}

impl<const S: usize> Metadata<S> {
    /// # Safety
    ///
    /// This is safe if header belongs to current Metadata instance
    pub unsafe fn fast_index_compute(&self, header: *const ChunkHeaderType) -> MetadataIndex {
        // SAFETY: same precondition as described in the function documentation.
        unsafe { header.offset_from(self.headers.as_ptr()) as MetadataIndex }
    }

    /// # Safety
    ///
    /// This is safe if index is in bounds!
    pub unsafe fn fast_elem_compute(
        &self,
        index: MetadataIndex,
    ) -> (&'static ChunkHeaderType, OwnedWatchdog) {
        let watchdog_index = index / 64;
        let watchdog_mask_index = index % 64;
        // SAFETY: same precondition as described in the function documentation.
        unsafe {
            (
                &*(self.headers.as_ptr().offset(index as isize)),
                OwnedWatchdog::new(
                    &*(self.watchdogs.as_ptr().offset(watchdog_index as isize)),
                    1u64 << watchdog_mask_index,
                ),
            )
        }
    }

    #[inline(always)]
    pub const fn count(&self) -> usize {
        S
    }
}

#[derive(Debug)]
pub struct MetadataSegment<const S: usize = 32768> {
    pub data: StructInSHM<MetadataSegmentID, Metadata<S>>,
}

impl<const S: usize> MetadataSegment<S> {
    pub fn create() -> ZResult<Self> {
        let data = StructInSHM::create()?;
        Ok(Self { data })
    }

    pub fn open(id: MetadataSegmentID) -> ZResult<Self> {
        let data = StructInSHM::open(id)?;
        Ok(Self { data })
    }
}
