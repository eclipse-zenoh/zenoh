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
use std::marker::PhantomData;

use zenoh_buffers::{ZBuf, ZSlice, ZSliceBuffer};

use crate::{SHMBuf, SHMBufMut};

use super::zsliceshmmut::ZSliceShmMut;

/// An immutable SHM slice
#[zenoh_macros::unstable_doc]
#[derive(Clone, Debug)]
pub struct ZSliceShm<'a, T: SHMBuf<'a>> {
    data: T,
    _phantom: PhantomData<&'a T>,
}

impl<'a, T: SHMBuf<'a>> Deref for ZSliceShm<'a, T> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.data.as_ref()
    }
}

impl<'a, T: SHMBuf<'a>> AsRef<[u8]> for ZSliceShm<'a, T> {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl<'a, T: SHMBuf<'a>> From<T> for ZSliceShm<'a, T> {
    fn from(value: T) -> Self {
        Self {
            data: value,
            _phantom: Default::default(),
        }
    }
}

impl<'a, T: SHMBufMut<'a>> TryFrom<ZSliceShm<'a, T>> for ZSliceShmMut<'a, T> {
    type Error = T;

    fn try_from(value: ZSliceShm<'a, T>) -> Result<Self, Self::Error> {
        Self::try_new(value.data)
    }
}

impl<'a, T: SHMBuf<'a> + ZSliceBuffer> From<ZSliceShm<'a, T>> for ZSlice {
    fn from(value: ZSliceShm<'a, T>) -> Self {
        value.data.into()
    }
}

impl<'a, T: SHMBuf<'a> + ZSliceBuffer> From<ZSliceShm<'a, T>> for ZBuf {
    fn from(value: ZSliceShm<'a, T>) -> Self {
        value.data.into()
    }
}
