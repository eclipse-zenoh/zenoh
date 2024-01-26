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

use std::{cmp, collections::BinaryHeap, mem, sync::atomic::AtomicPtr};

use zenoh_result::ZResult;

use crate::api::{
    common::types::ChunkID,
    provider::{
        chunk::{AllocatedChunk, ChunkDescriptor},
        shared_memory_provider_backend::SharedMemoryProviderBackend,
        types::{AllocError, ChunkAllocResult},
    },
};

use super::posix_shared_memory_segment::PosixSharedMemorySegment;

// todo: MIN_FREE_CHUNK_SIZE limitation is made to reduce memory fragmentation and lower
// the CPU time needed to defragment() - that's reasonable, and there is additional thing here:
// our SHM\zerocopy functionality outperforms common buffer transmission only starting from some
// buffer size (I guess it is 2K on x86_64). In other words, there should be some minimal size
// threshold reasonable to use with SHM - and it would be good to synchronize this threshold with
// MIN_FREE_CHUNK_SIZE limitation!
const MIN_FREE_CHUNK_SIZE: u32 = 1_024;

#[derive(Eq, Copy, Clone, Debug)]
struct Chunk {
    offset: ChunkID,
    size: u32,
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

pub struct PosixSharedMemoryProviderBackend {
    available: u32,
    segment: PosixSharedMemorySegment,
    free_list: BinaryHeap<Chunk>,
    alignment: u32,
}

impl PosixSharedMemoryProviderBackend {
    pub fn new(size: u32) -> ZResult<Self> {
        let segment = PosixSharedMemorySegment::create(size)?;

        let mut free_list = BinaryHeap::new();
        let root_chunk = Chunk { offset: 0, size };
        free_list.push(root_chunk);

        log::trace!(
            "Created PosixSharedMemoryProviderBackend id {}, size {size}",
            segment.segment.id()
        );

        Ok(Self {
            available: size,
            segment,
            free_list,
            alignment: mem::align_of::<u32>() as u32,
        })
    }
}

impl SharedMemoryProviderBackend for PosixSharedMemoryProviderBackend {
    fn alloc(&mut self, len: usize) -> ChunkAllocResult {
        fn align_addr_at(addr: u32, align: u32) -> u32 {
            match addr % align {
                0 => addr,
                r => addr + (align - r),
            }
        }

        log::trace!("PosixSharedMemoryProviderBackend::alloc({len})");
        // Always allocate a size that will keep the proper alignment requirements
        let required_len = align_addr_at(len as u32, self.alignment);

        if self.available >= required_len {
            // The strategy taken is the same for some Unix System V implementations -- as described in the
            // famous Bach's book --  in essence keep an ordered list of free slot and always look for the
            // biggest as that will give the biggest left-over.
            match self.free_list.pop() {
                Some(mut chunk) if chunk.size >= required_len => {
                    // NOTE: don't loose any chunks here, as it will lead to memory leak
                    log::trace!("Allocator selected Chunk ({:?})", &chunk);
                    if chunk.size - required_len >= MIN_FREE_CHUNK_SIZE {
                        let free_chunk = Chunk {
                            offset: chunk.offset + required_len,
                            size: chunk.size - required_len,
                        };
                        log::trace!("The allocation will leave a Free Chunk: {:?}", &free_chunk);
                        self.free_list.push(free_chunk);
                        chunk.size = required_len;
                    }
                    self.available -= chunk.size;

                    let descriptor =
                        ChunkDescriptor::new(self.segment.segment.id(), chunk.offset, chunk.size);

                    Ok(AllocatedChunk {
                        descriptor,
                        data: unsafe {
                            AtomicPtr::new(self.segment.segment.elem_mut(chunk.offset))
                        },
                    })
                }
                Some(c) => {
                    log::trace!("PosixSharedMemoryProviderBackend::alloc({}) cannot find any big enough chunk\nSharedMemoryManager::free_list = {:?}", len, self.free_list);
                    self.free_list.push(c);
                    Err(AllocError::NeedDefragment)
                }
                None => {
                    // NOTE: that should never happen! If this happens - there is a critical bug somewhere around!
                    let err = format!("PosixSharedMemoryProviderBackend::alloc({}) cannot find any available chunk\nSharedMemoryManager::free_list = {:?}", len, self.free_list);
                    #[cfg(feature = "test")]
                    panic!("{err}");
                    #[cfg(not(feature = "test"))]
                    {
                        log::error!("{err}");
                        Err(AllocError::OutOfMemory)
                    }
                }
            }
        } else {
            log::trace!( "PosixSharedMemoryProviderBackend does not have sufficient free memory to allocate {} bytes, try de-fragmenting!", len);
            Err(AllocError::OutOfMemory)
        }
    }

    fn free(&mut self, chunk: &ChunkDescriptor) {
        let free_chunk = Chunk {
            offset: chunk.chunk,
            size: chunk.len,
        };
        self.available += free_chunk.size;
        self.free_list.push(free_chunk);
    }

    fn defragment(&mut self) {
        fn try_merge_adjacent_chunks(a: &Chunk, b: &Chunk) -> Option<Chunk> {
            let end_offset = a.offset + a.size;
            if end_offset == b.offset {
                Some(Chunk {
                    size: a.size + b.size,
                    offset: a.offset,
                })
            } else {
                None
            }
        }

        if self.free_list.len() > 1 {
            let mut fbs: Vec<Chunk> = self.free_list.drain().collect();
            fbs.sort_by(|x, y| x.offset.partial_cmp(&y.offset).unwrap());
            let mut current = fbs.remove(0);
            let mut i = 0;
            let n = fbs.len();
            for chunk in fbs.iter() {
                i += 1;
                let next = *chunk;
                match try_merge_adjacent_chunks(&current, &next) {
                    Some(c) => {
                        current = c;
                        if i == n {
                            self.free_list.push(current)
                        }
                    }
                    None => {
                        self.free_list.push(current);
                        if i == n {
                            self.free_list.push(next);
                        } else {
                            current = next;
                        }
                    }
                }
            }
        }
    }

    fn available(&self) -> usize {
        self.available as usize
    }
}
