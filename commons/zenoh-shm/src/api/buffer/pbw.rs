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

use std::{marker::PhantomData, ops::{Deref, DerefMut}, sync::atomic::Ordering};

use crate::{
    api::provider::{
        chunk::AllocatedChunk, shm_provider::{AllocLayout, AllocPolicy, ProtocolIDSource}, shm_provider_backend::ShmProviderBackend, types::ZAllocError
    },
    promise::PromiseInner,
};

/// SHM buffer promise allocation result
#[zenoh_macros::unstable_doc]
pub type PromiseAllocResult = Result<ZAllocatedPromise, ZAllocError>;

/// SHM buffer promise creation result
#[zenoh_macros::unstable_doc]
pub type PromiseResult<'a, IDSource, Backend, Policy> =
    zenoh_core::Result<ZLayoutedPromise<'a, IDSource, Backend, Policy>>;

/// A layouted promise to publish SHM buffer
#[zenoh_macros::unstable_doc]
pub struct ZLayoutedPromise<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    layout: &'a AllocLayout<'a, IDSource, Backend>,
    pub(crate) inner: PromiseInner,
    _phantom: PhantomData<Policy>,
}

impl<'a, IDSource, Backend, Policy> ZLayoutedPromise<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    pub(crate) fn new(layout: &'a AllocLayout<'a, IDSource, Backend>, inner: PromiseInner) -> Self {
        Self {
            layout,
            inner,
            _phantom: PhantomData::default(),
        }
    }

    pub fn promise(&self) -> &ZPromise {
        unsafe { core::mem::transmute::<&PromiseInner, &ZPromise>(&self.inner) }
    }
}

impl<'a, IDSource, Backend, Policy> ZLayoutedPromise<'a, IDSource, Backend, Policy>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
    Policy: AllocPolicy
{
    pub fn alloc(self) -> PromiseAllocResult {
        self.layout.provider.alloc_promise_inner::<Policy>(&self.layout.provider_layout, self.inner)
    }
}

#[repr(transparent)]
pub struct ZPromise(pub(crate) PromiseInner);

/*
/// A builder for ZLayoutedPromise
#[zenoh_macros::unstable_doc]
pub struct ZPromiseBuilder<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    data: AllocData<'a, IDSource, Backend>,
}

impl<'a, IDSource, Backend> Resolvable for ZPromiseBuilder<'a, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    type To = PromiseLayoutResult<'a, IDSource, Backend>;
}

// Sync alloc policy
impl<IDSource, Backend> Wait for ZPromiseBuilder<'_, IDSource, Backend>
where
    IDSource: ProtocolIDSource,
    Backend: ShmProviderBackend,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let layout = AllocLayout::new(self.data)?;
        layout.make_promise().map_err(|_| ZLayoutError::IncorrectLayoutArgs)
    }
}
*/

/*
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
*/

/// A promise to publish SHM buffer
#[zenoh_macros::unstable_doc]
pub struct ZAllocatedPromise {
    inner: PromiseInner,
    chunk: AllocatedChunk
}

impl Drop for ZAllocatedPromise {
    fn drop(&mut self) {
        self.inner.publish(&self.chunk);
    }
}

impl ZAllocatedPromise {
    pub(crate) fn new(inner: PromiseInner, chunk: AllocatedChunk) -> Self {
        Self {inner, chunk}
    }
}

impl ZAllocatedPromise {
    pub fn publish(self) {
        self.inner.publish(&self.chunk);
    }
}

impl Deref for ZAllocatedPromise {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        let bp = self.chunk.data.load(Ordering::SeqCst);
        unsafe { std::slice::from_raw_parts(bp, self.inner.info.data_len.get()) }
    }
}

impl DerefMut for ZAllocatedPromise {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let bp = self.chunk.data.load(Ordering::SeqCst);
        unsafe { std::slice::from_raw_parts_mut(bp, self.inner.info.data_len.get()) }
    }
}

impl AsRef<[u8]> for ZAllocatedPromise {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl AsMut<[u8]> for ZAllocatedPromise {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}