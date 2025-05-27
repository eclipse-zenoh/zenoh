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
    num::NonZeroUsize,
};

use zenoh_buffers::{ZBuf, ZSlice};

use super::{
    traits::{BufferRelayoutError, OwnedShmBuf, ShmBuf},
    zshmmut::{zshmmut, ZShmMut},
};
use crate::{api::provider::types::MemoryLayout, ShmBufInner};

/// An immutable SHM buffer
#[zenoh_macros::unstable_doc]
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZShm(pub(crate) ShmBufInner);

impl ShmBuf for ZShm {
    fn is_valid(&self) -> bool {
        self.0.is_valid()
    }
}

impl OwnedShmBuf for ZShm {
    fn try_resize(&mut self, new_size: NonZeroUsize) -> Option<()> {
        unsafe { self.0.try_resize(new_size) }
    }

    fn try_relayout(&mut self, new_layout: MemoryLayout) -> Result<(), BufferRelayoutError> {
        unsafe { self.0.try_relayout(new_layout) }
    }
}

impl PartialEq<&zshm> for ZShm {
    fn eq(&self, other: &&zshm) -> bool {
        self.0 == other.0
    }
}

impl Borrow<zshm> for ZShm {
    fn borrow(&self) -> &zshm {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl BorrowMut<zshm> for ZShm {
    fn borrow_mut(&mut self) -> &mut zshm {
        // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
        // to ShmBufInner type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl Deref for ZShm {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl AsRef<[u8]> for ZShm {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl From<ZShm> for ZSlice {
    fn from(value: ZShm) -> Self {
        value.0.into()
    }
}

impl From<ZShm> for ZBuf {
    fn from(value: ZShm) -> Self {
        value.0.into()
    }
}

impl TryFrom<ZShm> for ZShmMut {
    type Error = ZShm;

    fn try_from(value: ZShm) -> Result<Self, Self::Error> {
        match value.0.is_unique() && value.0.is_valid() {
            true => Ok(Self(value.0)),
            false => Err(value),
        }
    }
}

impl TryFrom<&mut ZShm> for &mut zshmmut {
    type Error = ();

    fn try_from(value: &mut ZShm) -> Result<Self, Self::Error> {
        match value.0.is_unique() && value.0.is_valid() {
            true => {
                // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
                // to ShmBufInner type, so it is safe to transmute them in any direction
                Ok(unsafe { core::mem::transmute::<&mut ZShm, &mut zshmmut>(value) })
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
pub struct zshm(ShmBufInner);

impl ShmBuf for zshm {
    fn is_valid(&self) -> bool {
        self.0.is_valid()
    }
}

impl Deref for zshm {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl AsRef<[u8]> for zshm {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl ToOwned for zshm {
    type Owned = ZShm;

    fn to_owned(&self) -> Self::Owned {
        self.0.clone().into()
    }
}

impl PartialEq<ZShm> for &zshm {
    fn eq(&self, other: &ZShm) -> bool {
        self.0 == other.0
    }
}

impl TryFrom<&mut zshm> for &mut zshmmut {
    type Error = ();

    fn try_from(value: &mut zshm) -> Result<Self, Self::Error> {
        match value.0.is_unique() && value.0.is_valid() {
            true => {
                // SAFETY: ZShm, ZShmMut, zshm and zshmmut are #[repr(transparent)]
                // to ShmBufInner type, so it is safe to transmute them in any direction
                Ok(unsafe { core::mem::transmute::<&mut zshm, &mut zshmmut>(value) })
            }
            false => Err(()),
        }
    }
}
