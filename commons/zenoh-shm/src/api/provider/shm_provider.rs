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
    future::{Future, IntoFuture},
    marker::PhantomData,
    num::NonZeroUsize,
    pin::Pin,
    sync::{atomic::Ordering, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use zenoh_core::{zlock, zresult::ZResult, Resolvable, Wait};

use super::{
    chunk::{AllocatedChunk, ChunkDescriptor},
    shm_provider_backend::ShmProviderBackend,
    types::{
        AllocAlignment, BufAllocResult, BufLayoutAllocResult, ChunkAllocResult, ZAllocError,
        ZLayoutAllocError, ZLayoutError,
    },
};
use crate::{
    api::{
        buffer::zshmmut::ZShmMut,
        protocol_implementations::posix::posix_shm_provider_backend::PosixShmProviderBackend,
        provider::memory_layout::{IntoMemoryLayout, MemoryLayout},
    },
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

struct AllocData<'a, Backend, What>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
{
    what: What,
    provider: &'a ShmProvider<Backend>,
}

#[zenoh_macros::unstable_doc]
pub struct AllocBuilder<'a, Backend, What>(AllocData<'a, Backend, What>)
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout;

impl<'a, Backend, What> AllocBuilder<'a, Backend, What>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
{
    fn new(provider: &'a ShmProvider<Backend>, what: What) -> Self {
        Self(AllocData { what, provider })
    }

    /// Try to build an allocation layout
    #[zenoh_macros::unstable_doc]
    pub fn into_layout(self) -> Result<AllocLayout<'a, Backend, What>, ZLayoutError> {
        AllocLayout::new(self.0)
    }

    /// Set the allocation policy
    #[zenoh_macros::unstable_doc]
    pub fn with_policy<Policy>(self) -> ProviderAllocBuilder<'a, Backend, What, Policy> {
        ProviderAllocBuilder {
            data: self.0,
            _phantom: PhantomData,
        }
    }
}

#[zenoh_macros::unstable_doc]
impl<'a, Backend, What> Resolvable for AllocBuilder<'a, Backend, What>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
    ProviderAllocBuilder<'a, Backend, What>: Resolvable,
{
    type To = <ProviderAllocBuilder<'a, Backend, What> as Resolvable>::To;
}

// Sync alloc policy
impl<'a, Backend, What> Wait for AllocBuilder<'a, Backend, What>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
    ProviderAllocBuilder<'a, Backend, What>: Wait,
    Self: Resolvable<To = <ProviderAllocBuilder<'a, Backend, What> as Resolvable>::To>,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let builder = ProviderAllocBuilder::<'a, Backend, What> {
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
pub struct AllocLayout<'a, Backend, What>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
{
    size: NonZeroUsize,
    provider_layout: MemoryLayout,
    provider: &'a ShmProvider<Backend>,
    _phantom: PhantomData<What>,
}

impl<'a, Backend, What> AllocLayout<'a, Backend, What>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
{
    /// Allocate the new buffer with this layout
    #[zenoh_macros::unstable_doc]
    pub fn alloc<'b>(&'b self) -> LayoutAllocBuilder<'b, 'a, Backend, What> {
        LayoutAllocBuilder {
            layout: self,
            _phantom: PhantomData,
        }
    }

    fn new(data: AllocData<'a, Backend, What>) -> Result<Self, ZLayoutError> {
        // NOTE: Depending on internal implementation, provider's backend might relayout
        // the allocations for bigger alignment (ex. 4-byte aligned allocation to 8-bytes aligned)

        // calculate required layout
        let required_layout: MemoryLayout = data.what.try_into()?;
        let size = required_layout.size();

        // Obtain provider's layout for our layout
        let provider_layout = data
            .provider
            .backend
            .layout_for(required_layout)
            .map_err(|_| ZLayoutError::ProviderIncompatibleLayout)?;

        Ok(Self {
            size,
            provider_layout,
            provider: data.provider,
            _phantom: PhantomData::default(),
        })
    }
}

/// Trait for deallocation policies.
#[zenoh_macros::unstable_doc]
pub trait ForceDeallocPolicy {
    fn dealloc<Backend: ShmProviderBackend>(provider: &ShmProvider<Backend>) -> bool;
}

/// Try to dealloc optimal (currently eldest+1) chunk
#[zenoh_macros::unstable_doc]
pub struct DeallocOptimal;
impl ForceDeallocPolicy for DeallocOptimal {
    fn dealloc<Backend: ShmProviderBackend>(provider: &ShmProvider<Backend>) -> bool {
        let chunk_to_dealloc = {
            let mut guard = zlock!(provider.busy_list);
            match guard.len() {
                0 => return false,
                1 => guard.remove(0),
                _ => guard.swap_remove(1),
            }
        };
        provider.backend.free(&chunk_to_dealloc.descriptor());
        true
    }
}

/// Try to dealloc youngest chunk
#[zenoh_macros::unstable_doc]
pub struct DeallocYoungest;
impl ForceDeallocPolicy for DeallocYoungest {
    fn dealloc<Backend: ShmProviderBackend>(provider: &ShmProvider<Backend>) -> bool {
        match zlock!(provider.busy_list).pop() {
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
    fn dealloc<Backend: ShmProviderBackend>(provider: &ShmProvider<Backend>) -> bool {
        let mut guard = zlock!(provider.busy_list);
        match guard.is_empty() {
            true => false,
            false => {
                provider.backend.free(&guard.swap_remove(0).descriptor());
                true
            }
        }
    }
}

/// Trait for allocation policies
#[zenoh_macros::unstable_doc]
pub trait AllocPolicy {
    fn alloc<Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<Backend>,
    ) -> ChunkAllocResult;
}

/// Trait for async allocation policies
#[zenoh_macros::unstable_doc]
#[async_trait]
pub trait AsyncAllocPolicy: Send {
    async fn alloc_async<Backend: ShmProviderBackend + Sync>(
        layout: &MemoryLayout,
        provider: &ShmProvider<Backend>,
    ) -> ChunkAllocResult;
}

/// Just try to allocate
#[zenoh_macros::unstable_doc]
pub struct JustAlloc;
impl AllocPolicy for JustAlloc {
    fn alloc<Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<Backend>,
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
    fn alloc<Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<Backend>,
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
    fn alloc<Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<Backend>,
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
    fn alloc<Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<Backend>,
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
    async fn alloc_async<Backend: ShmProviderBackend + Sync>(
        layout: &MemoryLayout,
        provider: &ShmProvider<Backend>,
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
    fn alloc<Backend: ShmProviderBackend>(
        layout: &MemoryLayout,
        provider: &ShmProvider<Backend>,
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

    Backend: ShmProviderBackend,
> {
    provider: &'a ShmProvider< Backend>,
    allocations: lockfree::map::Map<std::ptr::NonNull<u8>, ShmBufInner>,
    _phantom: PhantomData<Policy>,
}

impl<'a, Policy: AllocPolicy,  Backend: ShmProviderBackend>
    ShmAllocator<'a, Policy,  Backend>
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

unsafe impl<'a, Policy: AllocPolicy,  Backend: ShmProviderBackend>
    allocator_api2::alloc::Allocator for ShmAllocator<'a, Policy,  Backend>
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
pub struct ProviderAllocBuilder<'a, Backend, What, Policy = JustAlloc>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
{
    data: AllocData<'a, Backend, What>,
    _phantom: PhantomData<Policy>,
}

// Generic impl
impl<'a, Backend, What, Policy> ProviderAllocBuilder<'a, Backend, What, Policy>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
{
    /// Set the allocation policy
    #[zenoh_macros::unstable_doc]
    pub fn with_policy<OtherPolicy>(self) -> ProviderAllocBuilder<'a, Backend, What, OtherPolicy> {
        ProviderAllocBuilder {
            data: self.data,
            _phantom: PhantomData,
        }
    }
}

impl<'b, 'a: 'b, Backend, What, Policy> Resolvable
    for ProviderAllocBuilder<'_, Backend, What, Policy>
where
    Backend: ShmProviderBackend + 'a,
    What: IntoMemoryLayout + 'a,
    LayoutAllocBuilder<'b, 'a, Backend, What, Policy>: Resolvable<To = BufAllocResult>,
{
    type To = BufLayoutAllocResult;
}

// Sync alloc policy
impl<'a, Backend, What, Policy> Wait for ProviderAllocBuilder<'a, Backend, What, Policy>
where
    Backend: ShmProviderBackend + 'a,
    What: IntoMemoryLayout + 'a,
    Policy: AllocPolicy,
    for<'b> LayoutAllocBuilder<'b, 'a, Backend, What, Policy>: Resolvable<To = BufAllocResult>,
{
    fn wait(self) -> <Self as Resolvable>::To {
        AllocLayout::new(self.data)
            .map_err(ZLayoutAllocError::Layout)?
            .alloc()
            .with_policy::<Policy>()
            .wait()
            .map_err(ZLayoutAllocError::Alloc)
    }
}

// Async alloc policy
impl<'a, Backend, What, Policy> IntoFuture for ProviderAllocBuilder<'a, Backend, What, Policy>
where
    Backend: ShmProviderBackend + Sync,
    What: IntoMemoryLayout + Send + Sync + 'a,
    Policy: AsyncAllocPolicy,
    ProviderAllocBuilder<'a, Backend, What, Policy>: Resolvable<To = BufLayoutAllocResult>,
    for<'b> LayoutAllocBuilder<'b, 'a, Backend, What, Policy>:
        IntoFuture<Output = BufAllocResult> + Send,
    for<'b, 'c> <LayoutAllocBuilder<'b, 'c, Backend, What, Policy> as IntoFuture>::IntoFuture: Send,
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
pub struct LayoutAllocBuilder<'b, 'a: 'b, Backend, What, Policy = JustAlloc>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
{
    layout: &'b AllocLayout<'a, Backend, What>,
    _phantom: PhantomData<Policy>,
}

// Generic impl
impl<'b, 'a: 'b, Backend, What, Policy> LayoutAllocBuilder<'b, 'a, Backend, What, Policy>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
{
    /// Set the allocation policy
    #[zenoh_macros::unstable_doc]
    pub fn with_policy<OtherPolicy>(
        self,
    ) -> LayoutAllocBuilder<'b, 'a, Backend, What, OtherPolicy> {
        LayoutAllocBuilder {
            layout: self.layout,
            _phantom: PhantomData,
        }
    }
}

impl<'b, 'a: 'b, Backend, Policy> Resolvable
    for LayoutAllocBuilder<'b, 'a, Backend, NonZeroUsize, Policy>
where
    Backend: ShmProviderBackend,
{
    type To = BufAllocResult;
}

impl<'b, 'a: 'b, Backend, Policy> Resolvable
    for LayoutAllocBuilder<'b, 'a, Backend, usize, Policy>
where
    Backend: ShmProviderBackend,
{
    type To = BufAllocResult;
}

impl<'b, 'a: 'b, Backend, Policy> Resolvable
    for LayoutAllocBuilder<'b, 'a, Backend, (usize, AllocAlignment), Policy>
where
    Backend: ShmProviderBackend,
{
    type To = BufAllocResult;
}

impl<'b, 'a: 'b, Backend, Policy> Resolvable
    for LayoutAllocBuilder<'b, 'a, Backend, (NonZeroUsize, AllocAlignment), Policy>
where
    Backend: ShmProviderBackend,
{
    type To = BufAllocResult;
}

/*
impl<Backend, T, Policy> Resolvable for LayoutAllocBuilder<'_, Backend, LayoutForType<T>, Policy>
where
    Backend: ShmProviderBackend,
{
    type To = TypedBufLayoutAllocResult<T>;
}*/

// Sync alloc policy
impl<'b, 'a: 'b, Backend, What, Policy> Wait for LayoutAllocBuilder<'b, 'a, Backend, What, Policy>
where
    Backend: ShmProviderBackend,
    What: IntoMemoryLayout,
    Policy: AllocPolicy,
    Self: Resolvable<To = BufAllocResult>,
{
    fn wait(self) -> <Self as Resolvable>::To {
        self.layout
            .provider
            .alloc_inner::<Policy>(self.layout.size, &self.layout.provider_layout)
    }
}

// Async alloc policy
impl<'a, Backend, What, Policy> IntoFuture for LayoutAllocBuilder<'a, 'a, Backend, What, Policy>
where
    Backend: ShmProviderBackend + Sync,
    What: IntoMemoryLayout + Send + Sync + 'a,
    Policy: AsyncAllocPolicy,
    LayoutAllocBuilder<'a, 'a, Backend, What, Policy>: Resolvable<To = BufAllocResult>,
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
    /// Set the backend
    #[zenoh_macros::unstable_doc]
    pub fn backend<Backend: ShmProviderBackend>(
        backend: Backend,
    ) -> ShmProviderBuilderBackend<Backend> {
        ShmProviderBuilderBackend { backend }
    }

    /// Set the default backend
    #[zenoh_macros::unstable_doc]
    pub fn default_backend(size: usize) -> ShmProviderBuilderWithDefaultBackend {
        ShmProviderBuilderWithDefaultBackend { size }
    }
}

#[zenoh_macros::unstable_doc]
pub struct ShmProviderBuilderBackend<Backend>
where
    Backend: ShmProviderBackend,
{
    backend: Backend,
}
#[zenoh_macros::unstable_doc]
impl<Backend> Resolvable for ShmProviderBuilderBackend<Backend>
where
    Backend: ShmProviderBackend,
{
    type To = ShmProvider<Backend>;
}

#[zenoh_macros::unstable_doc]
impl<Backend> Wait for ShmProviderBuilderBackend<Backend>
where
    Backend: ShmProviderBackend,
{
    /// build ShmProvider
    fn wait(self) -> <Self as Resolvable>::To {
        ShmProvider::new(self.backend)
    }
}

#[zenoh_macros::unstable_doc]
pub struct ShmProviderBuilderWithDefaultBackend {
    size: usize,
}

impl ShmProviderBuilderWithDefaultBackend {
    pub fn with_alignment(
        self,
        alignment: AllocAlignment,
    ) -> ShmProviderBuilderWithDefaultBackendWithAlignment {
        ShmProviderBuilderWithDefaultBackendWithAlignment {
            size: self.size,
            alignment,
        }
    }
}

#[zenoh_macros::unstable_doc]
impl Resolvable for ShmProviderBuilderWithDefaultBackend {
    type To = ZResult<ShmProvider<PosixShmProviderBackend>>;
}

#[zenoh_macros::unstable_doc]
impl Wait for ShmProviderBuilderWithDefaultBackend {
    /// build ShmProvider
    fn wait(self) -> <Self as Resolvable>::To {
        let backend = PosixShmProviderBackend::builder()
            .with_size(self.size)
            .wait()?;

        Ok(ShmProvider::new(backend))
    }
}

#[zenoh_macros::unstable_doc]
pub struct ShmProviderBuilderWithDefaultBackendWithAlignment {
    size: usize,
    alignment: AllocAlignment,
}

#[zenoh_macros::unstable_doc]
impl Resolvable for ShmProviderBuilderWithDefaultBackendWithAlignment {
    type To = ZResult<ShmProvider<PosixShmProviderBackend>>;
}

#[zenoh_macros::unstable_doc]
impl Wait for ShmProviderBuilderWithDefaultBackendWithAlignment {
    /// build ShmProvider
    fn wait(self) -> <Self as Resolvable>::To {
        let layout = MemoryLayout::new(self.size, self.alignment)?;
        let backend = PosixShmProviderBackend::builder()
            .with_layout(layout)
            .wait()?;
        Ok(ShmProvider::new(backend))
    }
}

/// A generalized interface for shared memory data sources
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub struct ShmProvider<Backend>
where
    Backend: ShmProviderBackend,
{
    backend: Backend,
    busy_list: Mutex<Vec<BusyChunk>>,
}

impl<Backend: ShmProviderBackend + Default> Default for ShmProvider<Backend> {
    fn default() -> Self {
        Self::new(Backend::default())
    }
}

impl<Backend> ShmProvider<Backend>
where
    Backend: ShmProviderBackend,
{
    /// Rich interface for making allocations
    #[zenoh_macros::unstable_doc]
    pub fn alloc<What: IntoMemoryLayout>(&self, what: What) -> AllocBuilder<'_, Backend, What> {
        AllocBuilder::new(self, what)
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

        fn retain_unordered<T>(vec: &mut Vec<T>, mut f: impl FnMut(&T) -> bool) {
            let mut i = 0;
            while i < vec.len() {
                if f(&vec[i]) {
                    i += 1;
                } else {
                    vec.swap_remove(i); // move last into place of vec[i]
                                        // don't increment i: need to test the swapped-in element
                }
            }
        }

        tracing::trace!("Running Garbage Collector");

        let mut largest = 0usize;
        let mut guard = self.busy_list.lock().unwrap();

        retain_unordered(&mut guard, |maybe_free| {
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
impl<Backend> ShmProvider<Backend>
where
    Backend: ShmProviderBackend,
{
    fn new(backend: Backend) -> Self {
        Self {
            backend,
            busy_list: Mutex::new(Vec::default()),
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
            .store(self.backend.id(), Ordering::Relaxed);

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
        zlock!(self.busy_list).push(BusyChunk::new(allocated_metadata));

        shmb
    }
}

// PRIVATE impls for Sync backend
impl<Backend> ShmProvider<Backend>
where
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
