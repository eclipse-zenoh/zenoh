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

use zenoh_result::ZResult;

use crate::posix_shm::segment::Segment;

const DATA_SEGMENT_PREFIX: &str = "data";

pub type DataSegmentID = u16;

pub struct DataSegment {
    pub segment: Segment<DataSegmentID>,
}

impl DataSegment {
    pub fn create(alloc_size: usize) -> ZResult<Self> {
        let segment = Segment::create(alloc_size, DATA_SEGMENT_PREFIX)?;
        Ok(Self { segment })
    }

    pub fn open(id: DataSegmentID) -> ZResult<Self> {
        let segment = Segment::open(id, DATA_SEGMENT_PREFIX)?;
        Ok(Self { segment })
    }
}
