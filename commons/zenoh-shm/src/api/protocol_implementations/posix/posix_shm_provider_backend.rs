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

use std::{
    borrow::Borrow,
    cmp,
    collections::BinaryHeap,
    sync::{
        atomic::{AtomicPtr, AtomicUsize, Ordering},
        Mutex,
    },
};

use zenoh_core::zlock;
use zenoh_result::ZResult;

use super::posix_shm_segment::PosixShmSegment;
use crate::api::{
    common::types::ChunkID,
    provider::{
        chunk::{AllocatedChunk, ChunkDescriptor},
        shm_provider_backend::ShmProviderBackend,
        types::{AllocAlignment, ChunkAllocResult, MemoryLayout, ZAllocError},
    },
};

// TODO: MIN_FREE_CHUNK_SIZE limitation is made to reduce memory fragmentation and lower
// the CPU time needed to defragment() - that's reasonable, and there is additional thing here:
// our SHM\zerocopy functionality outperforms common buffer transmission only starting from 1K
// buffer size. In other words, there should be some minimal size threshold reasonable to use with
// SHM - and it would be good to synchronize this threshold with MIN_FREE_CHUNK_SIZE limitation!
const MIN_FREE_CHUNK_SIZE: usize = 1_024;

#[derive(Eq, Copy, Clone, Debug)]
struct Chunk {
    offset: ChunkID,
    size: usize,
}

impl Ord for Chunk {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.size.cmp(&other.size)
    }
}

impl PartialOrd for Chunk {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size
    }
}

/// Builder to create posix SHM provider
#[zenoh_macros::unstable_doc]
pub struct PosixShmProviderBackendBuilder;

impl PosixShmProviderBackendBuilder {
    /// Use existing layout
    #[zenoh_macros::unstable_doc]
    pub fn with_layout<Layout: Borrow<MemoryLayout>>(
        self,
        layout: Layout,
    ) -> LayoutedPosixShmProviderBackendBuilder<Layout> {
        LayoutedPosixShmProviderBackendBuilder { layout }
    }

    /// Construct layout in-place using arguments
    #[zenoh_macros::unstable_doc]
    pub fn with_layout_args(
        self,
        size: usize,
        alignment: AllocAlignment,
    ) -> ZResult<LayoutedPosixShmProviderBackendBuilder<MemoryLayout>> {
        let layout = MemoryLayout::new(size, alignment)?;
        Ok(LayoutedPosixShmProviderBackendBuilder { layout })
    }

    /// Construct layout in-place from size (default alignment will be used)
    #[zenoh_macros::unstable_doc]
    pub fn with_size(
        self,
        size: usize,
    ) -> ZResult<LayoutedPosixShmProviderBackendBuilder<MemoryLayout>> {
        let layout = MemoryLayout::new(size, AllocAlignment::default())?;
        Ok(LayoutedPosixShmProviderBackendBuilder { layout })
    }
}

#[zenoh_macros::unstable_doc]
pub struct LayoutedPosixShmProviderBackendBuilder<Layout: Borrow<MemoryLayout>> {
    layout: Layout,
}

impl<Layout: Borrow<MemoryLayout>> LayoutedPosixShmProviderBackendBuilder<Layout> {
    /// try to create PosixShmProviderBackend
    #[zenoh_macros::unstable_doc]
    pub fn res(self) -> ZResult<PosixShmProviderBackend> {
        PosixShmProviderBackend::new(self.layout.borrow())
    }
}

/// A backend for ShmProvider based on POSIX shared memory.
/// This is the default general-purpose backed shipped with Zenoh.
#[zenoh_macros::unstable_doc]
pub struct PosixShmProviderBackend {
    available: AtomicUsize,
    segment: PosixShmSegment,
    free_list: Mutex<BinaryHeap<Chunk>>,
    alignment: AllocAlignment,
}

impl PosixShmProviderBackend {
    /// Get the builder to construct a new instance
    #[zenoh_macros::unstable_doc]
    pub fn builder() -> PosixShmProviderBackendBuilder {
        PosixShmProviderBackendBuilder
    }

    fn new(layout: &MemoryLayout) -> ZResult<Self> {
        let segment = PosixShmSegment::create(layout.size())?;

        let mut free_list = BinaryHeap::new();
        let root_chunk = Chunk {
            offset: 0,
            size: layout.size(),
        };
        free_list.push(root_chunk);

        tracing::trace!(
            "Created PosixShmProviderBackend id {}, layout {:?}",
            segment.segment.id(),
            layout
        );

        Ok(Self {
            available: AtomicUsize::new(layout.size()),
            segment,
            free_list: Mutex::new(free_list),
            alignment: layout.alignment(),
        })
    }
}

impl ShmProviderBackend for PosixShmProviderBackend {
    fn alloc(&self, layout: &MemoryLayout) -> ChunkAllocResult {
        tracing::trace!("PosixShmProviderBackend::alloc({:?})", layout);

        let required_len = layout.size();

        if self.available.load(Ordering::Relaxed) < required_len {
            tracing::trace!( "PosixShmProviderBackend does not have sufficient free memory to allocate {:?}, try de-fragmenting!", layout);
            return Err(ZAllocError::OutOfMemory);
        }

        let mut guard = zlock!(self.free_list);
        // The strategy taken is the same for some Unix System V implementations -- as described in the
        // famous Bach's book --  in essence keep an ordered list of free slot and always look for the
        // biggest as that will give the biggest left-over.
        match guard.pop() {
            Some(mut chunk) if chunk.size >= required_len => {
                // NOTE: don't loose any chunks here, as it will lead to memory leak
                tracing::trace!("Allocator selected Chunk ({:?})", &chunk);
                if chunk.size - required_len >= MIN_FREE_CHUNK_SIZE {
                    let free_chunk = Chunk {
                        offset: chunk.offset + required_len as ChunkID,
                        size: chunk.size - required_len,
                    };
                    tracing::trace!("The allocation will leave a Free Chunk: {:?}", &free_chunk);
                    guard.push(free_chunk);
                    chunk.size = required_len;
                }
                self.available.fetch_sub(chunk.size, Ordering::Relaxed);

                let descriptor =
                    ChunkDescriptor::new(self.segment.segment.id(), chunk.offset, chunk.size);

                Ok(AllocatedChunk {
                    descriptor,
                    data: unsafe { AtomicPtr::new(self.segment.segment.elem_mut(chunk.offset)) },
                })
            }
            Some(c) => {
                tracing::trace!("PosixShmProviderBackend::alloc({:?}) cannot find any big enough chunk\nShmManager::free_list = {:?}", layout, self.free_list);
                guard.push(c);
                Err(ZAllocError::NeedDefragment)
            }
            None => {
                // NOTE: that should never happen! If this happens - there is a critical bug somewhere around!
                let err = format!("PosixShmProviderBackend::alloc({:?}) cannot find any available chunk\nShmManager::free_list = {:?}", layout, self.free_list);
                #[cfg(feature = "test")]
                panic!("{err}");
                #[cfg(not(feature = "test"))]
                {
                    tracing::error!("{err}");
                    Err(ZAllocError::OutOfMemory)
                }
            }
        }
    }

    fn free(&self, chunk: &ChunkDescriptor) {
        let free_chunk = Chunk {
            offset: chunk.chunk,
            size: chunk.len,
        };
        self.available.fetch_add(free_chunk.size, Ordering::Relaxed);
        zlock!(self.free_list).push(free_chunk);
    }

    fn defragment(&self) -> usize {
        fn try_merge_adjacent_chunks(a: &Chunk, b: &Chunk) -> Option<Chunk> {
            let end_offset = a.offset as usize + a.size;
            if end_offset == b.offset as usize {
                Some(Chunk {
                    size: a.size + b.size,
                    offset: a.offset,
                })
            } else {
                None
            }
        }

        let mut largest = 0usize;

        // TODO: optimize this!
        // this is an old legacy algo for merging adjacent chunks
        // we extract chunks to separate container, sort them by offset and then check each chunk for
        // adjacence with neighbour. Adjacent chunks are joined and returned back to temporary container.
        // If chunk is not adjacent with it's neighbour, it is placed back to self.free_list
        let mut guard = zlock!(self.free_list);
        if guard.len() > 1 {
            let mut fbs: Vec<Chunk> = guard.drain().collect();
            fbs.sort_by(|x, y| x.offset.cmp(&y.offset));
            let mut current = fbs.remove(0);
            let mut i = 0;
            let n = fbs.len();
            for chunk in fbs.iter() {
                i += 1;
                let next = *chunk;
                match try_merge_adjacent_chunks(&current, &next) {
                    Some(c) => {
                        current = c;
                        largest = largest.max(current.size);
                        if i == n {
                            guard.push(current)
                        }
                    }
                    None => {
                        guard.push(current);
                        if i == n {
                            guard.push(next);
                        } else {
                            current = next;
                        }
                    }
                }
            }
        }
        largest
    }

    fn available(&self) -> usize {
        self.available.load(Ordering::Relaxed)
    }

    fn layout_for(&self, layout: MemoryLayout) -> ZResult<MemoryLayout> {
        layout.extend(self.alignment)
    }
}
