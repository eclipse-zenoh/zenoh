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

use zenoh_buffers::{ZBuf, ZSlice};

use crate::SharedMemoryBuf;

use super::zsliceshm::ZSliceShm;

/// A mutable SHM slice
#[zenoh_macros::unstable_doc]
pub struct ZSliceShmMut {
    slice: SharedMemoryBuf,
}

impl ZSliceShmMut {
    pub(crate) unsafe fn new_unchecked(slice: SharedMemoryBuf) -> Self {
        Self { slice }
    }

    //pub(crate) fn try_new(slice: SharedMemoryBuf) -> Option<Self> {
    //    match slice.is_unique() && slice.is_valid() {
    //        true => Some(Self { slice }),
    //        false => None,
    //    }
    //}
}

impl Deref for ZSliceShmMut {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.slice.as_ref()
    }
}

impl DerefMut for ZSliceShmMut {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.slice.as_mut()
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

impl From<ZSliceShmMut> for ZBuf {
    fn from(val: ZSliceShmMut) -> Self {
        val.slice.into()
    }
}

impl From<ZSliceShmMut> for ZSlice {
    fn from(val: ZSliceShmMut) -> Self {
        val.slice.into()
    }
}

impl From<ZSliceShmMut> for ZSliceShm {
    fn from(val: ZSliceShmMut) -> Self {
        ZSliceShm::new(val.slice)
    }
}
