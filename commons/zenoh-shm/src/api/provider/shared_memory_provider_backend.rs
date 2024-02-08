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

use super::{chunk::ChunkDescriptor, types::ChunkAllocResult};

// The provider backend trait
// Implemet this interface to create a Zenoh-compatible shared memory provider
pub trait SharedMemoryProviderBackend {
    // Allocate the chunk of desired size
    // If successful, the result's chunk size will be >= len
    fn alloc(&mut self, len: usize) -> ChunkAllocResult;

    // Deallocate the chunk
    // It is guaranteed that chunk's len will correspond to the len returned from alloc(...)
    fn free(&mut self, chunk: &ChunkDescriptor);

    // Defragment the memory
    // Should return the size of largest defragmented chunk
    fn defragment(&mut self) -> usize;

    // Bytes available for use
    // Note: Zenoh algorithms expect O(1) complexity for this method
    fn available(&self) -> usize;
}
