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

use core::ops::{Deref, DerefMut};
use std::{
    borrow::{Borrow, BorrowMut},
    num::NonZeroUsize,
};

use zenoh_buffers::{ZBuf, ZSlice};

use super::{
    traits::{BufferRelayoutError, OwnedShmBuf, ShmBuf, ShmBufMut},
    zshm::{zshm, ZShm},
};
use crate::{api::provider::types::MemoryLayout, ShmBufInner};

/// A mutable SHM buffer
#[zenoh_macros::unstable_doc]
#[derive(Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct ZShmMut(pub(crate) ShmBufInner);

impl ShmBuf for ZShmMut {
    fn is_valid(&self) -> bool {
        self.0.is_valid()
    }
}

impl ShmBufMut for ZShmMut {}

impl OwnedShmBuf for ZShmMut {
    fn try_resize(&mut self, new_size: NonZeroUsize) -> Option<()> {
        // Safety: this is safe because ZShmMut is an owned representation of SHM buffer and thus
        // is guaranteed not to be wrapped into ZSlice (see ShmBufInner::try_resize comment)
        unsafe { self.0.try_resize(new_size) }
    }

    fn try_relayout(&mut self, new_layout: MemoryLayout) -> Result<(), BufferRelayoutError> {
        // Safety: this is safe because ZShmMut is an owned representation of SHM buffer and thus
        // is guaranteed not to be wrapped into ZSlice (see ShmBufInner::try_resize comment)
        unsafe { self.0.try_relayout(new_layout) }
    }
}

impl PartialEq<zshmmut> for &ZShmMut {
    fn eq(&self, other: &zshmmut) -> bool {
        self.0 == other.0
    }
}

impl Borrow<zshm> for ZShmMut {
    fn borrow(&self) -> &zshm {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl BorrowMut<zshm> for ZShmMut {
    fn borrow_mut(&mut self) -> &mut zshm {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl Borrow<zshmmut> for ZShmMut {
    fn borrow(&self) -> &zshmmut {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl BorrowMut<zshmmut> for ZShmMut {
    fn borrow_mut(&mut self) -> &mut zshmmut {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl Deref for ZShmMut {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl DerefMut for ZShmMut {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut()
    }
}

impl AsRef<[u8]> for ZShmMut {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl AsMut<[u8]> for ZShmMut {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}

impl From<ZShmMut> for ZShm {
    fn from(value: ZShmMut) -> Self {
        value.0.into()
    }
}

impl From<ZShmMut> for ZSlice {
    fn from(value: ZShmMut) -> Self {
        value.0.into()
    }
}

impl From<ZShmMut> for ZBuf {
    fn from(value: ZShmMut) -> Self {
        value.0.into()
    }
}

/// A borrowed mutable SHM buffer
#[zenoh_macros::unstable_doc]
#[derive(Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct zshmmut(ShmBufInner);

impl PartialEq<ZShmMut> for &zshmmut {
    fn eq(&self, other: &ZShmMut) -> bool {
        self.0 == other.0
    }
}

impl ShmBuf for zshmmut {
    fn is_valid(&self) -> bool {
        self.0.is_valid()
    }
}

impl ShmBufMut for zshmmut {}

impl Deref for zshmmut {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl DerefMut for zshmmut {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut()
    }
}

impl AsRef<[u8]> for zshmmut {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl AsMut<[u8]> for zshmmut {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}
