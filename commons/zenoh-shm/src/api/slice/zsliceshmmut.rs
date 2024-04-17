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
use std::borrow::{Borrow, BorrowMut};

use zenoh_buffers::{ZBuf, ZSlice};

use crate::SharedMemoryBuf;

use super::{
    traits::{SHMBuf, SHMBufMut},
    zsliceshm::{zsliceshm, ZSliceShm},
};

/// A mutable SHM slice
#[zenoh_macros::unstable_doc]
#[derive(Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct ZSliceShmMut(SharedMemoryBuf);

impl SHMBuf for ZSliceShmMut {
    fn is_valid(&self) -> bool {
        self.0.is_valid()
    }
}

impl SHMBufMut for ZSliceShmMut {}

impl ZSliceShmMut {
    pub(crate) unsafe fn new_unchecked(data: SharedMemoryBuf) -> Self {
        Self(data)
    }
}

impl PartialEq<zsliceshmmut> for &ZSliceShmMut {
    fn eq(&self, other: &zsliceshmmut) -> bool {
        self.0 == other.0 .0
    }
}

impl TryFrom<SharedMemoryBuf> for ZSliceShmMut {
    type Error = SharedMemoryBuf;

    fn try_from(value: SharedMemoryBuf) -> Result<Self, Self::Error> {
        match value.is_unique() && value.is_valid() {
            true => Ok(Self(value)),
            false => Err(value),
        }
    }
}

impl TryFrom<ZSliceShm> for ZSliceShmMut {
    type Error = ZSliceShm;

    fn try_from(value: ZSliceShm) -> Result<Self, Self::Error> {
        match value.0.is_unique() && value.0.is_valid() {
            true => Ok(Self(value.0)),
            false => Err(value),
        }
    }
}

impl Borrow<zsliceshm> for ZSliceShmMut {
    fn borrow(&self) -> &zsliceshm {
        unsafe { core::mem::transmute(self) }
    }
}

impl BorrowMut<zsliceshm> for ZSliceShmMut {
    fn borrow_mut(&mut self) -> &mut zsliceshm {
        unsafe { core::mem::transmute(self) }
    }
}

impl Borrow<zsliceshmmut> for ZSliceShmMut {
    fn borrow(&self) -> &zsliceshmmut {
        unsafe { core::mem::transmute(self) }
    }
}

impl BorrowMut<zsliceshmmut> for ZSliceShmMut {
    fn borrow_mut(&mut self) -> &mut zsliceshmmut {
        unsafe { core::mem::transmute(self) }
    }
}

impl Deref for ZSliceShmMut {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl DerefMut for ZSliceShmMut {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut()
    }
}

impl AsRef<[u8]> for ZSliceShmMut {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl AsMut<[u8]> for ZSliceShmMut {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}

impl From<ZSliceShmMut> for ZSliceShm {
    fn from(value: ZSliceShmMut) -> Self {
        value.0.into()
    }
}

impl From<ZSliceShmMut> for ZSlice {
    fn from(value: ZSliceShmMut) -> Self {
        value.0.into()
    }
}

impl From<ZSliceShmMut> for ZBuf {
    fn from(value: ZSliceShmMut) -> Self {
        value.0.into()
    }
}

/// A borrowed mutable SHM slice
#[zenoh_macros::unstable_doc]
#[derive(Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct zsliceshmmut(ZSliceShmMut);

impl PartialEq<ZSliceShmMut> for &zsliceshmmut {
    fn eq(&self, other: &ZSliceShmMut) -> bool {
        self.0 .0 == other.0
    }
}

impl Deref for zsliceshmmut {
    type Target = ZSliceShmMut;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for zsliceshmmut {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl TryFrom<&mut SharedMemoryBuf> for &mut zsliceshmmut {
    type Error = ();

    fn try_from(value: &mut SharedMemoryBuf) -> Result<Self, Self::Error> {
        match value.is_unique() && value.is_valid() {
            true => Ok(unsafe { core::mem::transmute(value) }),
            false => Err(()),
        }
    }
}

impl From<&mut zsliceshmmut> for &mut zsliceshm {
    fn from(value: &mut zsliceshmmut) -> Self {
        unsafe { core::mem::transmute(value) }
    }
}
