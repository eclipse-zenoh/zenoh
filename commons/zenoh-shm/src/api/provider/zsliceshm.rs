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

use crate::SharedMemoryBuf;

/*************************************/
/*             ZSLICESHM             */
/*************************************/
pub struct ZSliceShm {
    pub(crate) slice: SharedMemoryBuf,
}

impl ZSliceShm {
    pub(crate) fn new(slice: SharedMemoryBuf) -> Self {
        Self { slice }
    }
}

impl Deref for ZSliceShm {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.slice.as_ref()
    }
}

impl DerefMut for ZSliceShm {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.slice.as_mut()
    }
}

impl AsRef<[u8]> for ZSliceShm {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl AsMut<[u8]> for ZSliceShm {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}
