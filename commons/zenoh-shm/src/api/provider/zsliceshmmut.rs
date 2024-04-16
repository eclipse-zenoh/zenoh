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
use std::marker::PhantomData;

use zenoh_buffers::{ZBuf, ZSlice, ZSliceBuffer};

use crate::SHMBufMut;

use super::zsliceshm::ZSliceShm;

/// An immutable SHM slice
#[zenoh_macros::unstable_doc]
#[derive(Clone, Debug)]
pub struct ZSliceShmMut<'a, T: SHMBufMut<'a>> {
    data: T,
    _phantom: PhantomData<&'a T>,
}

impl<'a, T: SHMBufMut<'a>> ZSliceShmMut<'a, T> {
    pub(crate) unsafe fn new_unchecked(data: T) -> Self {
        Self {
            data,
            _phantom: Default::default(),
        }
    }

    pub(crate) fn try_new(data: T) -> Result<Self, T> {
        match data.is_unique() && data.is_valid() {
            true => Ok(Self {
                data,
                _phantom: Default::default(),
            }),
            false => Err(data),
        }
    }
}

//impl<'a, T: SHMBufMut<'a>> TryFrom<T> for ZSliceShmMut<'a, T> {
//    type Error = T;
//
//    fn try_from(value: T) -> Result<Self, Self::Error> {
//        match value.is_unique() && value.is_valid() {
//            true => Ok(Self {
//                data: value,
//                _phantom: Default::default(),
//            }),
//            false => Err(value),
//        }
//    }
//}

impl<'a, T: SHMBufMut<'a>> Deref for ZSliceShmMut<'a, T> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.data.as_ref()
    }
}

impl<'a, T: SHMBufMut<'a>> DerefMut for ZSliceShmMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data.as_mut()
    }
}

impl<'a, T: SHMBufMut<'a>> AsRef<[u8]> for ZSliceShmMut<'a, T> {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl<'a, T: SHMBufMut<'a>> AsMut<[u8]> for ZSliceShmMut<'a, T> {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}

impl<'a, T: SHMBufMut<'a>> From<ZSliceShmMut<'a, T>> for ZSliceShm<'a, T> {
    fn from(value: ZSliceShmMut<'a, T>) -> Self {
        value.data.into()
    }
}

impl<'a, T: SHMBufMut<'a> + ZSliceBuffer> From<ZSliceShmMut<'a, T>> for ZSlice {
    fn from(value: ZSliceShmMut<'a, T>) -> Self {
        value.data.into()
    }
}

impl<'a, T: SHMBufMut<'a> + ZSliceBuffer> From<ZSliceShmMut<'a, T>> for ZBuf {
    fn from(value: ZSliceShmMut<'a, T>) -> Self {
        value.data.into()
    }
}
