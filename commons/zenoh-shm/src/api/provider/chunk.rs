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

use std::sync::atomic::AtomicPtr;

use crate::api::common::types::{ChunkID, SegmentID};

/// Uniquely identifies the particular chunk within particular segment
#[zenoh_macros::unstable_doc]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkDescriptor {
    pub segment: SegmentID,
    pub chunk: ChunkID,
    pub len: usize,
}

impl ChunkDescriptor {
    /// Create a new Chunk Descriptor
    #[zenoh_macros::unstable_doc]
    pub fn new(segment: SegmentID, chunk: ChunkID, len: usize) -> Self {
        Self {
            segment,
            chunk,
            len,
        }
    }
}

/// A recently-allocated chunk.
#[zenoh_macros::unstable_doc]
pub struct AllocatedChunk {
    pub descriptor: ChunkDescriptor,
    pub data: AtomicPtr<u8>,
}

impl AllocatedChunk {
    /// Create a new Allocated Chunk
    #[zenoh_macros::unstable_doc]
    pub fn new(descriptor: ChunkDescriptor, data: AtomicPtr<u8>) -> Self {
        Self { descriptor, data }
    }
}
