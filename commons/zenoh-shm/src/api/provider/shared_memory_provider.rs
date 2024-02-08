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
    collections::VecDeque,
    marker::PhantomData,
    sync::{atomic::Ordering, Arc},
};

use zenoh_result::ZResult;

use crate::{
    api::common::types::ProtocolID,
    header::{
        allocated_descriptor::AllocatedHeaderDescriptor, descriptor::HeaderDescriptor,
        storage::GLOBAL_HEADER_STORAGE,
    },
    watchdog::{
        allocated_watchdog::AllocatedWatchdog,
        confirmator::{ConfirmedDescriptor, GLOBAL_CONFIRMATOR},
        descriptor::Descriptor,
        storage::GLOBAL_STORAGE,
        validator::GLOBAL_VALIDATOR,
    },
    SharedMemoryBuf, SharedMemoryBufInfo,
};

use super::{
    chunk::{AllocatedChunk, ChunkDescriptor},
    shared_memory_provider_backend::SharedMemoryProviderBackend,
    types::{BufAllocResult, ChunkAllocResult, ZAllocError},
};

#[derive(Debug)]
pub struct BusyChunk {
    descriptor: ChunkDescriptor,
    header: AllocatedHeaderDescriptor,
    _watchdog: AllocatedWatchdog,
}

impl BusyChunk {
    pub fn new(
        descriptor: ChunkDescriptor,
        header: AllocatedHeaderDescriptor,
        watchdog: AllocatedWatchdog,
    ) -> Self {
        Self {
            descriptor,
            header,
            _watchdog: watchdog,
        }
    }
}

pub trait ForceDeallocPolicy {
    fn dealloc(provider: &mut SharedMemoryProvider) -> bool;
}

pub struct DeallocOptimal;
impl ForceDeallocPolicy for DeallocOptimal {
    fn dealloc(provider: &mut SharedMemoryProvider) -> bool {
        let chunk_to_dealloc = match provider.busy_list.remove(1) {
            Some(val) => val,
            None => match provider.busy_list.pop_front() {
                Some(val) => val,
                None => return false,
            },
        };

        provider.backend.free(&chunk_to_dealloc.descriptor);
        true
    }
}

pub struct DeallocYoungest;
impl ForceDeallocPolicy for DeallocYoungest {
    fn dealloc(provider: &mut SharedMemoryProvider) -> bool {
        match provider.busy_list.pop_back() {
            Some(val) => {
                provider.backend.free(&val.descriptor);
                true
            }
            None => false,
        }
    }
}
pub struct DeallocEldest;
impl ForceDeallocPolicy for DeallocEldest {
    fn dealloc(provider: &mut SharedMemoryProvider) -> bool {
        match provider.busy_list.pop_front() {
            Some(val) => {
                provider.backend.free(&val.descriptor);
                true
            }
            None => false,
        }
    }
}

pub trait AllocPolicy {
    fn alloc(provider: &mut SharedMemoryProvider, len: usize) -> ChunkAllocResult;
}

pub struct JustAlloc;
impl AllocPolicy for JustAlloc {
    fn alloc(provider: &mut SharedMemoryProvider, len: usize) -> ChunkAllocResult {
        provider.backend.alloc(len)
    }
}

pub struct GarbageCollect<
    InnerPolicy: AllocPolicy = JustAlloc,
    AltPolicy: AllocPolicy = InnerPolicy,
> {
    _phantom: PhantomData<InnerPolicy>,
    _phantom2: PhantomData<AltPolicy>,
}
impl<InnerPolicy: AllocPolicy, AltPolicy: AllocPolicy> AllocPolicy
    for GarbageCollect<InnerPolicy, AltPolicy>
{
    fn alloc(provider: &mut SharedMemoryProvider, len: usize) -> ChunkAllocResult {
        let result = InnerPolicy::alloc(provider, len);
        if let Err(ZAllocError::OutOfMemory) = result {
            // try to alloc again only if GC managed to reclaim big enough chunk
            if provider.garbage_collect() >= len {
                return AltPolicy::alloc(provider, len);
            }
        }
        result
    }
}

pub struct Defragment<InnerPolicy: AllocPolicy = JustAlloc, AltPolicy: AllocPolicy = InnerPolicy> {
    _phantom: PhantomData<InnerPolicy>,
    _phantom2: PhantomData<AltPolicy>,
}
impl<InnerPolicy: AllocPolicy, AltPolicy: AllocPolicy> AllocPolicy
    for Defragment<InnerPolicy, AltPolicy>
{
    fn alloc(provider: &mut SharedMemoryProvider, len: usize) -> ChunkAllocResult {
        let result = InnerPolicy::alloc(provider, len);
        if let Err(ZAllocError::NeedDefragment) = result {
            // try to alloc again only if big enough chunk was defragmented
            if provider.defragment() >= len {
                return AltPolicy::alloc(provider, len);
            }
        }
        result
    }
}

pub struct Deallocate<
    const N: usize,
    InnerPolicy: AllocPolicy = JustAlloc,
    AltPolicy: AllocPolicy = InnerPolicy,
    DeallocatePolicy: ForceDeallocPolicy = DeallocOptimal,
> {
    _phantom: PhantomData<InnerPolicy>,
    _phantom2: PhantomData<AltPolicy>,
    _phantom3: PhantomData<DeallocatePolicy>,
}
impl<
        const N: usize,
        InnerPolicy: AllocPolicy,
        AltPolicy: AllocPolicy,
        DeallocatePolicy: ForceDeallocPolicy,
    > AllocPolicy for Deallocate<N, InnerPolicy, AltPolicy, DeallocatePolicy>
{
    fn alloc(provider: &mut SharedMemoryProvider, len: usize) -> ChunkAllocResult {
        let mut result = InnerPolicy::alloc(provider, len);
        for _ in 0..N {
            match &result {
                Err(ZAllocError::NeedDefragment) | Err(ZAllocError::OutOfMemory) => {
                    if !DeallocatePolicy::dealloc(provider) {
                        return result;
                    }
                }
                _ => {
                    return result;
                }
            }
            result = AltPolicy::alloc(provider, len);
        }
        result
    }
}

// todo: allocator API
/*
pub struct ShmAllocator<'a, Policy: AllocPolicy> {
    provider: UnsafeCell<&'a mut SharedMemoryProvider>,
    allocations: lockfree::map::Map<std::ptr::NonNull<u8>, SharedMemoryBuf>,
    _phantom: PhantomData<Policy>,
}

unsafe impl<'a, Policy: AllocPolicy> allocator_api2::alloc::Allocator for ShmAllocator<'a, Policy> {
    fn allocate(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, allocator_api2::alloc::AllocError> {
        // todo: support alignment!
        match unsafe { &mut *(self.provider.get()) }.alloc::<Policy>(layout.size()) {
            Ok(val) => {
                let inner = val.buf.load(Ordering::Relaxed);
                let ptr = NonNull::new(inner).ok_or(AllocError)?;
                let sl = unsafe { std::slice::from_raw_parts(inner, 2) };
                let res = NonNull::from(sl);

                self.allocations.insert(ptr, val);
                Ok(res)
            }
            Err(_) => todo!(),
        }
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, _layout: std::alloc::Layout) {
        let _ = self.allocations.remove(&ptr);
    }
}
*/

// SharedMemoryProvider aggregates backend, watchdog and refcount storages and
// provides a generalized interface for shared memory data sources
pub struct SharedMemoryProvider {
    backend: Box<dyn SharedMemoryProviderBackend>,
    busy_list: VecDeque<BusyChunk>,
    id: ProtocolID,
}

impl SharedMemoryProvider {
    // Crete the new SharedMemoryProvider
    // this method is intentionally made private as the SharedMemoryProvider instantiation is up to SharedMemoryFactory
    pub(crate) fn new(backend: Box<dyn SharedMemoryProviderBackend>, id: ProtocolID) -> Self {
        Self {
            backend,
            busy_list: VecDeque::default(),
            id,
        }
    }

    // Allocate buffer of desired size
    pub fn alloc<Policy>(&mut self, len: usize) -> BufAllocResult
    where
        Policy: AllocPolicy,
    {
        // allocate resources for SHM buffer
        let (allocated_header, allocated_watchdog, confirmed_watchdog) = Self::alloc_resources()?;

        // allocate data chunk
        // Perform actions depending on the Policy
        // NOTE: it is necessary to properly map this chunk OR free it if mapping fails!
        // Don't loose this chunk as it leads to memory leak at the backend side!
        // NOTE: self.backend.alloc(len) returns chunk with len >= required len,
        // and it is necessary to handle that properly and pass this len to corresponding free(...)
        let chunk = Policy::alloc(self, len)?;

        // wrap everything to SharedMemoryBuf
        let wrapped = self.wrap(
            chunk,
            len,
            allocated_header,
            allocated_watchdog,
            confirmed_watchdog,
        );
        Ok(wrapped)
    }

    // Defragment the memory
    pub fn defragment(&mut self) -> usize {
        self.backend.defragment()
    }

    // Map externally-allocated chunk into SharedMemoryBuf
    // This method is designed to be used with push data sources
    // Remember that chunk's len may be >= len!
    pub fn map(&mut self, chunk: AllocatedChunk, len: usize) -> ZResult<SharedMemoryBuf> {
        // allocate resources for SHM buffer
        let (allocated_header, allocated_watchdog, confirmed_watchdog) = Self::alloc_resources()?;

        // wrap everything to SharedMemoryBuf
        let wrapped = self.wrap(
            chunk,
            len,
            allocated_header,
            allocated_watchdog,
            confirmed_watchdog,
        );
        Ok(wrapped)
    }

    // Try to collect free chunks
    // Returns the size of largest freed chunk
    pub fn garbage_collect(&mut self) -> usize {
        fn is_free_chunk(chunk: &BusyChunk) -> bool {
            let header = chunk.header.descriptor.header();
            if header.refcount.load(Ordering::SeqCst) != 0 {
                return header.watchdog_invalidated.load(Ordering::SeqCst);
            }
            true
        }

        log::trace!("Running Garbage Collector");

        let mut largest = 0usize;
        self.busy_list.retain(|maybe_free| {
            if is_free_chunk(maybe_free) {
                log::trace!("Garbage Collecting Chunk: {:?}", maybe_free);
                self.backend.free(&maybe_free.descriptor);
                largest = largest.max(maybe_free.descriptor.len);
                return false;
            }
            true
        });
        largest
    }

    // Bytes available for use
    pub fn available(&self) -> usize {
        self.backend.available()
    }

    fn alloc_resources() -> ZResult<(
        AllocatedHeaderDescriptor,
        AllocatedWatchdog,
        ConfirmedDescriptor,
    )> {
        // allocate shared header
        let allocated_header = GLOBAL_HEADER_STORAGE.allocate_header()?;

        // allocate watchdog
        let allocated_watchdog = GLOBAL_STORAGE.allocate_watchdog()?;

        // add watchdog to confirmator
        let confirmed_watchdog = GLOBAL_CONFIRMATOR.add_owned(&allocated_watchdog.descriptor)?;

        Ok((allocated_header, allocated_watchdog, confirmed_watchdog))
    }

    fn wrap(
        &mut self,
        chunk: AllocatedChunk,
        len: usize,
        allocated_header: AllocatedHeaderDescriptor,
        allocated_watchdog: AllocatedWatchdog,
        confirmed_watchdog: ConfirmedDescriptor,
    ) -> SharedMemoryBuf {
        let header = allocated_header.descriptor.clone();
        let descriptor = Descriptor::from(&allocated_watchdog.descriptor);

        // add watchdog to validator
        let c_header = header.clone();
        GLOBAL_VALIDATOR.add(
            allocated_watchdog.descriptor.clone(),
            Box::new(move || {
                c_header
                    .header()
                    .watchdog_invalidated
                    .store(true, Ordering::SeqCst);
            }),
        );

        // Create buffer's info
        let info = SharedMemoryBufInfo::new(
            chunk.descriptor.clone(),
            self.id,
            len,
            descriptor,
            HeaderDescriptor::from(&header),
            header.header().generation.load(Ordering::SeqCst),
        );

        // Create buffer
        let shmb = SharedMemoryBuf {
            header,
            buf: chunk.data,
            info,
            watchdog: Arc::new(confirmed_watchdog),
        };

        // Create and store busy chunk
        self.busy_list.push_back(BusyChunk::new(
            chunk.descriptor,
            allocated_header,
            allocated_watchdog,
        ));

        shmb
    }
}
