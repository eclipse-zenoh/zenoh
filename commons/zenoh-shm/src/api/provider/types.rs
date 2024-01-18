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

use crate::SharedMemoryBuf;

use super::chunk::AllocatedChunk;

// Allocation errors enum
pub enum AllocError {
    NeedDefragment,              // defragmentation needed
    OutOfMemory,                 // the provider is out of memory
    Other(zenoh_result::Error),  // other error
}

impl From<zenoh_result::Error> for AllocError {
    fn from(value: zenoh_result::Error) -> Self {
        Self::Other(value)
    }
}

pub type ChunkAllocResult = Result<AllocatedChunk, AllocError>;
pub type BufAllocResult = Result<SharedMemoryBuf, AllocError>;
