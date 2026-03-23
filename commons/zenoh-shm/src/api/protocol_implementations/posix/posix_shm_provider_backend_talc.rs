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
    ptr::NonNull,
    slice,
    sync::{Arc, Mutex},
};

use talc::{ErrOnOom, Talc};
use zenoh_core::{zlock, Resolvable, Wait};
use zenoh_result::ZResult;

use super::posix_shm_segment::PosixShmSegment;
use crate::{
    api::{
        common::{types::ProtocolID, with_id::WithProtocolID},
        protocol_implementations::posix::protocol_id::POSIX_PROTOCOL_ID,
        provider::{
            chunk::ChunkDescriptor,
            memory_layout::MemoryLayout,
            shm_provider_backend::ShmProviderBackend,
            types::{AllocAlignment, ChunkAllocResult, ZAllocError, ZLayoutError},
        },
    },
    posix_shm::pool::ShmSegmentPool,
};

/// Configuration for the SHM segment pool attached to a [`PosixShmProviderBackendTalc`].
///
/// The pool keeps idle SHM segments alive and returns them on the next allocation of the
/// same size, eliminating `shm_open + mmap + mlock` / `munmap + shm_unlink` overhead on
/// the hot publish path.
#[zenoh_macros::unstable_doc]
#[derive(Clone, Debug)]
pub struct ShmPoolConfig {
    /// Maximum number of idle segments retained per exact byte-size bucket. Default: `8`.
    ///
    /// At 1 MB payloads this caps resident memory at `8 MB` per provider instance. For
    /// steady-state ping-pong at `N` concurrent streams at most `N` idle segments are
    /// needed, so `8` covers typical workloads with headroom.
    pub max_idle_per_size: usize,
}

impl Default for ShmPoolConfig {
    fn default() -> Self {
        Self {
            max_idle_per_size: 8,
        }
    }
}

/// Builder to create posix SHM provider
#[zenoh_macros::unstable_doc]
pub struct PosixShmProviderBackendTalcBuilder<Layout> {
    layout: Layout,
    pool_config: Option<ShmPoolConfig>,
}

#[zenoh_macros::unstable_doc]
impl<Layout> Resolvable for PosixShmProviderBackendTalcBuilder<Layout> {
    type To = ZResult<PosixShmProviderBackendTalc>;
}

#[zenoh_macros::unstable_doc]
impl<Layout: TryInto<MemoryLayout>> Wait for PosixShmProviderBackendTalcBuilder<Layout>
where
    Layout::Error: Into<ZLayoutError>,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let layout = self.layout.try_into().map_err(Into::into)?;
        PosixShmProviderBackendTalc::new(&layout, self.pool_config)
    }
}

/// A talc backend based on POSIX shared memory.
///
/// This is the default general-purpose backend shipped with Zenoh.
/// Talc allocator provides great performnce (2nd after `buddy_system_allocator`) while maintaining
/// excellent fragmentation resistance and memory utilization efficiency.
///
/// When a [`ShmPoolConfig`] is provided (via [`builder_with_pool`]), each allocation creates a
/// whole dedicated SHM segment of exactly the requested size and returns it to the pool on
/// release, eliminating per-message `shm_open + mmap` / `munmap + shm_unlink` cycles.
/// When the pool is not configured, the traditional Talc sub-allocation path is used.
#[zenoh_macros::unstable_doc]
pub struct PosixShmProviderBackendTalc {
    segment: Arc<PosixShmSegment>,
    talc: Mutex<Talc<ErrOnOom>>,
    alignment: AllocAlignment,
    pool: Option<Arc<ShmSegmentPool>>,
}

impl PosixShmProviderBackendTalc {
    /// Get the builder to construct a new instance using the traditional Talc sub-allocation path.
    ///
    /// No segment pool is configured; the traditional Talc sub-allocation within a single
    /// pre-allocated SHM segment is used.
    #[zenoh_macros::unstable_doc]
    pub fn builder<Layout>(layout: Layout) -> PosixShmProviderBackendTalcBuilder<Layout> {
        PosixShmProviderBackendTalcBuilder {
            layout,
            pool_config: None,
        }
    }

    /// Get the builder to construct a new instance with segment pooling enabled.
    ///
    /// Pooling eliminates per-message `shm_open + mmap` / `munmap + shm_unlink` overhead
    /// by retaining idle segments and reusing them on subsequent allocations of the same size.
    #[zenoh_macros::unstable_doc]
    pub fn builder_with_pool<Layout>(
        layout: Layout,
        config: ShmPoolConfig,
    ) -> PosixShmProviderBackendTalcBuilder<Layout> {
        PosixShmProviderBackendTalcBuilder {
            layout,
            pool_config: Some(config),
        }
    }

    fn new(layout: &MemoryLayout, pool_config: Option<ShmPoolConfig>) -> ZResult<Self> {
        let segment = Arc::new(PosixShmSegment::create(layout.size())?);

        // because of platform specific, our shm segment is >= requested size, so in order to utilize
        // additional memory we re-layout the size
        let real_size = segment.segment.elem_count().get();
        // SAFETY: the segment is guaranteed to be valid and the index 0 is always valid.
        let ptr = unsafe { segment.segment.elem_mut(0) };

        let mut talc = Talc::new(ErrOnOom);

        // SAFETY: the pointer and size are guaranteed to be valid as they represent the whole segment.
        unsafe {
            talc.claim(slice::from_raw_parts_mut(ptr, real_size).into())
                .map_err(|_| "Error initializing Talc backend!")?;
        }

        tracing::trace!(
            "Created PosixShmProviderBackendTalc id {}, layout {:?}",
            segment.segment.id(),
            layout
        );

        let pool = pool_config.map(|cfg| Arc::new(ShmSegmentPool::new(cfg.max_idle_per_size)));

        Ok(Self {
            segment,
            talc: Mutex::new(talc),
            alignment: layout.alignment(),
            pool,
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

        // Pool path: each allocation gets a whole dedicated SHM segment, keyed by exact size.
        // The segment is returned to the pool when the last ZSlice referencing it drops.
        if let Some(pool) = &self.pool {
            let segment = if let Some(seg) = pool.take(layout.size()) {
                tracing::trace!(
                    "PosixShmProviderBackendTalc: reusing pooled segment id {}",
                    seg.segment.id()
                );
                seg
            } else {
                Arc::new(
                    PosixShmSegment::create(layout.size()).map_err(|_| ZAllocError::OutOfMemory)?,
                )
            };

            // SAFETY: elem_mut(0) is always valid for a freshly-created or freshly-pooled segment.
            let buf = unsafe { NonNull::new_unchecked(segment.segment.elem_mut(0)) };
            return Ok(segment.allocated_chunk_pooled(
                buf,
                layout,
                Arc::downgrade(pool),
                layout.size(),
            ));
        }

        // Traditional Talc sub-allocation path (no pool configured).
        // SAFETY: layout is guaranteed to be valid as it's passed from `MemoryLayout`.
        let alloc_layout = unsafe {
            Layout::from_size_align_unchecked(
                layout.size().get(),
                layout.alignment().get_alignment_value().get(),
            )
        };

        let alloc = {
            let mut lock = zlock!(self.talc);
            // SAFETY: layout is guaranteed to be valid.
            unsafe { lock.malloc(alloc_layout) }
        };

        match alloc {
            Ok(buf) => Ok(self.segment.clone().allocated_chunk(buf, layout)),
            Err(_) => Err(ZAllocError::OutOfMemory),
        }
    }

    fn free(&self, chunk: &ChunkDescriptor) {
        // Pool path: segment return happens at PooledPosixShmSegment::drop — no-op here.
        if self.pool.is_some() {
            return;
        }

        // Traditional Talc path.
        // SAFETY: chunk descriptor is guaranteed to be valid and belong to the segment.
        let alloc_layout = unsafe {
            Layout::from_size_align_unchecked(
                chunk.len.get(),
                self.alignment.get_alignment_value().get(),
            )
        };

        // SAFETY: chunk descriptor is guaranteed to be valid and belong to the segment.
        let ptr = unsafe { self.segment.segment.elem_mut(chunk.chunk) };

        // SAFETY: ptr and layout are guaranteed to be valid as they are passed from `ChunkDescriptor`.
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
