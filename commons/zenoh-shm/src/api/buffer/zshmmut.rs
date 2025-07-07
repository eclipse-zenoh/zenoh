//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.inner which is available at
// http://www.eclipse.org/legal/epl-2.inner, or the Apache License, Version 2.inner
// which is available at https://www.apache.org/licenses/LICENSE-2.inner.
//
// SPDX-License-Identifier: EPL-2.inner OR Apache-2.inner
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

use core::ops::{Deref, DerefMut};
use std::{
    borrow::{Borrow, BorrowMut},
    marker::PhantomData,
    num::NonZeroUsize,
};

use zenoh_buffers::{ZBuf, ZSlice};

use super::{
    traits::{BufferRelayoutError, OwnedShmBuf, ShmBuf, ShmBufMut},
    zshm::{zshm, ZShm},
};
use crate::{
    api::{
        buffer::traits::{ResideInShm, ShmBufUnsafeMut},
        provider::types::MemoryLayout,
    },
    ShmBufInner,
};

/// A mutable SHM buffer
#[zenoh_macros::unstable_doc]
#[derive(Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct ZShmMut<T: ?Sized> {
    pub(crate) inner: ShmBufInner,
    pub(crate) _phantom: PhantomData<T>,
}

impl<T: ResideInShm> ShmBuf<T> for ZShmMut<T> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl ShmBuf<[u8]> for ZShmMut<[u8]> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl<T: ResideInShm> ShmBufUnsafeMut<T> for ZShmMut<T> {
    unsafe fn as_mut_unchecked(&mut self) -> &mut T {
        let slice = self.inner.as_mut_slice_inner();
        unsafe { &mut *(slice.as_mut_ptr() as *mut T) }
    }
}

impl ShmBufUnsafeMut<[u8]> for ZShmMut<[u8]> {
    unsafe fn as_mut_unchecked(&mut self) -> &mut [u8] {
        self.inner.as_mut_slice_inner()
    }
}

impl<T: ResideInShm> ShmBufMut<T> for ZShmMut<T> {}
impl ShmBufMut<[u8]> for ZShmMut<[u8]> {}

impl<T: ResideInShm> OwnedShmBuf<T> for ZShmMut<T> {
    fn try_resize(&mut self, new_size: NonZeroUsize) -> Option<()> {
        // Safety: this is safe because ZShmMut is an owned representation of SHM buffer and thus
        // is guaranteed not to be wrapped into ZSlice (see ShmBufInner::try_resize comment)
        unsafe { self.inner.try_resize(new_size) }
    }

    fn try_relayout(&mut self, new_layout: MemoryLayout) -> Result<(), BufferRelayoutError> {
        // Safety: this is safe because ZShmMut is an owned representation of SHM buffer and thus
        // is guaranteed not to be wrapped into ZSlice (see ShmBufInner::try_resize comment)
        unsafe { self.inner.try_relayout(new_layout) }
    }
}

impl OwnedShmBuf<[u8]> for ZShmMut<[u8]> {
    fn try_resize(&mut self, new_size: NonZeroUsize) -> Option<()> {
        // Safety: this is safe because ZShmMut is an owned representation of SHM buffer and thus
        // is guaranteed not to be wrapped into ZSlice (see ShmBufInner::try_resize comment)
        unsafe { self.inner.try_resize(new_size) }
    }

    fn try_relayout(&mut self, new_layout: MemoryLayout) -> Result<(), BufferRelayoutError> {
        // Safety: this is safe because ZShmMut is an owned representation of SHM buffer and thus
        // is guaranteed not to be wrapped into ZSlice (see ShmBufInner::try_resize comment)
        unsafe { self.inner.try_relayout(new_layout) }
    }
}

impl<T: ?Sized> PartialEq<zshmmut<T>> for &ZShmMut<T> {
    fn eq(&self, other: &zshmmut<T>) -> bool {
        self.inner == other.inner
    }
}

impl<T: ?Sized> Borrow<zshm<T>> for ZShmMut<T> {
    fn borrow(&self) -> &zshm<T> {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl<T: ?Sized> BorrowMut<zshm<T>> for ZShmMut<T> {
    fn borrow_mut(&mut self) -> &mut zshm<T> {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl<T: ?Sized> Borrow<zshmmut<T>> for ZShmMut<T> {
    fn borrow(&self) -> &zshmmut<T> {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl<T: ?Sized> BorrowMut<zshmmut<T>> for ZShmMut<T> {
    fn borrow_mut(&mut self) -> &mut zshmmut<T> {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl<T: ResideInShm> Deref for ZShmMut<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let slice = self.inner.as_ref();
        unsafe { &*(slice.as_ptr() as *const T) }
    }
}

impl Deref for ZShmMut<[u8]> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

impl<T: ResideInShm> DerefMut for ZShmMut<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let slice = self.inner.as_mut();
        unsafe { &mut *(slice.as_mut_ptr() as *mut T) }
    }
}

impl DerefMut for ZShmMut<[u8]> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut()
    }
}

impl AsRef<[u8]> for ZShmMut<[u8]> {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl<T: ResideInShm> AsRef<T> for ZShmMut<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl AsMut<[u8]> for ZShmMut<[u8]> {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}

impl<T: ResideInShm> AsMut<T> for ZShmMut<T> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

impl<T: ?Sized> From<ZShmMut<T>> for ZShm<T> {
    fn from(value: ZShmMut<T>) -> Self {
        // SAFETY: this is safe because Self already guarantees inner to be T
        unsafe { ZShm::new_unchecked(value.inner) }
    }
}

impl<T: ?Sized> From<ZShmMut<T>> for ZSlice {
    fn from(value: ZShmMut<T>) -> Self {
        value.inner.into()
    }
}

impl<T: ?Sized> From<ZShmMut<T>> for ZBuf {
    fn from(value: ZShmMut<T>) -> Self {
        value.inner.into()
    }
}

/// A borrowed mutable SHM buffer
#[zenoh_macros::unstable_doc]
#[derive(Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct zshmmut<T: ?Sized> {
    pub(crate) inner: ShmBufInner,
    _phantom: PhantomData<T>,
}

impl<T: ?Sized> PartialEq<ZShmMut<T>> for &zshmmut<T> {
    fn eq(&self, other: &ZShmMut<T>) -> bool {
        self.inner == other.inner
    }
}

impl<T: ResideInShm> ShmBuf<T> for &zshmmut<T> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl ShmBuf<[u8]> for &zshmmut<[u8]> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl<T: ResideInShm> ShmBuf<T> for &mut zshmmut<T> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl ShmBuf<[u8]> for &mut zshmmut<[u8]> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl<T: ResideInShm> ShmBufUnsafeMut<T> for &mut zshmmut<T> {
    unsafe fn as_mut_unchecked(&mut self) -> &mut T {
        let slice = self.inner.as_mut_slice_inner();
        unsafe { &mut *(slice.as_mut_ptr() as *mut T) }
    }
}

impl ShmBufUnsafeMut<[u8]> for &mut zshmmut<[u8]> {
    unsafe fn as_mut_unchecked(&mut self) -> &mut [u8] {
        self.inner.as_mut_slice_inner()
    }
}

impl<T: ResideInShm> ShmBufMut<T> for &mut zshmmut<T> {}
impl ShmBufMut<[u8]> for &mut zshmmut<[u8]> {}

impl Deref for zshmmut<[u8]> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

impl<T: ResideInShm> Deref for zshmmut<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let slice = self.inner.as_ref();
        unsafe { &*(slice.as_ptr() as *const T) }
    }
}

impl<T: ResideInShm> DerefMut for zshmmut<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let slice = self.inner.as_mut();
        unsafe { &mut *(slice.as_mut_ptr() as *mut T) }
    }
}

impl DerefMut for zshmmut<[u8]> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut()
    }
}

impl AsRef<[u8]> for zshmmut<[u8]> {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl<T: ResideInShm> AsRef<T> for zshmmut<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl AsMut<[u8]> for zshmmut<[u8]> {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}

impl<T: ResideInShm> AsMut<T> for zshmmut<T> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}
