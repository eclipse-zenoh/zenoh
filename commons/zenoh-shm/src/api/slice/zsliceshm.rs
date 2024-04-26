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
    ops::DerefMut,
};

use zenoh_buffers::{ZBuf, ZSlice};

use crate::SharedMemoryBuf;

use super::{traits::SHMBuf, zsliceshmmut::zsliceshmmut};

/// An immutable SHM slice
#[zenoh_macros::unstable_doc]
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZSliceShm(pub(crate) SharedMemoryBuf);

impl SHMBuf for ZSliceShm {
    fn is_valid(&self) -> bool {
        self.0.is_valid()
    }
}

impl PartialEq<&zsliceshm> for ZSliceShm {
    fn eq(&self, other: &&zsliceshm) -> bool {
        self.0 == other.0 .0
    }
}

impl Borrow<zsliceshm> for ZSliceShm {
    fn borrow(&self) -> &zsliceshm {
        // SAFETY: ZSliceShm, ZSliceShmMut, zsliceshm and zsliceshmmut are #[repr(transparent)]
        // to SharedMemoryBuf type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl BorrowMut<zsliceshm> for ZSliceShm {
    fn borrow_mut(&mut self) -> &mut zsliceshm {
        // SAFETY: ZSliceShm, ZSliceShmMut, zsliceshm and zsliceshmmut are #[repr(transparent)]
        // to SharedMemoryBuf type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(self) }
    }
}

impl Deref for ZSliceShm {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl AsRef<[u8]> for ZSliceShm {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl From<SharedMemoryBuf> for ZSliceShm {
    fn from(value: SharedMemoryBuf) -> Self {
        Self(value)
    }
}

impl From<ZSliceShm> for ZSlice {
    fn from(value: ZSliceShm) -> Self {
        value.0.into()
    }
}

impl From<ZSliceShm> for ZBuf {
    fn from(value: ZSliceShm) -> Self {
        value.0.into()
    }
}

impl TryFrom<&mut ZSliceShm> for &mut zsliceshmmut {
    type Error = ();

    fn try_from(value: &mut ZSliceShm) -> Result<Self, Self::Error> {
        match value.0.is_unique() && value.0.is_valid() {
            true => {
                // SAFETY: ZSliceShm, ZSliceShmMut, zsliceshm and zsliceshmmut are #[repr(transparent)]
                // to SharedMemoryBuf type, so it is safe to transmute them in any direction
                Ok(unsafe { core::mem::transmute(value) })
            }
            false => Err(()),
        }
    }
}

/// A borrowed immutable SHM slice
#[zenoh_macros::unstable_doc]
#[derive(Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct zsliceshm(ZSliceShm);

impl ToOwned for zsliceshm {
    type Owned = ZSliceShm;

    fn to_owned(&self) -> Self::Owned {
        self.0.clone()
    }
}

impl PartialEq<ZSliceShm> for &zsliceshm {
    fn eq(&self, other: &ZSliceShm) -> bool {
        self.0 .0 == other.0
    }
}

impl Deref for zsliceshm {
    type Target = ZSliceShm;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for zsliceshm {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<&SharedMemoryBuf> for &zsliceshm {
    fn from(value: &SharedMemoryBuf) -> Self {
        // SAFETY: ZSliceShm, ZSliceShmMut, zsliceshm and zsliceshmmut are #[repr(transparent)]
        // to SharedMemoryBuf type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(value) }
    }
}

impl From<&mut SharedMemoryBuf> for &mut zsliceshm {
    fn from(value: &mut SharedMemoryBuf) -> Self {
        // SAFETY: ZSliceShm, ZSliceShmMut, zsliceshm and zsliceshmmut are #[repr(transparent)]
        // to SharedMemoryBuf type, so it is safe to transmute them in any direction
        unsafe { core::mem::transmute(value) }
    }
}

impl TryFrom<&mut zsliceshm> for &mut zsliceshmmut {
    type Error = ();

    fn try_from(value: &mut zsliceshm) -> Result<Self, Self::Error> {
        match value.0 .0.is_unique() && value.0 .0.is_valid() {
            true => {
                // SAFETY: ZSliceShm, ZSliceShmMut, zsliceshm and zsliceshmmut are #[repr(transparent)]
                // to SharedMemoryBuf type, so it is safe to transmute them in any direction
                Ok(unsafe { core::mem::transmute(value) })
            }
            false => Err(()),
        }
    }
}
