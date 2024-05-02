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

use super::{
    chunk_header::ChunkHeaderType,
    descriptor::{HeaderIndex, HeaderSegmentID},
};
use crate::posix_shm::array::ArrayInSHM;

const HEADER_SEGMENT_PREFIX: &str = "header";

pub struct HeaderSegment {
    pub array: ArrayInSHM<HeaderSegmentID, ChunkHeaderType, HeaderIndex>,
}

impl HeaderSegment {
    pub fn create(header_count: usize) -> ZResult<Self> {
        let array = ArrayInSHM::create(header_count, HEADER_SEGMENT_PREFIX)?;
        Ok(Self { array })
    }

    pub fn open(id: HeaderSegmentID) -> ZResult<Self> {
        let array = ArrayInSHM::open(id, HEADER_SEGMENT_PREFIX)?;
        Ok(Self { array })
    }
}
