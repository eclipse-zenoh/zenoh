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

use core::any::Any;

use zenoh_shm::{
    api::provider::zsliceshm::ZSliceShm, zsliceshm_access::consume_zsliceshm, SharedMemoryBuf,
};

use crate::{ZSlice, ZSliceBuffer};

impl From<ZSliceShm> for ZSlice {
    fn from(val: ZSliceShm) -> Self {
        consume_zsliceshm(val).into()
    }
}

impl ZSliceBuffer for SharedMemoryBuf {
    fn as_slice(&self) -> &[u8] {
        self.as_ref()
    }
    unsafe fn as_mut_slice_unchecked(&mut self) -> &mut [u8] {
        self.as_mut()
    }
    fn as_mut_slice(&mut self) -> Option<&mut [u8]> {
        if self.is_valid() && self.is_unique() {
            return Some(self.as_mut());
        }
        None
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn is_valid(&self) -> bool {
        self.is_valid()
    }
}
