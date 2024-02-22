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
    collections::VecDeque,
    marker::PhantomData,
    ptr::NonNull,
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use zenoh_core::zlock;
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
    types::{AllocAlignment, AllocLayout, BufAllocResult, ChunkAllocResult, ZAllocError},
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
    fn dealloc<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
    ) -> bool;
}

pub struct DeallocOptimal;
impl ForceDeallocPolicy for DeallocOptimal {
    fn dealloc<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
    ) -> bool {
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
    fn dealloc<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
    ) -> bool {
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
    fn dealloc<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
    ) -> bool {
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
    fn alloc<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
        layout: &AllocLayout,
    ) -> ChunkAllocResult;
}

#[async_trait]
pub trait AsyncAllocPolicy {
    async fn alloc_async<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
        layout: &AllocLayout,
    ) -> ChunkAllocResult;
}

pub struct JustAlloc;
impl AllocPolicy for JustAlloc {
    fn alloc<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
        layout: &AllocLayout,
    ) -> ChunkAllocResult {
        provider.backend.alloc(layout)
    }
}

pub struct GarbageCollect<InnerPolicy: AllocPolicy = JustAlloc, AltPolicy: AllocPolicy = JustAlloc>
{
    _phantom: PhantomData<InnerPolicy>,
    _phantom2: PhantomData<AltPolicy>,
}
impl<InnerPolicy: AllocPolicy, AltPolicy: AllocPolicy> AllocPolicy
    for GarbageCollect<InnerPolicy, AltPolicy>
{
    fn alloc<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
        layout: &AllocLayout,
    ) -> ChunkAllocResult {
        let result = InnerPolicy::alloc(provider, layout);
        if let Err(ZAllocError::OutOfMemory) = result {
            // try to alloc again only if GC managed to reclaim big enough chunk
            if provider.garbage_collect() >= layout.size() {
                return AltPolicy::alloc(provider, layout);
            }
        }
        result
    }
}

pub struct Defragment<InnerPolicy: AllocPolicy = JustAlloc, AltPolicy: AllocPolicy = JustAlloc> {
    _phantom: PhantomData<InnerPolicy>,
    _phantom2: PhantomData<AltPolicy>,
}
impl<InnerPolicy: AllocPolicy, AltPolicy: AllocPolicy> AllocPolicy
    for Defragment<InnerPolicy, AltPolicy>
{
    fn alloc<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
        layout: &AllocLayout,
    ) -> ChunkAllocResult {
        let result = InnerPolicy::alloc(provider, layout);
        if let Err(ZAllocError::NeedDefragment) = result {
            // try to alloc again only if big enough chunk was defragmented
            if provider.defragment() >= layout.size() {
                return AltPolicy::alloc(provider, layout);
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
    fn alloc<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
        layout: &AllocLayout,
    ) -> ChunkAllocResult {
        let mut result = InnerPolicy::alloc(provider, layout);
        for _ in 0..N {
            match result {
                Err(ZAllocError::NeedDefragment) | Err(ZAllocError::OutOfMemory) => {
                    if !DeallocatePolicy::dealloc(provider) {
                        return result;
                    }
                }
                _ => {
                    return result;
                }
            }
            result = AltPolicy::alloc(provider, layout);
        }
        result
    }
}

pub struct BlockOn<InnerPolicy: AllocPolicy = JustAlloc> {
    _phantom: PhantomData<InnerPolicy>,
}
#[async_trait]
impl<InnerPolicy: AllocPolicy> AsyncAllocPolicy for BlockOn<InnerPolicy> {
    async fn alloc_async<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
        layout: &AllocLayout,
    ) -> ChunkAllocResult {
        loop {
            match InnerPolicy::alloc(provider, layout) {
                Err(ZAllocError::NeedDefragment) | Err(ZAllocError::OutOfMemory) => {
                    // todo: implement provider's async signalling instead of this!
                    async_std::task::sleep(Duration::from_millis(1)).await;
                }
                other_result => {
                    return other_result;
                }
            }
        }
    }
}
impl<InnerPolicy: AllocPolicy> AllocPolicy for BlockOn<InnerPolicy> {
    fn alloc<Backend: SharedMemoryProviderBackend>(
        provider: &mut SharedMemoryProvider<Backend>,
        layout: &AllocLayout,
    ) -> ChunkAllocResult {
        loop {
            match InnerPolicy::alloc(provider, layout) {
                Err(ZAllocError::NeedDefragment) | Err(ZAllocError::OutOfMemory) => {
                    // todo: implement provider's async signalling instead of this!
                    std::thread::sleep(Duration::from_millis(1));
                }
                other_result => {
                    return other_result;
                }
            }
        }
    }
}

// todo: allocator API
pub struct ShmAllocator<'a, Policy: AllocPolicy, Backend: SharedMemoryProviderBackend> {
    provider: Mutex<&'a mut SharedMemoryProvider<Backend>>,
    allocations: lockfree::map::Map<std::ptr::NonNull<u8>, SharedMemoryBuf>,
    _phantom: PhantomData<Policy>,
}

impl<'a, Policy: AllocPolicy, Backend: SharedMemoryProviderBackend>
    ShmAllocator<'a, Policy, Backend>
{
    fn allocate(&self, layout: std::alloc::Layout) -> BufAllocResult {
        let mut guard = zlock!(self.provider);
        guard
            .alloc()
            .with_layout_args(layout.size(), AllocAlignment::new(layout.align() as u32))?
            .res()
    }
}

unsafe impl<'a, Policy: AllocPolicy, Backend: SharedMemoryProviderBackend>
    allocator_api2::alloc::Allocator for ShmAllocator<'a, Policy, Backend>
{
    fn allocate(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, allocator_api2::alloc::AllocError> {
        let allocation = self
            .allocate(layout)
            .map_err(|_| allocator_api2::alloc::AllocError)?;

        let inner = allocation.buf.load(Ordering::Relaxed);
        let ptr = NonNull::new(inner).ok_or(allocator_api2::alloc::AllocError)?;
        let sl = unsafe { std::slice::from_raw_parts(inner, 2) };
        let res = NonNull::from(sl);

        self.allocations.insert(ptr, allocation);
        Ok(res)
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, _layout: std::alloc::Layout) {
        let _ = self.allocations.remove(&ptr);
    }
}

pub trait LimitedAlignment {
    fn max_align(&self) -> AllocAlignment;
}

impl<T: SharedMemoryProviderBackend> LimitedAlignment for T {
    fn max_align(&self) -> AllocAlignment {
        self.max_align()
    }
}

impl<Backend: SharedMemoryProviderBackend> LimitedAlignment for SharedMemoryProvider<Backend> {
    fn max_align(&self) -> AllocAlignment {
        self.backend.max_align()
    }
}

pub struct AllocBuilder<'a, Backend: SharedMemoryProviderBackend, Policy = JustAlloc> {
    provider: &'a mut SharedMemoryProvider<Backend>,
    _phantom: PhantomData<Policy>,
}

impl<'a, Backend: SharedMemoryProviderBackend, T> AllocBuilder<'a, Backend, T> {
    pub fn with_policy<OtherPolicy>(self) -> AllocBuilder<'a, Backend, OtherPolicy> {
        AllocBuilder {
            provider: self.provider,
            _phantom: PhantomData,
        }
    }

    pub fn with_layout<Layout>(self, layout: Layout) -> LayoutedAllocBuilder<'a, T, Layout, Backend>
    where
        Layout: Borrow<AllocLayout>,
    {
        LayoutedAllocBuilder {
            builder: self,
            layout,
        }
    }

    pub fn with_layout_args(
        self,
        size: usize,
        alignment: AllocAlignment,
    ) -> ZResult<LayoutedAllocBuilder<'a, T, AllocLayout, Backend>> {
        let layout = AllocLayout::new(size, alignment, self.provider)?;

        Ok(LayoutedAllocBuilder {
            builder: self,
            layout,
        })
    }

    pub fn with_size(
        self,
        size: usize,
    ) -> ZResult<LayoutedAllocBuilder<'a, T, AllocLayout, Backend>> {
        let layout = AllocLayout::new(size, AllocAlignment::default(), self.provider)?;

        Ok(LayoutedAllocBuilder {
            builder: self,
            layout,
        })
    }
}

pub struct LayoutedAllocBuilder<'a, Policy, Layout, Backend>
where
    Layout: Borrow<AllocLayout>,
    Backend: SharedMemoryProviderBackend,
{
    builder: AllocBuilder<'a, Backend, Policy>,
    layout: Layout,
}

impl<'a, Policy, Layout, Backend> LayoutedAllocBuilder<'a, Policy, Layout, Backend>
where
    Layout: Borrow<AllocLayout>,
    Backend: SharedMemoryProviderBackend,
{
    pub fn res(self) -> BufAllocResult
    where
        Policy: AllocPolicy,
    {
        self.builder
            .provider
            .alloc_inner::<Policy>(self.layout.borrow())
    }

    pub async fn res_async(self) -> BufAllocResult
    where
        Policy: AsyncAllocPolicy,
    {
        self.builder
            .provider
            .alloc_inner_async::<Policy>(self.layout.borrow())
            .await
    }
}

// SharedMemoryProvider aggregates backend, watchdog and refcount storages and
// provides a generalized interface for shared memory data sources
pub struct SharedMemoryProvider<Backend: SharedMemoryProviderBackend> {
    backend: Backend,
    busy_list: VecDeque<BusyChunk>,
    id: ProtocolID,
}

impl<Backend: SharedMemoryProviderBackend> SharedMemoryProvider<Backend> {
    // Crete the new SharedMemoryProvider
    pub fn new(backend: Backend, id: ProtocolID) -> Self {
        Self {
            backend,
            busy_list: VecDeque::default(),
            id,
        }
    }

    // Allocate buffer of desired size
    pub fn alloc<'a, 'b: 'a>(&'b mut self) -> AllocBuilder<'a, Backend> {
        AllocBuilder {
            provider: self,
            _phantom: PhantomData,
        }
    }

    // Defragment memory
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

    fn alloc_inner<Policy>(&mut self, layout: &AllocLayout) -> BufAllocResult
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
        let chunk = Policy::alloc(self, layout)?;

        // wrap everything to SharedMemoryBuf
        let wrapped = self.wrap(
            chunk,
            layout.size(),
            allocated_header,
            allocated_watchdog,
            confirmed_watchdog,
        );
        Ok(wrapped)
    }

    // Allocate buffer of desired size using async policy!
    async fn alloc_inner_async<Policy>(&mut self, layout: &AllocLayout) -> BufAllocResult
    where
        Policy: AsyncAllocPolicy,
    {
        // allocate resources for SHM buffer
        let (allocated_header, allocated_watchdog, confirmed_watchdog) = Self::alloc_resources()?;

        // allocate data chunk
        // Perform actions depending on the Policy
        // NOTE: it is necessary to properly map this chunk OR free it if mapping fails!
        // Don't loose this chunk as it leads to memory leak at the backend side!
        // NOTE: self.backend.alloc(len) returns chunk with len >= required len,
        // and it is necessary to handle that properly and pass this len to corresponding free(...)
        let chunk = Policy::alloc_async(self, layout).await?;

        // wrap everything to SharedMemoryBuf
        let wrapped = self.wrap(
            chunk,
            layout.size(),
            allocated_header,
            allocated_watchdog,
            confirmed_watchdog,
        );
        Ok(wrapped)
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
