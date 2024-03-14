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
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
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
    types::{AllocAlignment, BufAllocResult, ChunkAllocResult, MemoryLayout, ZAllocError},
    zsliceshm::ZSliceShm,
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

pub struct AllocLayoutBuilder<'a, const ID: ProtocolID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    provider: &'a SharedMemoryProvider<ID, Backend>,
}
impl<'a, const ID: ProtocolID, Backend> AllocLayoutBuilder<'a, ID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    /// Set size for layout
    pub fn size(self, size: usize) -> AllocLayoutSizedBuilder<'a, ID, Backend> {
        AllocLayoutSizedBuilder {
            provider: self.provider,
            size,
        }
    }

    /*
    pub fn for_type<T: IStable<ContainsIndirections = stabby::abi::B0>>(
        self,
    ) -> AllocLayout<'a, Backend> {
        todo: return AllocLayout for type
    }
    */
}

pub struct AllocLayoutSizedBuilder<'a, const ID: ProtocolID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    provider: &'a SharedMemoryProvider<ID, Backend>,
    size: usize,
}
impl<'a, const ID: ProtocolID, Backend> AllocLayoutSizedBuilder<'a, ID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    /// Set alignment for layout
    pub fn alignment(
        self,
        alignment: AllocAlignment,
    ) -> AllocLayoutAlignedBuilder<'a, ID, Backend> {
        AllocLayoutAlignedBuilder {
            provider: self.provider,
            size: self.size,
            alignment,
        }
    }

    pub fn res(self) -> ZResult<AllocLayout<'a, ID, Backend>> {
        AllocLayout::new(self.size, AllocAlignment::default(), self.provider)
    }
}

pub struct AllocLayoutAlignedBuilder<'a, const ID: ProtocolID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    provider: &'a SharedMemoryProvider<ID, Backend>,
    size: usize,
    alignment: AllocAlignment,
}
impl<'a, const ID: ProtocolID, Backend> AllocLayoutAlignedBuilder<'a, ID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    /// Try to build layout with specified args for parent SharedMemoryProvider
    pub fn res(self) -> ZResult<AllocLayout<'a, ID, Backend>> {
        AllocLayout::new(self.size, self.alignment, self.provider)
    }
}

/// A layout for allocations.
/// This is a pre-calculated layout suitable for making series of similar allocations
#[derive(Debug)]
pub struct AllocLayout<'a, const ID: ProtocolID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    size: usize,
    provider_layout: MemoryLayout,
    provider: &'a SharedMemoryProvider<ID, Backend>,
}

impl<'a, const ID: ProtocolID, Backend> AllocLayout<'a, ID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    /// Allocate the new buffer with this layout
    pub fn alloc(&'a self) -> AllocBuilder<'a, ID, Backend> {
        AllocBuilder {
            layout: self,
            _phantom: PhantomData,
        }
    }

    fn new(
        size: usize,
        alignment: AllocAlignment,
        provider: &'a SharedMemoryProvider<ID, Backend>,
    ) -> ZResult<Self> {
        // NOTE: Depending on internal implementation, provider's backend might relayout
        // the allocations for bigger alignment (ex. 4-byte aligned allocation to 8-bytes aligned)

        // Create layout for specified arguments
        let layout = MemoryLayout::new(size, alignment)?;

        // Obtain provider's layout for our layout
        let provider_layout = provider.backend.layout_for(layout)?;

        Ok(Self {
            size,
            provider_layout,
            provider,
        })
    }
}

pub trait ForceDeallocPolicy {
    fn dealloc<const ID: ProtocolID, Backend: SharedMemoryProviderBackend>(
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> bool;
}

/// Try to dealloc optimal (currently eldest+1) chunk
pub struct DeallocOptimal;
impl ForceDeallocPolicy for DeallocOptimal {
    fn dealloc<const ID: ProtocolID, Backend: SharedMemoryProviderBackend>(
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> bool {
        let mut guard = provider.busy_list.lock().unwrap();
        let chunk_to_dealloc = match guard.remove(1) {
            Some(val) => val,
            None => match guard.pop_front() {
                Some(val) => val,
                None => return false,
            },
        };
        drop(guard);

        provider.backend.free(&chunk_to_dealloc.descriptor);
        true
    }
}

/// Try to dealloc youngest chunk
pub struct DeallocYoungest;
impl ForceDeallocPolicy for DeallocYoungest {
    fn dealloc<const ID: ProtocolID, Backend: SharedMemoryProviderBackend>(
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> bool {
        match provider.busy_list.lock().unwrap().pop_back() {
            Some(val) => {
                provider.backend.free(&val.descriptor);
                true
            }
            None => false,
        }
    }
}

/// Try to dealloc eldest chunk
pub struct DeallocEldest;
impl ForceDeallocPolicy for DeallocEldest {
    fn dealloc<const ID: ProtocolID, Backend: SharedMemoryProviderBackend>(
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> bool {
        match provider.busy_list.lock().unwrap().pop_front() {
            Some(val) => {
                provider.backend.free(&val.descriptor);
                true
            }
            None => false,
        }
    }
}

pub trait AllocPolicy {
    fn alloc<const ID: ProtocolID, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> ChunkAllocResult;
}

#[async_trait]
pub trait AsyncAllocPolicy {
    async fn alloc_async<const ID: ProtocolID, Backend: SharedMemoryProviderBackend + Sync>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> ChunkAllocResult;
}

/// Just try to allocate
pub struct JustAlloc;
impl AllocPolicy for JustAlloc {
    fn alloc<const ID: ProtocolID, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> ChunkAllocResult {
        provider.backend.alloc(layout)
    }
}

/// Garbage collection policy.
/// Try to reclaim old buffers if allocation failed and allocate again
/// if the largest reclaimed chuk is not smaller than the one required
pub struct GarbageCollect<InnerPolicy = JustAlloc, AltPolicy = JustAlloc>
where
    InnerPolicy: AllocPolicy,
    AltPolicy: AllocPolicy,
{
    _phantom: PhantomData<InnerPolicy>,
    _phantom2: PhantomData<AltPolicy>,
}
impl<InnerPolicy, AltPolicy> AllocPolicy for GarbageCollect<InnerPolicy, AltPolicy>
where
    InnerPolicy: AllocPolicy,
    AltPolicy: AllocPolicy,
{
    fn alloc<const ID: ProtocolID, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> ChunkAllocResult {
        let result = InnerPolicy::alloc(layout, provider);
        if let Err(ZAllocError::OutOfMemory) = result {
            // try to alloc again only if GC managed to reclaim big enough chunk
            if provider.garbage_collect() >= layout.size() {
                return AltPolicy::alloc(layout, provider);
            }
        }
        result
    }
}

/// Defragmenting policy.
/// Try to defragment if allocation failed and allocate again
/// if the largest defragmented chuk is not smaller than the one required
pub struct Defragment<InnerPolicy = JustAlloc, AltPolicy = JustAlloc>
where
    InnerPolicy: AllocPolicy,
    AltPolicy: AllocPolicy,
{
    _phantom: PhantomData<InnerPolicy>,
    _phantom2: PhantomData<AltPolicy>,
}
impl<InnerPolicy, AltPolicy> AllocPolicy for Defragment<InnerPolicy, AltPolicy>
where
    InnerPolicy: AllocPolicy,
    AltPolicy: AllocPolicy,
{
    fn alloc<const ID: ProtocolID, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> ChunkAllocResult {
        let result = InnerPolicy::alloc(layout, provider);
        if let Err(ZAllocError::NeedDefragment) = result {
            // try to alloc again only if big enough chunk was defragmented
            if provider.defragment() >= layout.size() {
                return AltPolicy::alloc(layout, provider);
            }
        }
        result
    }
}

/// Deallocating policy.
/// Forcely deallocate up to N buffers until allocation succeeds.
pub struct Deallocate<
    const N: usize,
    InnerPolicy = JustAlloc,
    AltPolicy = InnerPolicy,
    DeallocatePolicy = DeallocOptimal,
> where
    InnerPolicy: AllocPolicy,
    AltPolicy: AllocPolicy,
    DeallocatePolicy: ForceDeallocPolicy,
{
    _phantom: PhantomData<InnerPolicy>,
    _phantom2: PhantomData<AltPolicy>,
    _phantom3: PhantomData<DeallocatePolicy>,
}
impl<const N: usize, InnerPolicy, AltPolicy, DeallocatePolicy> AllocPolicy
    for Deallocate<N, InnerPolicy, AltPolicy, DeallocatePolicy>
where
    InnerPolicy: AllocPolicy,
    AltPolicy: AllocPolicy,
    DeallocatePolicy: ForceDeallocPolicy,
{
    fn alloc<const ID: ProtocolID, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> ChunkAllocResult {
        let mut result = InnerPolicy::alloc(layout, provider);
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
            result = AltPolicy::alloc(layout, provider);
        }
        result
    }
}

/// Blocking allocation policy.
/// This policy will block until the allocation succeeds.
/// Both sync and async modes available.
pub struct BlockOn<InnerPolicy = JustAlloc>
where
    InnerPolicy: AllocPolicy,
{
    _phantom: PhantomData<InnerPolicy>,
}
#[async_trait]
impl<InnerPolicy> AsyncAllocPolicy for BlockOn<InnerPolicy>
where
    InnerPolicy: AllocPolicy,
{
    async fn alloc_async<const ID: ProtocolID, Backend: SharedMemoryProviderBackend + Sync>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> ChunkAllocResult {
        loop {
            match InnerPolicy::alloc(layout, provider) {
                Err(ZAllocError::NeedDefragment) | Err(ZAllocError::OutOfMemory) => {
                    // todo: implement provider's async signalling instead of this!
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                other_result => {
                    return other_result;
                }
            }
        }
    }
}
impl<InnerPolicy> AllocPolicy for BlockOn<InnerPolicy>
where
    InnerPolicy: AllocPolicy,
{
    fn alloc<const ID: ProtocolID, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<ID, Backend>,
    ) -> ChunkAllocResult {
        loop {
            match InnerPolicy::alloc(layout, provider) {
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
/*pub struct ShmAllocator<
    'a,
    Policy: AllocPolicy,
    const ID: ProtocolID,
    Backend: SharedMemoryProviderBackend,
> {
    provider: &'a SharedMemoryProvider<ID, Backend>,
    allocations: lockfree::map::Map<std::ptr::NonNull<u8>, SharedMemoryBuf>,
    _phantom: PhantomData<Policy>,
}

impl<'a, Policy: AllocPolicy, const ID: ProtocolID, Backend: SharedMemoryProviderBackend>
    ShmAllocator<'a, Policy, ID, Backend>
{
    fn allocate(&self, layout: std::alloc::Layout) -> BufAllocResult {
        self.provider
            .alloc_layout()
            .size(layout.size())
            .alignment(AllocAlignment::new(layout.align() as u32))
            .res()?
            .alloc()
            .res()
    }
}

unsafe impl<'a, Policy: AllocPolicy, const ID: ProtocolID, Backend: SharedMemoryProviderBackend>
    allocator_api2::alloc::Allocator for ShmAllocator<'a, Policy, ID, Backend>
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
}*/

/// Builder for allocations
pub struct AllocBuilder<
    'a,
    const ID: ProtocolID,
    Backend: SharedMemoryProviderBackend,
    Policy = JustAlloc,
> {
    layout: &'a AllocLayout<'a, ID, Backend>,
    _phantom: PhantomData<Policy>,
}

// Generic impl
impl<'a, const ID: ProtocolID, Backend, Policy> AllocBuilder<'a, ID, Backend, Policy>
where
    Backend: SharedMemoryProviderBackend,
{
    /// Set the allocation policy
    pub fn with_policy<OtherPolicy>(self) -> AllocBuilder<'a, ID, Backend, OtherPolicy> {
        AllocBuilder {
            layout: self.layout,
            _phantom: PhantomData,
        }
    }
}

// Alloc policy
impl<'a, const ID: ProtocolID, Backend, Policy> AllocBuilder<'a, ID, Backend, Policy>
where
    Backend: SharedMemoryProviderBackend,
    Policy: AllocPolicy,
{
    /// Get the result
    pub fn res(self) -> BufAllocResult {
        self.layout
            .provider
            .alloc_inner::<Policy>(self.layout.size, &self.layout.provider_layout)
    }
}

// Async Alloc policy
impl<'a, const ID: ProtocolID, Backend, Policy> AllocBuilder<'a, ID, Backend, Policy>
where
    Backend: SharedMemoryProviderBackend + Sync,
    Policy: AsyncAllocPolicy,
{
    /// Get the async result
    pub async fn res_async(self) -> BufAllocResult {
        self.layout
            .provider
            .alloc_inner_async::<Policy>(self.layout.size, &self.layout.provider_layout)
            .await
    }
}

pub struct SharedMemoryProviderBuilder;
impl SharedMemoryProviderBuilder {
    /// Builder
    pub fn builder() -> Self {
        Self
    }

    /// Set the protocol ID
    pub fn protocol_id<const ID: ProtocolID>(self) -> SharedMemoryProviderBuilderID<ID> {
        SharedMemoryProviderBuilderID::<ID> {}
    }
}

pub struct SharedMemoryProviderBuilderID<const ID: ProtocolID>;
impl<const ID: ProtocolID> SharedMemoryProviderBuilderID<ID> {
    /// Set the backend
    pub fn backend<Backend: SharedMemoryProviderBackend>(
        self,
        backend: Backend,
    ) -> SharedMemoryProviderBuilderBackendID<ID, Backend> {
        SharedMemoryProviderBuilderBackendID { backend }
    }
}

pub struct SharedMemoryProviderBuilderBackendID<const ID: ProtocolID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    backend: Backend,
}
impl<const ID: ProtocolID, Backend> SharedMemoryProviderBuilderBackendID<ID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    pub fn res(self) -> SharedMemoryProvider<ID, Backend> {
        SharedMemoryProvider::<ID, Backend>::new(self.backend)
    }
}

/// A generalized interface for shared memory data sources
#[derive(Debug)]
pub struct SharedMemoryProvider<const ID: ProtocolID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    backend: Backend,
    busy_list: Mutex<VecDeque<BusyChunk>>,
}

impl<const ID: ProtocolID, Backend> SharedMemoryProvider<ID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    /// Create layout builder associated with particular SharedMemoryProvider.
    /// Layout is a rich interface to make allocations
    pub fn alloc_layout(&self) -> AllocLayoutBuilder<ID, Backend> {
        AllocLayoutBuilder { provider: self }
    }

    /// Defragment memory
    pub fn defragment(&self) -> usize {
        self.backend.defragment()
    }

    /// Map externally-allocated chunk into SharedMemoryBuf.
    /// This method is designed to be used with push data sources.
    /// Remember that chunk's len may be >= len!
    pub fn map(&self, chunk: AllocatedChunk, len: usize) -> ZResult<SharedMemoryBuf> {
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

    /// Try to collect free chunks.
    /// Returns the size of largest collected chunk
    pub fn garbage_collect(&self) -> usize {
        fn is_free_chunk(chunk: &BusyChunk) -> bool {
            let header = chunk.header.descriptor.header();
            if header.refcount.load(Ordering::SeqCst) != 0 {
                return header.watchdog_invalidated.load(Ordering::SeqCst);
            }
            true
        }

        log::trace!("Running Garbage Collector");

        let mut largest = 0usize;
        let mut guard = self.busy_list.lock().unwrap();
        guard.retain(|maybe_free| {
            if is_free_chunk(maybe_free) {
                log::trace!("Garbage Collecting Chunk: {:?}", maybe_free);
                self.backend.free(&maybe_free.descriptor);
                largest = largest.max(maybe_free.descriptor.len);
                return false;
            }
            true
        });
        drop(guard);

        largest
    }

    /// Bytes available for use
    pub fn available(&self) -> usize {
        self.backend.available()
    }
}

// PRIVATE impls
impl<const ID: ProtocolID, Backend> SharedMemoryProvider<ID, Backend>
where
    Backend: SharedMemoryProviderBackend,
{
    fn new(backend: Backend) -> Self {
        Self {
            backend,
            busy_list: Mutex::new(VecDeque::default()),
        }
    }

    fn alloc_inner<Policy>(&self, size: usize, layout: &MemoryLayout) -> BufAllocResult
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
        let chunk = Policy::alloc(layout, self)?;

        // wrap allocated chunk to SharedMemoryBuf
        let wrapped = self.wrap(
            chunk,
            size,
            allocated_header,
            allocated_watchdog,
            confirmed_watchdog,
        );
        Ok(ZSliceShm::new(wrapped))
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
        &self,
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
            ID,
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
        self.busy_list.lock().unwrap().push_back(BusyChunk::new(
            chunk.descriptor,
            allocated_header,
            allocated_watchdog,
        ));

        shmb
    }
}

// PRIVATE impls for Sync backend
impl<const ID: ProtocolID, Backend> SharedMemoryProvider<ID, Backend>
where
    Backend: SharedMemoryProviderBackend + Sync,
{
    async fn alloc_inner_async<Policy>(
        &self,
        size: usize,
        backend_layout: &MemoryLayout,
    ) -> BufAllocResult
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
        let chunk = Policy::alloc_async(backend_layout, self).await?;

        // wrap allocated chunk to SharedMemoryBuf
        let wrapped = self.wrap(
            chunk,
            size,
            allocated_header,
            allocated_watchdog,
            confirmed_watchdog,
        );
        Ok(ZSliceShm::new(wrapped))
    }
}
