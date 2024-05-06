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

use std::sync::atomic::AtomicU64;

use zenoh_result::ZResult;

use super::descriptor::SegmentID;
use crate::posix_shm::array::ArrayInSHM;

const WATCHDOG_SEGMENT_PREFIX: &str = "watchdog";

#[derive(Debug)]
pub struct Segment {
    pub array: ArrayInSHM<SegmentID, AtomicU64, u32>,
}

impl Segment {
    pub fn create(watchdog_count: usize) -> ZResult<Self> {
        let elem_count = (watchdog_count + 63) / 64;
        let array = ArrayInSHM::create(elem_count, WATCHDOG_SEGMENT_PREFIX)?;
        Ok(Self { array })
    }

    pub fn open(id: SegmentID) -> ZResult<Self> {
        let array = ArrayInSHM::open(id, WATCHDOG_SEGMENT_PREFIX)?;
        Ok(Self { array })
    }
}
