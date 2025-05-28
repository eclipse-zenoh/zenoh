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
    future::{Future, IntoFuture},
    marker::PhantomData,
    num::NonZeroUsize,
    pin::Pin,
    sync::{atomic::Ordering, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use zenoh_core::{Resolvable, Wait};

use super::{
    chunk::{AllocatedChunk, ChunkDescriptor},
    shm_provider_backend::ShmProviderBackend,
    types::{
        AllocAlignment, BufAllocResult, BufLayoutAllocResult, ChunkAllocResult, MemoryLayout,
        ZAllocError, ZLayoutAllocError, ZLayoutError,
    },
};
use crate::{
    api::{buffer::zshmmut::ZShmMut, common::types::ProtocolID},
    metadata::{
        allocated_descriptor::AllocatedMetadataDescriptor, descriptor::MetadataDescriptor,
        storage::GLOBAL_METADATA_STORAGE,
    },
    watchdog::{
        confirmator::{ConfirmedDescriptor, GLOBAL_CONFIRMATOR},
        validator::GLOBAL_VALIDATOR,
    },
    ShmBufInfo, ShmBufInner,
};

#[derive(Debug)]
struct BusyChunk {
    metadata: AllocatedMetadataDescriptor,
}

impl BusyChunk {
    fn new(metadata: AllocatedMetadataDescriptor) -> Self {
        Self { metadata }
    }

    fn descriptor(&self) -> ChunkDescriptor {
        self.metadata.header().data_descriptor()
    }
}

struct AllocData<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    size: usize,
    alignment: AllocAlignment,
    provider: &'a ShmProvider<IDSource, Backend>,
}

#[zenoh_macros::unstable_doc]
pub struct AllocLayoutSizedBuilder<'a, IDSource, Backend>(AllocData<'a, IDSource, Backend>)
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend;

impl<'a, IDSource, Backend> AllocLayoutSizedBuilder<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    fn new(provider: &'a ShmProvider<IDSource, Backend>, size: usize) -> Self {
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
    pub fn into_layout(self) -> Result<AllocLayout<'a, IDSource, Backend>, ZLayoutError> {
        AllocLayout::new(self.0)
    }

    /// Set the allocation policy
    #[zenoh_macros::unstable_doc]
    pub fn with_policy<Policy>(self) -> ProviderAllocBuilder<'a, IDSource, Backend, Policy> {
        ProviderAllocBuilder {
            data: self.0,
            _phantom: PhantomData,
        }
    }
}

#[zenoh_macros::unstable_doc]
impl<IDSource, Backend> Resolvable for AllocLayoutSizedBuilder<'_, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    type To = BufLayoutAllocResult;
}

// Sync alloc policy
impl<'a, IDSource, Backend> Wait for AllocLayoutSizedBuilder<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let builder = ProviderAllocBuilder::<'a, IDSource, Backend, JustAlloc> {
            data: self.0,
            _phantom: PhantomData,
        };
        builder.wait()
    }
}

/// A layout for allocations.
/// This is a pre-calculated layout suitable for making series of similar allocations
/// adopted for particular ShmProvider
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub struct AllocLayout<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    size: NonZeroUsize,
    provider_layout: MemoryLayout,
    provider: &'a ShmProvider<IDSource, Backend>,
}

impl<'a, IDSource, Backend> AllocLayout<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    /// Allocate the new buffer with this layout
    #[zenoh_macros::unstable_doc]
    pub fn alloc(&'a self) -> LayoutAllocBuilder<'a, IDSource, Backend> {
        LayoutAllocBuilder {
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
        let size = layout.size();

        // Obtain provider's layout for our layout
        let provider_layout = data
            .provider
            .backend
            .layout_for(layout)
            .map_err(|_| ZLayoutError::ProviderIncompatibleLayout)?;

        Ok(Self {
            size,
            provider_layout,
            provider: data.provider,
        })
    }
}

/// Trait for deallocation policies.
#[zenoh_macros::unstable_doc]
pub trait ForceDeallocPolicy {
    fn dealloc<IDSource: ProtocolIDSource, Backend: ShmProviderBackend>(
        provider: &ShmProvider<IDSource, Backend>,
    ) -> bool;
}

/// Try to dealloc optimal (currently eldest+1) chunk
#[zenoh_macros::unstable_doc]
pub struct DeallocOptimal;
impl ForceDeallocPolicy for DeallocOptimal {
    fn dealloc<IDSource: ProtocolIDSource, Backend: ShmProviderBackend>(
        provider: &ShmProvider<IDSource, Backend>,
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

        provider.backend.free(&chunk_to_dealloc.descriptor());
        true
    }
}

/// Try to dealloc youngest chunk
#[zenoh_macros::unstable_doc]
pub struct DeallocYoungest;
impl ForceDeallocPolicy for DeallocYoungest {
    fn dealloc<IDSource: ProtocolIDSource, Backend: ShmProviderBackend>(
        provider: &ShmProvider<IDSource, Backend>,
    ) -> bool {
        match provider.busy_list.lock().unwrap().pop_back() {
            Some(val) => {
                provider.backend.free(&val.descriptor());
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
    fn dealloc<IDSource: ProtocolIDSource, Backend: ShmProviderBackend>(
        provider: &ShmProvider<IDSource, Backend>,
    ) -> bool {
        match provider.busy_list.lock().unwrap().pop_front() {
            Some(val) => {
                provider.backend.free(&val.descriptor());
                true
            }
            None => false,
        }
    }
}

/// Trait for allocation policies
#[zenoh_macros::unstable_doc]
pub trait AllocPolicy {
    fn alloc<IDSource: ProtocolIDSource, Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<IDSource, Backend>,
    ) -> ChunkAllocResult;
}

/// Trait for async allocation policies
#[zenoh_macros::unstable_doc]
#[async_trait]
pub trait AsyncAllocPolicy: Send {
    async fn alloc_async<IDSource: ProtocolIDSource, Backend: ShmProviderBackend + Sync>(
        layout: &MemoryLayout,
        provider: &ShmProvider<IDSource, Backend>,
    ) -> ChunkAllocResult;
}

/// Just try to allocate
#[zenoh_macros::unstable_doc]
pub struct JustAlloc;
impl AllocPolicy for JustAlloc {
    fn alloc<IDSource: ProtocolIDSource, Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<IDSource, Backend>,
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
    fn alloc<IDSource: ProtocolIDSource, Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<IDSource, Backend>,
    ) -> ChunkAllocResult {
        let result = InnerPolicy::alloc(layout, provider);
        if result.is_err() {
            // try to alloc again only if GC managed to reclaim big enough chunk
            if provider.garbage_collect() >= layout.size().get() {
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
    fn alloc<IDSource: ProtocolIDSource, Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<IDSource, Backend>,
    ) -> ChunkAllocResult {
        let result = InnerPolicy::alloc(layout, provider);
        if let Err(ZAllocError::NeedDefragment) = result {
            // try to alloc again only if big enough chunk was defragmented
            if provider.defragment() >= layout.size().get() {
                return AltPolicy::alloc(layout, provider);
            }
        }
        result
    }
}

/// Deallocating policy.
/// Forcibly deallocate up to N buffers until allocation succeeds.
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
    fn alloc<IDSource: ProtocolIDSource, Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<IDSource, Backend>,
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
    InnerPolicy: AllocPolicy + Send,
{
    async fn alloc_async<IDSource: ProtocolIDSource, Backend: ShmProviderBackend + Sync>(
        layout: &MemoryLayout,
        provider: &ShmProvider<IDSource, Backend>,
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
    fn alloc<IDSource: ProtocolIDSource, Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<IDSource, Backend>,
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
    Backend: ShmProviderBackend,
> {
    provider: &'a ShmProvider<IDSource, Backend>,
    allocations: lockfree::map::Map<std::ptr::NonNull<u8>, ShmBufInner>,
    _phantom: PhantomData<Policy>,
}

impl<'a, Policy: AllocPolicy, IDSource, Backend: ShmProviderBackend>
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

unsafe impl<'a, Policy: AllocPolicy, IDSource, Backend: ShmProviderBackend>
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

/// Builder for making allocations with instant layout calculation
#[zenoh_macros::unstable_doc]
pub struct ProviderAllocBuilder<
    'a,
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
    Policy = JustAlloc,
> {
    data: AllocData<'a, IDSource, Backend>,
    _phantom: PhantomData<Policy>,
}

// Generic impl
impl<'a, IDSource, Backend, Policy> ProviderAllocBuilder<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    /// Set the allocation policy
    #[zenoh_macros::unstable_doc]
    pub fn with_policy<OtherPolicy>(
        self,
    ) -> ProviderAllocBuilder<'a, IDSource, Backend, OtherPolicy> {
        ProviderAllocBuilder {
            data: self.data,
            _phantom: PhantomData,
        }
    }
}

impl<IDSource, Backend, Policy> Resolvable for ProviderAllocBuilder<'_, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    type To = BufLayoutAllocResult;
}

// Sync alloc policy
impl<IDSource, Backend, Policy> Wait for ProviderAllocBuilder<'_, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
    Policy: AllocPolicy,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let layout = AllocLayout::new(self.data).map_err(ZLayoutAllocError::Layout)?;

        layout
            .alloc()
            .with_policy::<Policy>()
            .wait()
            .map_err(ZLayoutAllocError::Alloc)
    }
}

// Async alloc policy
impl<'a, IDSource, Backend, Policy> IntoFuture
    for ProviderAllocBuilder<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend + Sync,
    Policy: AsyncAllocPolicy,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Pin<Box<dyn Future<Output = <Self as IntoFuture>::Output> + 'a + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(
            async move {
                let layout = AllocLayout::new(self.data).map_err(ZLayoutAllocError::Layout)?;
                layout
                    .alloc()
                    .with_policy::<Policy>()
                    .await
                    .map_err(ZLayoutAllocError::Alloc)
            }
            .into_future(),
        )
    }
}

/// Builder for making allocations through precalculated Layout
#[zenoh_macros::unstable_doc]
pub struct LayoutAllocBuilder<
    'a,
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
    Policy = JustAlloc,
> {
    layout: &'a AllocLayout<'a, IDSource, Backend>,
    _phantom: PhantomData<Policy>,
}

// Generic impl
impl<'a, IDSource, Backend, Policy> LayoutAllocBuilder<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    /// Set the allocation policy
    #[zenoh_macros::unstable_doc]
    pub fn with_policy<OtherPolicy>(
        self,
    ) -> LayoutAllocBuilder<'a, IDSource, Backend, OtherPolicy> {
        LayoutAllocBuilder {
            layout: self.layout,
            _phantom: PhantomData,
        }
    }
}

impl<IDSource, Backend, Policy> Resolvable for LayoutAllocBuilder<'_, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    type To = BufAllocResult;
}

// Sync alloc policy
impl<IDSource, Backend, Policy> Wait for LayoutAllocBuilder<'_, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
    Policy: AllocPolicy,
{
    fn wait(self) -> <Self as Resolvable>::To {
        self.layout
            .provider
            .alloc_inner::<Policy>(self.layout.size, &self.layout.provider_layout)
    }
}

// Async alloc policy
impl<'a, IDSource, Backend, Policy> IntoFuture for LayoutAllocBuilder<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend + Sync,
    Policy: AsyncAllocPolicy,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Pin<Box<dyn Future<Output = <Self as Resolvable>::To> + 'a + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(
            async move {
                self.layout
                    .provider
                    .alloc_inner_async::<Policy>(self.layout.size, &self.layout.provider_layout)
                    .await
            }
            .into_future(),
        )
    }
}

#[zenoh_macros::unstable_doc]
pub struct ShmProviderBuilder;
impl ShmProviderBuilder {
    /// Get the builder to construct ShmProvider
    #[zenoh_macros::unstable_doc]
    pub fn builder() -> Self {
        Self
    }

    /// Set compile-time-evaluated protocol ID (preferred)
    #[zenoh_macros::unstable_doc]
    pub fn protocol_id<const ID: ProtocolID>(self) -> ShmProviderBuilderID<StaticProtocolID<ID>> {
        ShmProviderBuilderID::<StaticProtocolID<ID>> {
            id: StaticProtocolID,
        }
    }

    /// Set runtime-evaluated protocol ID
    #[zenoh_macros::unstable_doc]
    pub fn dynamic_protocol_id(self, id: ProtocolID) -> ShmProviderBuilderID<DynamicProtocolID> {
        ShmProviderBuilderID::<DynamicProtocolID> {
            id: DynamicProtocolID::new(id),
        }
    }
}

#[zenoh_macros::unstable_doc]
pub struct ShmProviderBuilderID<IDSource: ProtocolIDSource> {
    id: IDSource,
}
impl<IDSource: ProtocolIDSource> ShmProviderBuilderID<IDSource> {
    /// Set the backend
    #[zenoh_macros::unstable_doc]
    pub fn backend<Backend: ShmProviderBackend>(
        self,
        backend: Backend,
    ) -> ShmProviderBuilderBackendID<IDSource, Backend> {
        ShmProviderBuilderBackendID {
            backend,
            id: self.id,
        }
    }
}

#[zenoh_macros::unstable_doc]
pub struct ShmProviderBuilderBackendID<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    backend: Backend,
    id: IDSource,
}
#[zenoh_macros::unstable_doc]
impl<IDSource, Backend> Resolvable for ShmProviderBuilderBackendID<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    type To = ShmProvider<IDSource, Backend>;
}

#[zenoh_macros::unstable_doc]
impl<IDSource, Backend> Wait for ShmProviderBuilderBackendID<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    /// build ShmProvider
    fn wait(self) -> <Self as Resolvable>::To {
        ShmProvider::new(self.backend, self.id)
    }
}

/// Trait to create ProtocolID sources for ShmProvider
#[zenoh_macros::unstable_doc]
pub trait ProtocolIDSource: Send + Sync {
    fn id(&self) -> ProtocolID;
}

/// Static ProtocolID source. This is a recommended API to set ProtocolID
/// when creating ShmProvider as the ID value is statically evaluated
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
/// when creating ShmProvider for cases where ProtocolID is unknown
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
pub struct ShmProvider<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    backend: Backend,
    busy_list: Mutex<VecDeque<BusyChunk>>,
    id: IDSource,
}

impl<IDSource, Backend> ShmProvider<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
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
    pub fn map(&self, chunk: AllocatedChunk, len: usize) -> Result<ZShmMut, ZAllocError> {
        let len = len.try_into().map_err(|_| ZAllocError::Other)?;

        // allocate resources for SHM buffer
        let (allocated_metadata, confirmed_metadata) = Self::alloc_resources()?;

        // wrap everything to ShmBufInner
        let wrapped = self.wrap(chunk, len, allocated_metadata, confirmed_metadata);
        Ok(unsafe { ZShmMut::new_unchecked(wrapped) })
    }

    /// Try to collect free chunks.
    /// Returns the size of largest collected chunk
    #[zenoh_macros::unstable_doc]
    pub fn garbage_collect(&self) -> usize {
        fn is_free_chunk(chunk: &BusyChunk) -> bool {
            let header = chunk.metadata.header();
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
                let descriptor_to_free = maybe_free.descriptor();
                self.backend.free(&descriptor_to_free);
                largest = largest.max(descriptor_to_free.len.get());
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
impl<IDSource, Backend> ShmProvider<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    fn new(backend: Backend, id: IDSource) -> Self {
        Self {
            backend,
            busy_list: Mutex::new(VecDeque::default()),
            id,
        }
    }

    fn alloc_inner<Policy>(&self, size: NonZeroUsize, layout: &MemoryLayout) -> BufAllocResult
    where
        Policy: AllocPolicy,
    {
        // allocate resources for SHM buffer
        let (allocated_metadata, confirmed_metadata) = Self::alloc_resources()?;

        // allocate data chunk
        // Perform actions depending on the Policy
        // NOTE: it is necessary to properly map this chunk OR free it if mapping fails!
        // Don't loose this chunk as it leads to memory leak at the backend side!
        // NOTE: self.backend.alloc(len) returns chunk with len >= required len,
        // and it is necessary to handle that properly and pass this len to corresponding free(...)
        let chunk = Policy::alloc(layout, self)?;

        // wrap allocated chunk to ShmBufInner
        let wrapped = self.wrap(chunk, size, allocated_metadata, confirmed_metadata);
        Ok(unsafe { ZShmMut::new_unchecked(wrapped) })
    }

    fn alloc_resources() -> Result<(AllocatedMetadataDescriptor, ConfirmedDescriptor), ZAllocError>
    {
        // allocate metadata
        let allocated_metadata = GLOBAL_METADATA_STORAGE.read().allocate()?;

        // add watchdog to confirmator
        let confirmed_metadata = GLOBAL_CONFIRMATOR.read().add(allocated_metadata.clone());

        Ok((allocated_metadata, confirmed_metadata))
    }

    fn wrap(
        &self,
        chunk: AllocatedChunk,
        len: NonZeroUsize,
        allocated_metadata: AllocatedMetadataDescriptor,
        confirmed_metadata: ConfirmedDescriptor,
    ) -> ShmBufInner {
        // write additional metadata
        // chunk descriptor
        allocated_metadata
            .header()
            .set_data_descriptor(&chunk.descriptor);
        // protocol
        allocated_metadata
            .header()
            .protocol
            .store(self.id.id(), Ordering::Relaxed);

        // add watchdog to validator
        GLOBAL_VALIDATOR
            .read()
            .add(confirmed_metadata.owned.clone());

        // Create buffer's info
        let info = ShmBufInfo::new(
            len,
            MetadataDescriptor::from(&confirmed_metadata.owned),
            allocated_metadata
                .header()
                .generation
                .load(Ordering::SeqCst),
        );

        // Create buffer
        let shmb = ShmBufInner {
            metadata: confirmed_metadata,
            buf: chunk.data,
            info,
        };

        // Create and store busy chunk
        self.busy_list
            .lock()
            .unwrap()
            .push_back(BusyChunk::new(allocated_metadata));

        shmb
    }
}

// PRIVATE impls for Sync backend
impl<IDSource, Backend> ShmProvider<IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend + Sync,
{
    async fn alloc_inner_async<Policy>(
        &self,
        size: NonZeroUsize,
        backend_layout: &MemoryLayout,
    ) -> BufAllocResult
    where
        Policy: AsyncAllocPolicy,
    {
        // allocate resources for SHM buffer
        let (allocated_metadata, confirmed_metadata) = Self::alloc_resources()?;

        // allocate data chunk
        // Perform actions depending on the Policy
        // NOTE: it is necessary to properly map this chunk OR free it if mapping fails!
        // Don't loose this chunk as it leads to memory leak at the backend side!
        // NOTE: self.backend.alloc(len) returns chunk with len >= required len,
        // and it is necessary to handle that properly and pass this len to corresponding free(...)
        let chunk = Policy::alloc_async(backend_layout, self).await?;

        // wrap allocated chunk to ShmBufInner
        let wrapped = self.wrap(chunk, size, allocated_metadata, confirmed_metadata);
        Ok(unsafe { ZShmMut::new_unchecked(wrapped) })
    }
}
