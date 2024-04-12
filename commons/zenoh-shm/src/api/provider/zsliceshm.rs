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

use zenoh_buffers::{ZBuf, ZSlice};

use crate::SharedMemoryBuf;

/// An immutable SHM slice
#[zenoh_macros::unstable_doc]
#[derive(Clone, Debug)]
pub struct ZSliceShm {
    slice: SharedMemoryBuf,
}

impl ZSliceShm {
    pub(crate) fn new(slice: SharedMemoryBuf) -> Self {
        Self { slice }
    }
}

impl ZSliceShm {
    pub fn as_slice(&self) -> &[u8] {
        self.slice.as_slice()
    }
}

impl Deref for ZSliceShm {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.slice.as_ref()
    }
}

impl AsRef<[u8]> for ZSliceShm {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl From<ZSliceShm> for ZBuf {
    fn from(val: ZSliceShm) -> Self {
        val.slice.into()
    }
}

impl From<ZSliceShm> for ZSlice {
    fn from(val: ZSliceShm) -> Self {
        val.slice.into()
    }
}
