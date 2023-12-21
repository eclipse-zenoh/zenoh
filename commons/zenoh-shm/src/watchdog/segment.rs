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

use std::{mem::size_of, sync::atomic::AtomicU64};

use zenoh_result::ZResult;

use crate::posix_shm::segment::Segment as POSIXSegment;

use super::descriptor::SegmentID;

const WATCHDOG_SEGMENT_PREFIX: &str = "watchdog";

pub struct Segment {
    internal: POSIXSegment<SegmentID>,
}

unsafe impl Send for Segment {}
unsafe impl Sync for Segment {}

impl Segment {
    pub fn create(watchdog_count: usize) -> ZResult<Self> {
        let alloc_size = (watchdog_count + 63) / 64 * size_of::<u64>();

        let internal = POSIXSegment::create(alloc_size, WATCHDOG_SEGMENT_PREFIX)?;
        Ok(Self { internal })
    }

    pub fn open(id: SegmentID) -> ZResult<Self> {
        let internal = POSIXSegment::open(id, WATCHDOG_SEGMENT_PREFIX)?;
        Ok(Self { internal })
    }

    pub fn table_and_id(&self) -> (*const AtomicU64, SegmentID) {
        (
            self.internal.shmem.as_ptr() as *const AtomicU64,
            self.internal.id,
        )
    }

    pub fn table(&self) -> *const AtomicU64 {
        self.internal.shmem.as_ptr() as *const AtomicU64
    }

    pub fn len(&self) -> usize {
        self.internal.shmem.len()
    }
}
