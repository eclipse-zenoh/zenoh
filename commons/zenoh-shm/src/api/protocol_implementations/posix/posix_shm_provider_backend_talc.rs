//
// Copyright (c) 2025 ZettaScale Technology
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

use std::{
    alloc::Layout,
    borrow::Borrow,
    ptr::NonNull,
    slice,
    sync::{Arc, Mutex},
};

use talc::{ErrOnOom, Talc};
use zenoh_core::{zlock, Resolvable, Wait};
use zenoh_result::ZResult;

use super::posix_shm_segment::PosixShmSegment;
use crate::api::{
    common::{
        types::{ChunkID, ProtocolID, PtrInSegment},
        with_id::WithProtocolID,
    },
    protocol_implementations::posix::protocol_id::POSIX_PROTOCOL_ID,
    provider::{
        chunk::{AllocatedChunk, ChunkDescriptor},
        shm_provider_backend::ShmProviderBackend,
        types::{AllocAlignment, ChunkAllocResult, MemoryLayout, ZAllocError, ZLayoutError},
    },
};

/// Builder to create posix SHM provider
#[zenoh_macros::unstable_doc]
pub struct PosixShmProviderBackendTalcBuilder;

impl PosixShmProviderBackendTalcBuilder {
    /// Use existing layout
    #[zenoh_macros::unstable_doc]
    pub fn with_layout<Layout: Borrow<MemoryLayout>>(
        self,
        layout: Layout,
    ) -> LayoutedPosixShmProviderBackendTalcBuilder<Layout> {
        LayoutedPosixShmProviderBackendTalcBuilder { layout }
    }

    /// Construct layout in-place using arguments
    #[zenoh_macros::unstable_doc]
    pub fn with_layout_args(
        self,
        size: usize,
        alignment: AllocAlignment,
    ) -> Result<LayoutedPosixShmProviderBackendTalcBuilder<MemoryLayout>, ZLayoutError> {
        let layout = MemoryLayout::new(size, alignment)?;
        Ok(LayoutedPosixShmProviderBackendTalcBuilder { layout })
    }

    /// Construct layout in-place from size (default alignment will be used)
    #[zenoh_macros::unstable_doc]
    pub fn with_size(
        self,
        size: usize,
    ) -> LayoutedPosixShmProviderBackendTalcBuilder<MemoryLayout> {
        // `unwrap` here should never fail. If it fails - check that the default alignment is 1
        let layout = MemoryLayout::new(size, AllocAlignment::default()).unwrap();
        LayoutedPosixShmProviderBackendTalcBuilder { layout }
    }
}

#[zenoh_macros::unstable_doc]
pub struct LayoutedPosixShmProviderBackendTalcBuilder<Layout: Borrow<MemoryLayout>> {
    layout: Layout,
}

#[zenoh_macros::unstable_doc]
impl<Layout: Borrow<MemoryLayout>> Resolvable
    for LayoutedPosixShmProviderBackendTalcBuilder<Layout>
{
    type To = ZResult<PosixShmProviderBackendTalc>;
}

#[zenoh_macros::unstable_doc]
impl<Layout: Borrow<MemoryLayout>> Wait for LayoutedPosixShmProviderBackendTalcBuilder<Layout> {
    fn wait(self) -> <Self as Resolvable>::To {
        PosixShmProviderBackendTalc::new(self.layout.borrow())
    }
}

/// A talc backend based on POSIX shared memory.
/// This is the default general-purpose backend shipped with Zenoh.
/// Talc allocator provides great performnce (2nd after `buddy_system_allocator`) while maintaining
/// excellent fragmentation resistance and memory utilization efficiency.
#[zenoh_macros::unstable_doc]
pub struct PosixShmProviderBackendTalc {
    segment: Arc<PosixShmSegment>,
    talc: Mutex<Talc<ErrOnOom>>,
    alignment: AllocAlignment,
}

impl PosixShmProviderBackendTalc {
    /// Get the builder to construct a new instance
    #[zenoh_macros::unstable_doc]
    pub fn builder() -> PosixShmProviderBackendTalcBuilder {
        PosixShmProviderBackendTalcBuilder
    }

    fn new(layout: &MemoryLayout) -> ZResult<Self> {
        let segment = Arc::new(PosixShmSegment::create(layout.size())?);

        // because of platform specific, our shm segment is >= requested size, so in order to utilize
        // additional memory we re-layout the size
        let real_size = segment.segment.elem_count().get();
        let ptr = unsafe { segment.segment.elem_mut(0) };

        let mut talc = Talc::new(ErrOnOom);

        unsafe {
            talc.claim(slice::from_raw_parts_mut(ptr, real_size).into())
                .map_err(|_| "Error initializing Talc backend!")?;
        }

        tracing::trace!(
            "Created PosixShmProviderBackendTalc id {}, layout {:?}",
            segment.segment.id(),
            layout
        );

        Ok(Self {
            segment,
            talc: Mutex::new(talc),
            alignment: layout.alignment(),
        })
    }
}

impl WithProtocolID for PosixShmProviderBackendTalc {
    fn id(&self) -> ProtocolID {
        POSIX_PROTOCOL_ID
    }
}

impl ShmProviderBackend for PosixShmProviderBackendTalc {
    fn alloc(&self, layout: &MemoryLayout) -> ChunkAllocResult {
        tracing::trace!("PosixShmProviderBackendTalc::alloc({:?})", layout);

        let alloc_layout = unsafe {
            Layout::from_size_align_unchecked(
                layout.size().get(),
                layout.alignment().get_alignment_value().get(),
            )
        };

        let alloc = {
            let mut lock = zlock!(self.talc);
            unsafe { lock.malloc(alloc_layout) }
        };

        match alloc {
            Ok(buf) => {
                let descriptor = {
                    let chunk_id = unsafe { self.segment.segment.index(buf.as_ptr()) } as ChunkID;
                    let size = layout.size();
                    ChunkDescriptor::new(self.segment.segment.id(), chunk_id, size)
                };

                let data = PtrInSegment::new(buf.as_ptr(), self.segment.clone());

                Ok(AllocatedChunk { descriptor, data })
            }
            Err(_) => Err(ZAllocError::OutOfMemory),
        }
    }

    fn free(&self, chunk: &ChunkDescriptor) {
        let alloc_layout = unsafe {
            Layout::from_size_align_unchecked(
                chunk.len.get(),
                self.alignment.get_alignment_value().get(),
            )
        };

        let ptr = unsafe { self.segment.segment.elem_mut(chunk.chunk) };

        unsafe { zlock!(self.talc).free(NonNull::new_unchecked(ptr), alloc_layout) };
    }

    fn defragment(&self) -> usize {
        0
    }

    fn available(&self) -> usize {
        0
    }

    fn layout_for(&self, layout: MemoryLayout) -> Result<MemoryLayout, ZLayoutError> {
        layout.extend(self.alignment)
    }
}
