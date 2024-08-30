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

use std::{num::NonZeroUsize, sync::atomic::AtomicPtr};

use zenoh_result::ZResult;

use crate::{
    api::{
        client::shm_segment::ShmSegment,
        common::types::{ChunkID, SegmentID},
    },
    posix_shm::array::ArrayInSHM,
};

const POSIX_SHM_SEGMENT_PREFIX: &str = "posix_shm_provider_segment";

#[derive(Debug)]
pub(crate) struct PosixShmSegment {
    pub(crate) segment: ArrayInSHM<SegmentID, u8, ChunkID>,
}

impl PosixShmSegment {
    pub(crate) fn create(alloc_size: NonZeroUsize) -> ZResult<Self> {
        let segment = ArrayInSHM::create(alloc_size.get(), POSIX_SHM_SEGMENT_PREFIX)?;
        Ok(Self { segment })
    }

    pub(crate) fn open(id: SegmentID) -> ZResult<Self> {
        let segment = ArrayInSHM::open(id, POSIX_SHM_SEGMENT_PREFIX)?;
        Ok(Self { segment })
    }
}

impl ShmSegment for PosixShmSegment {
    fn map(&self, chunk: ChunkID) -> ZResult<AtomicPtr<u8>> {
        unsafe { Ok(AtomicPtr::new(self.segment.elem_mut(chunk))) }
    }
}
