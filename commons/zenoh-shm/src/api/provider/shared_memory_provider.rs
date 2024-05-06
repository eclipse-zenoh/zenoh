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

use super::{
    chunk::{AllocatedChunk, ChunkDescriptor},
    shared_memory_provider_backend::SharedMemoryProviderBackend,
    types::{
        AllocAlignment, BufAllocResult, BufLayoutAllocResult, ChunkAllocResult, MemoryLayout,
        ZAllocError, ZLayoutAllocError, ZLayoutError,
    },
};
use crate::{
    api::{buffer::zshmmut::ZShmMut, common::types::ProtocolID},
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

#[derive(Debug)]
struct BusyChunk {
    descriptor: ChunkDescriptor,
    header: AllocatedHeaderDescriptor,
    _watchdog: AllocatedWatchdog,
}

impl BusyChunk {
    fn new(
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

struct AllocData<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    size: usize,
    alignment: AllocAlignment,
    provider: &'a SharedMemoryProvider<IDSource, Backend>,
}

#[zenoh_macros::unstable_doc]
pub struct AllocLayoutSizedBuilder<'a, IDSource, Backend>(AllocData<'a, IDSource, Backend>)
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend;

impl<'a, IDSource, Backend> AllocLayoutSizedBuilder<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    fn new(provider: &'a SharedMemoryProvider<IDSource, Backend>, size: usize) -> Self {
        Self(AllocData {
            provider,
            size,
            alignment: AllocAlignment::default(),
        })
    }

    /// Set alignment
    #[zenoh_macros::unstable_doc]
    pub fn with_alignment(self, alignment: AllocAlignment) -> Self {
        Self(AllocData {
            provider: self.0.provider,
            size: self.0.size,
            alignment,
        })
    }

    /// Try to build an allocation layout
    #[zenoh_macros::unstable_doc]
    pub fn make_layout(self) -> Result<AllocLayout<'a, IDSource, Backend>, ZLayoutError> {
        AllocLayout::new(self.0)
    }

    /// Set the allocation policy
    #[zenoh_macros::unstable_doc]
    pub fn with_policy<Policy>(self) -> AllocBuilder2<'a, IDSource, Backend, Policy> {
        AllocBuilder2 {
            data: self.0,
            _phantom: PhantomData,
        }
    }

    /// Get the result
    #[zenoh_macros::unstable_doc]
    pub fn res(self) -> BufLayoutAllocResult {
        let builder = AllocBuilder2::<'a, IDSource, Backend, JustAlloc> {
            data: self.0,
            _phantom: PhantomData,
        };

        builder.res()
    }
}

/// A layout for allocations.
/// This is a pre-calculated layout suitable for making series of similar allocations
/// adopted for particular SharedMemoryProvider
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub struct AllocLayout<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    size: usize,
    provider_layout: MemoryLayout,
    provider: &'a SharedMemoryProvider<IDSource, Backend>,
}

impl<'a, IDSource, Backend> AllocLayout<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    /// Allocate the new buffer with this layout
    #[zenoh_macros::unstable_doc]
    pub fn alloc(&'a self) -> AllocBuilder<'a, IDSource, Backend> {
        AllocBuilder {
            layout: self,
            _phantom: PhantomData,
        }
    }

    fn new(data: AllocData<'a, IDSource, Backend>) -> Result<Self, ZLayoutError> {
        // NOTE: Depending on internal implementation, provider's backend might relayout
        // the allocations for bigger alignment (ex. 4-byte aligned allocation to 8-bytes aligned)

        // Create layout for specified arguments
        let layout = MemoryLayout::new(data.size, data.alignment)
            .map_err(|_| ZLayoutError::IncorrectLayoutArgs)?;

        // Obtain provider's layout for our layout
        let provider_layout = data
            .provider
            .backend
            .layout_for(layout)
            .map_err(|_| ZLayoutError::ProviderIncompatibleLayout)?;

        Ok(Self {
            size: data.size,
            provider_layout,
            provider: data.provider,
        })
    }
}

/// Trait for deallocation policies.
#[zenoh_macros::unstable_doc]
pub trait ForceDeallocPolicy {
    fn dealloc<IDSource: ProtocolIDSource, Backend: SharedMemoryProviderBackend>(
        provider: &SharedMemoryProvider<IDSource, Backend>,
    ) -> bool;
}

/// Try to dealloc optimal (currently eldest+1) chunk
#[zenoh_macros::unstable_doc]
pub struct DeallocOptimal;
impl ForceDeallocPolicy for DeallocOptimal {
    fn dealloc<IDSource: ProtocolIDSource, Backend: SharedMemoryProviderBackend>(
        provider: &SharedMemoryProvider<IDSource, Backend>,
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
#[zenoh_macros::unstable_doc]
pub struct DeallocYoungest;
impl ForceDeallocPolicy for DeallocYoungest {
    fn dealloc<IDSource: ProtocolIDSource, Backend: SharedMemoryProviderBackend>(
        provider: &SharedMemoryProvider<IDSource, Backend>,
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
#[zenoh_macros::unstable_doc]
pub struct DeallocEldest;
impl ForceDeallocPolicy for DeallocEldest {
    fn dealloc<IDSource: ProtocolIDSource, Backend: SharedMemoryProviderBackend>(
        provider: &SharedMemoryProvider<IDSource, Backend>,
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

/// Trait for allocation policies
#[zenoh_macros::unstable_doc]
pub trait AllocPolicy {
    fn alloc<IDSource: ProtocolIDSource, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<IDSource, Backend>,
    ) -> ChunkAllocResult;
}

/// Trait for async allocation policies
#[zenoh_macros::unstable_doc]
#[async_trait]
pub trait AsyncAllocPolicy {
    async fn alloc_async<
        IDSource: ProtocolIDSource + Send + Sync,
        Backend: SharedMemoryProviderBackend + Sync,
    >(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<IDSource, Backend>,
    ) -> ChunkAllocResult;
}

/// Just try to allocate
#[zenoh_macros::unstable_doc]
pub struct JustAlloc;
impl AllocPolicy for JustAlloc {
    fn alloc<IDSource: ProtocolIDSource, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<IDSource, Backend>,
    ) -> ChunkAllocResult {
        provider.backend.alloc(layout)
    }
}

/// Garbage collection policy.
/// Try to reclaim old buffers if allocation failed and allocate again
/// if the largest reclaimed chuk is not smaller than the one required
#[zenoh_macros::unstable_doc]
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
    fn alloc<IDSource: ProtocolIDSource, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<IDSource, Backend>,
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
#[zenoh_macros::unstable_doc]
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
    fn alloc<IDSource: ProtocolIDSource, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<IDSource, Backend>,
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
#[zenoh_macros::unstable_doc]
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
    fn alloc<IDSource: ProtocolIDSource, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<IDSource, Backend>,
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
#[zenoh_macros::unstable_doc]
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
    async fn alloc_async<
        IDSource: ProtocolIDSource + Send + Sync,
        Backend: SharedMemoryProviderBackend + Sync,
    >(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<IDSource, Backend>,
    ) -> ChunkAllocResult {
        loop {
            match InnerPolicy::alloc(layout, provider) {
                Err(ZAllocError::NeedDefragment) | Err(ZAllocError::OutOfMemory) => {
                    // TODO: implement provider's async signalling instead of this!
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
    fn alloc<IDSource: ProtocolIDSource, Backend: SharedMemoryProviderBackend>(
        layout: &MemoryLayout,
        provider: &SharedMemoryProvider<IDSource, Backend>,
    ) -> ChunkAllocResult {
        loop {
            match InnerPolicy::alloc(layout, provider) {
                Err(ZAllocError::NeedDefragment) | Err(ZAllocError::OutOfMemory) => {
                    // TODO: implement provider's async signalling instead of this!
                    std::thread::sleep(Duration::from_millis(1));
                }
                other_result => {
                    return other_result;
                }
            }
        }
    }
}

// TODO: allocator API
/*pub struct ShmAllocator<
    'a,
    Policy: AllocPolicy,
    IDSource,
    Backend: SharedMemoryProviderBackend,
> {
    provider: &'a SharedMemoryProvider<IDSource, Backend>,
    allocations: lockfree::map::Map<std::ptr::NonNull<u8>, SharedMemoryBuf>,
    _phantom: PhantomData<Policy>,
}

impl<'a, Policy: AllocPolicy, IDSource, Backend: SharedMemoryProviderBackend>
    ShmAllocator<'a, Policy, IDSource, Backend>
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

unsafe impl<'a, Policy: AllocPolicy, IDSource, Backend: SharedMemoryProviderBackend>
    allocator_api2::alloc::Allocator for ShmAllocator<'a, Policy, IDSource, Backend>
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
#[zenoh_macros::unstable_doc]
pub struct AllocBuilder2<
    'a,
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
    Policy = JustAlloc,
> {
    data: AllocData<'a, IDSource, Backend>,
    _phantom: PhantomData<Policy>,
}

// Generic impl
impl<'a, IDSource, Backend, Policy> AllocBuilder2<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    /// Set the allocation policy
    #[zenoh_macros::unstable_doc]
    pub fn with_policy<OtherPolicy>(self) -> AllocBuilder2<'a, IDSource, Backend, OtherPolicy> {
        AllocBuilder2 {
            data: self.data,
            _phantom: PhantomData,
        }
    }
}

// Alloc policy
impl<'a, IDSource, Backend, Policy> AllocBuilder2<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
    Policy: AllocPolicy,
{
    /// Get the result
    #[zenoh_macros::unstable_doc]
    pub fn res(self) -> BufLayoutAllocResult {
        let layout = AllocLayout::new(self.data).map_err(ZLayoutAllocError::Layout)?;

        layout
            .alloc()
            .with_policy::<Policy>()
            .res()
            .map_err(ZLayoutAllocError::Alloc)
    }
}

// Async Alloc policy
impl<'a, IDSource, Backend, Policy> AllocBuilder2<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource + Send + Sync,
    Backend: SharedMemoryProviderBackend + Sync,
    Policy: AsyncAllocPolicy,
{
    /// Get the async result
    #[zenoh_macros::unstable_doc]
    pub async fn res_async(self) -> BufLayoutAllocResult {
        let layout = AllocLayout::new(self.data).map_err(ZLayoutAllocError::Layout)?;

        layout
            .alloc()
            .with_policy::<Policy>()
            .res_async()
            .await
            .map_err(ZLayoutAllocError::Alloc)
    }
}

/// Builder for allocations
#[zenoh_macros::unstable_doc]
pub struct AllocBuilder<
    'a,
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
    Policy = JustAlloc,
> {
    layout: &'a AllocLayout<'a, IDSource, Backend>,
    _phantom: PhantomData<Policy>,
}

// Generic impl
impl<'a, IDSource, Backend, Policy> AllocBuilder<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    /// Set the allocation policy
    #[zenoh_macros::unstable_doc]
    pub fn with_policy<OtherPolicy>(self) -> AllocBuilder<'a, IDSource, Backend, OtherPolicy> {
        AllocBuilder {
            layout: self.layout,
            _phantom: PhantomData,
        }
    }
}

// Alloc policy
impl<'a, IDSource, Backend, Policy> AllocBuilder<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
    Policy: AllocPolicy,
{
    /// Get the result
    #[zenoh_macros::unstable_doc]
    pub fn res(self) -> BufAllocResult {
        self.layout
            .provider
            .alloc_inner::<Policy>(self.layout.size, &self.layout.provider_layout)
    }
}

// Async Alloc policy
impl<'a, IDSource, Backend, Policy> AllocBuilder<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource + Send + Sync,
    Backend: SharedMemoryProviderBackend + Sync,
    Policy: AsyncAllocPolicy,
{
    /// Get the async result
    #[zenoh_macros::unstable_doc]
    pub async fn res_async(self) -> BufAllocResult {
        self.layout
            .provider
            .alloc_inner_async::<Policy>(self.layout.size, &self.layout.provider_layout)
            .await
    }
}

#[zenoh_macros::unstable_doc]
pub struct SharedMemoryProviderBuilder;
impl SharedMemoryProviderBuilder {
    /// Get the builder to construct SharedMemoryProvider
    #[zenoh_macros::unstable_doc]
    pub fn builder() -> Self {
        Self
    }

    /// Set compile-time-evaluated protocol ID (preferred)
    #[zenoh_macros::unstable_doc]
    pub fn protocol_id<const ID: ProtocolID>(
        self,
    ) -> SharedMemoryProviderBuilderID<StaticProtocolID<ID>> {
        SharedMemoryProviderBuilderID::<StaticProtocolID<ID>> {
            id: StaticProtocolID,
        }
    }

    /// Set runtime-evaluated protocol ID
    #[zenoh_macros::unstable_doc]
    pub fn dynamic_protocol_id(
        self,
        id: ProtocolID,
    ) -> SharedMemoryProviderBuilderID<DynamicProtocolID> {
        SharedMemoryProviderBuilderID::<DynamicProtocolID> {
            id: DynamicProtocolID::new(id),
        }
    }
}

#[zenoh_macros::unstable_doc]
pub struct SharedMemoryProviderBuilderID<IDSource: ProtocolIDSource> {
    id: IDSource,
}
impl<IDSource: ProtocolIDSource> SharedMemoryProviderBuilderID<IDSource> {
    /// Set the backend
    #[zenoh_macros::unstable_doc]
    pub fn backend<Backend: SharedMemoryProviderBackend>(
        self,
        backend: Backend,
    ) -> SharedMemoryProviderBuilderBackendID<IDSource, Backend> {
        SharedMemoryProviderBuilderBackendID {
            backend,
            id: self.id,
        }
    }
}

#[zenoh_macros::unstable_doc]
pub struct SharedMemoryProviderBuilderBackendID<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    backend: Backend,
    id: IDSource,
}
impl<IDSource, Backend> SharedMemoryProviderBuilderBackendID<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    /// build SharedMemoryProvider
    #[zenoh_macros::unstable_doc]
    pub fn res(self) -> SharedMemoryProvider<IDSource, Backend> {
        SharedMemoryProvider::new(self.backend, self.id)
    }
}

/// Trait to create ProtocolID sources for SharedMemoryProvider
#[zenoh_macros::unstable_doc]
pub trait ProtocolIDSource {
    fn id(&self) -> ProtocolID;
}

/// Static ProtocolID source. This is a recommended API to set ProtocolID
/// when creating SharedMemoryProvider as the ID value is statically evaluated
/// at compile-time and can be optimized.
#[zenoh_macros::unstable_doc]
#[derive(Default)]
pub struct StaticProtocolID<const ID: ProtocolID>;
impl<const ID: ProtocolID> ProtocolIDSource for StaticProtocolID<ID> {
    fn id(&self) -> ProtocolID {
        ID
    }
}

/// Dynamic ProtocolID source. This is an alternative API to set ProtocolID
/// when creating SharedMemoryProvider for cases where ProtocolID is unknown
/// at compile-time.
#[zenoh_macros::unstable_doc]
pub struct DynamicProtocolID {
    id: ProtocolID,
}
impl DynamicProtocolID {
    #[zenoh_macros::unstable_doc]
    pub fn new(id: ProtocolID) -> Self {
        Self { id }
    }
}
impl ProtocolIDSource for DynamicProtocolID {
    fn id(&self) -> ProtocolID {
        self.id
    }
}
unsafe impl Send for DynamicProtocolID {}
unsafe impl Sync for DynamicProtocolID {}

/// A generalized interface for shared memory data sources
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub struct SharedMemoryProvider<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    backend: Backend,
    busy_list: Mutex<VecDeque<BusyChunk>>,
    id: IDSource,
}

impl<IDSource, Backend> SharedMemoryProvider<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    /// Rich interface for making allocations
    #[zenoh_macros::unstable_doc]
    pub fn alloc(&self, size: usize) -> AllocLayoutSizedBuilder<IDSource, Backend> {
        AllocLayoutSizedBuilder::new(self, size)
    }

    /// Defragment memory
    #[zenoh_macros::unstable_doc]
    pub fn defragment(&self) -> usize {
        self.backend.defragment()
    }

    /// Map externally-allocated chunk into ZShmMut.
    /// This method is designed to be used with push data sources.
    /// Remember that chunk's len may be >= len!
    #[zenoh_macros::unstable_doc]
    pub fn map(&self, chunk: AllocatedChunk, len: usize) -> ZResult<ZShmMut> {
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
        Ok(unsafe { ZShmMut::new_unchecked(wrapped) })
    }

    /// Try to collect free chunks.
    /// Returns the size of largest collected chunk
    #[zenoh_macros::unstable_doc]
    pub fn garbage_collect(&self) -> usize {
        fn is_free_chunk(chunk: &BusyChunk) -> bool {
            let header = chunk.header.descriptor.header();
            if header.refcount.load(Ordering::SeqCst) != 0 {
                return header.watchdog_invalidated.load(Ordering::SeqCst);
            }
            true
        }

        tracing::trace!("Running Garbage Collector");

        let mut largest = 0usize;
        let mut guard = self.busy_list.lock().unwrap();
        guard.retain(|maybe_free| {
            if is_free_chunk(maybe_free) {
                tracing::trace!("Garbage Collecting Chunk: {:?}", maybe_free);
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
    #[zenoh_macros::unstable_doc]
    pub fn available(&self) -> usize {
        self.backend.available()
    }
}

// PRIVATE impls
impl<IDSource, Backend> SharedMemoryProvider<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: SharedMemoryProviderBackend,
{
    fn new(backend: Backend, id: IDSource) -> Self {
        Self {
            backend,
            busy_list: Mutex::new(VecDeque::default()),
            id,
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
        Ok(unsafe { ZShmMut::new_unchecked(wrapped) })
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
            self.id.id(),
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
impl<IDSource, Backend> SharedMemoryProvider<IDSource, Backend>
where
    IDSource: ProtocolIDSource + Send + Sync,
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
        Ok(unsafe { ZShmMut::new_unchecked(wrapped) })
    }
}
