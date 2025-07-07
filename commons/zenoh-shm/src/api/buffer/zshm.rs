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

use core::ops::Deref;
use std::{
    borrow::{Borrow, BorrowMut},
    marker::PhantomData,
    num::NonZeroUsize,
};

use zenoh_buffers::{ZBuf, ZSlice};

use super::{
    traits::{BufferRelayoutError, OwnedShmBuf, ShmBuf},
    zshmmut::{zshmmut, ZShmMut},
};
use crate::{
    api::{buffer::traits::{ResideInShm, ShmBufUnsafeMut}, provider::types::MemoryLayout},
    ShmBufInner,
};

/// An immutable SHM buffer
#[zenoh_macros::unstable_doc]
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZShm<T: ?Sized> {
    pub(crate) inner: ShmBufInner,
    pub(crate) _phantom: PhantomData<T>,
}

impl<T: ResideInShm> ShmBuf<T> for ZShm<T> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl ShmBuf<[u8]> for ZShm<[u8]> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl<T: ResideInShm> ShmBufUnsafeMut<T> for ZShm<T> {
    unsafe fn as_mut_unchecked(&mut self) -> &mut T {
        let slice = self.inner.as_mut_slice_inner();
        unsafe { &mut *(slice.as_mut_ptr() as *mut T) }
    }
}

impl ShmBufUnsafeMut<[u8]> for ZShm<[u8]> {
    unsafe fn as_mut_unchecked(&mut self) -> &mut [u8] {
        self.inner.as_mut_slice_inner()
    }
}

impl<T: ResideInShm> OwnedShmBuf<T> for ZShm<T> {
    fn try_resize(&mut self, new_size: NonZeroUsize) -> Option<()> {
        // Safety: this is safe because ZShm is an owned representation of SHM buffer and thus
        // is guaranteed not to be wrapped into ZSlice (see ShmBufInner::try_resize comment)
        unsafe { self.inner.try_resize(new_size) }
    }

    fn try_relayout(&mut self, new_layout: MemoryLayout) -> Result<(), BufferRelayoutError> {
        // Safety: this is safe because ZShm is an owned representation of SHM buffer and thus
        // is guaranteed not to be wrapped into ZSlice (see ShmBufInner::try_relayout comment)
        unsafe { self.inner.try_relayout(new_layout) }
    }
}

impl OwnedShmBuf<[u8]> for ZShm<[u8]> {
    fn try_resize(&mut self, new_size: NonZeroUsize) -> Option<()> {
        // Safety: this is safe because ZShm is an owned representation of SHM buffer and thus
        // is guaranteed not to be wrapped into ZSlice (see ShmBufInner::try_resize comment)
        unsafe { self.inner.try_resize(new_size) }
    }

    fn try_relayout(&mut self, new_layout: MemoryLayout) -> Result<(), BufferRelayoutError> {
        // Safety: this is safe because ZShm is an owned representation of SHM buffer and thus
        // is guaranteed not to be wrapped into ZSlice (see ShmBufInner::try_relayout comment)
        unsafe { self.inner.try_relayout(new_layout) }
    }
}

impl<T: ?Sized> PartialEq<&zshm<T>> for ZShm<T> {
    fn eq(&self, other: &&zshm<T>) -> bool {
        self.inner == other.inner
    }
}

impl<T: ?Sized> Borrow<zshm<T>> for ZShm<T> {
    fn borrow(&self) -> &zshm<T> {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl<T: ?Sized> BorrowMut<zshm<T>> for ZShm<T> {
    fn borrow_mut(&mut self) -> &mut zshm<T> {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl<T: ResideInShm> Deref for ZShm<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let slice = self.inner.as_ref();
        unsafe { &*(slice.as_ptr() as *const T) }
    }
}

impl Deref for ZShm<[u8]> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

impl<T: ResideInShm> AsRef<T> for ZShm<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl AsRef<[u8]> for ZShm<[u8]> {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl<T: ?Sized> From<ZShm<T>> for ZSlice {
    fn from(value: ZShm<T>) -> Self {
        value.inner.into()
    }
}

impl<T: ?Sized> From<ZShm<T>> for ZBuf {
    fn from(value: ZShm<T>) -> Self {
        value.inner.into()
    }
}

impl<T: ?Sized> TryFrom<ZShm<T>> for ZShmMut<T> {
    type Error = ZShm<T>;

    fn try_from(value: ZShm<T>) -> Result<Self, Self::Error> {
        match value.inner.is_unique() && value.inner.is_valid() {
            true => Ok(unsafe { std::mem::transmute(value) }),
            false => Err(value),
        }
    }
}

impl<T: ?Sized> TryFrom<&mut ZShm<T>> for &mut zshmmut<T> {
    type Error = ();

    fn try_from(value: &mut ZShm<T>) -> Result<Self, Self::Error> {
        match value.inner.is_unique() && value.inner.is_valid() {
            true => {
                // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
                // to ShmBufInner type, so it is safe to transmute them in any direction
                Ok(unsafe { core::mem::transmute::<&mut ZShm<T>, &mut zshmmut<T>>(value) })
            }
            false => Err(()),
        }
    }
}

/// A borrowed immutable SHM buffer
#[zenoh_macros::unstable_doc]
#[derive(Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct zshm<T: ?Sized> {
    pub(crate) inner: ShmBufInner,
    _phantom: PhantomData<T>,
}

impl<T: ResideInShm> ShmBuf<T> for &zshm<T> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl ShmBuf<[u8]> for &zshm<[u8]> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl<T: ResideInShm> ShmBuf<T> for &mut zshm<T> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

impl ShmBuf<[u8]> for &mut zshm<[u8]> {
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}


impl<T: ResideInShm> ShmBufUnsafeMut<T> for &mut zshm<T> {
    unsafe fn as_mut_unchecked(&mut self) -> &mut T {
        let slice = self.inner.as_mut_slice_inner();
        unsafe { &mut *(slice.as_mut_ptr() as *mut T) }
    }
}

impl ShmBufUnsafeMut<[u8]> for &mut zshm<[u8]> {
    unsafe fn as_mut_unchecked(&mut self) -> &mut [u8] {
        self.inner.as_mut_slice_inner()
    }
}

impl<T: ResideInShm> Deref for zshm<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let slice = self.inner.as_ref();
        unsafe { &*(slice.as_ptr() as *const T) }
    }
}

impl Deref for zshm<[u8]> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}


impl<T: ResideInShm> AsRef<T> for zshm<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl AsRef<[u8]> for zshm<[u8]> {
    fn as_ref(&self) -> &[u8] {
        self
    }
}


impl<T: ?Sized> ToOwned for zshm<T> {
    type Owned = ZShm<T>;

    fn to_owned(&self) -> Self::Owned {
        // SAFETY: this is safe because Self already guarantees inner to be T
        unsafe { ZShm::new_unchecked(self.inner.clone()) }
    }
}

impl<T: ?Sized> PartialEq<ZShm<T>> for &zshm<T> {
    fn eq(&self, other: &ZShm<T>) -> bool {
        self.inner == other.inner
    }
}

impl<T: ?Sized> TryFrom<&mut zshm<T>> for &mut zshmmut<T> {
    type Error = ();

    fn try_from(value: &mut zshm<T>) -> Result<Self, Self::Error> {
        match value.inner.is_unique() && value.inner.is_valid() {
            true => {
                // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
                // to ShmBufInner type, so it is safe to transmute them in any direction
                Ok(unsafe { core::mem::transmute::<&mut zshm<T>, &mut zshmmut<T>>(value) })
            }
            false => Err(()),
        }
    }
}
