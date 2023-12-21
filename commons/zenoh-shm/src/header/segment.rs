//
// Copyright (c) 2023 ZettaScale Technology
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

use std::mem::size_of;

use zenoh_result::{bail, ZResult};

use crate::posix_shm::segment::Segment as POSIXSegment;

use super::{
    chunk_header::ChunkHeaderType,
    descriptor::{HeaderIndex, HeaderSegmentID},
};

const HEADER_SEGMENT_PREFIX: &str = "header";

pub struct HeaderSegment {
    internal: POSIXSegment<HeaderSegmentID>,
}

unsafe impl Send for HeaderSegment {}
unsafe impl Sync for HeaderSegment {}

impl HeaderSegment {
    pub fn create(header_count: usize) -> ZResult<Self> {
        if header_count > HeaderIndex::MAX as usize + 1 {
            bail!("Unable to create header segment of {header_count} headers: length is out of range for HeaderIndex!")
        }

        let alloc_size = header_count * size_of::<ChunkHeaderType>();

        let internal = POSIXSegment::create(alloc_size, HEADER_SEGMENT_PREFIX)?;
        Ok(Self { internal })
    }

    pub fn open(id: HeaderSegmentID) -> ZResult<Self> {
        let internal = POSIXSegment::open(id, HEADER_SEGMENT_PREFIX)?;
        Ok(Self { internal })
    }

    pub fn table_and_id(&self) -> (*const ChunkHeaderType, HeaderSegmentID) {
        (
            self.internal.shmem.as_ptr() as *const ChunkHeaderType,
            self.internal.id,
        )
    }

    /// # Safety
    /// This is safe only if the index belongs to the current segment
    pub unsafe fn header(&self, index: HeaderIndex) -> *const ChunkHeaderType {
        (self.internal.shmem.as_ptr() as *const ChunkHeaderType).add(index as usize)
    }
}
