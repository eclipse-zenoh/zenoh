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
    chunk::ChunkDescriptor,
    types::{ChunkAllocResult, MemoryLayout},
};

/// The provider backend trait
/// Implemet this interface to create a Zenoh-compatible shared memory provider
pub trait SharedMemoryProviderBackend {
    /// Allocate the chunk of desired size.
    /// If successful, the result's chunk size will be >= len
    fn alloc(&self, layout: &MemoryLayout) -> ChunkAllocResult;

    /// Deallocate the chunk.
    /// It is guaranteed that chunk's descriptor will correspond to the one returned from alloc(...)
    fn free(&self, chunk: &ChunkDescriptor);

    /// Defragment the memory.
    /// Should return the size of largest defragmented chunk
    fn defragment(&self) -> usize;

    /// Bytes available for use
    fn available(&self) -> usize;

    /// Check and calculate suitable layout for layout.
    /// Depending on the implementation, backend may relayout allocations for bigger layouts.
    /// This method is used to:
    /// - validate, if the provided layout can be used with this backend
    /// - adopt the layout for backend capabilities
    fn layout_for(&self, layout: MemoryLayout) -> ZResult<MemoryLayout>;
}
