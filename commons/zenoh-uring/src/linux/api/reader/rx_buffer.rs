//
// Copyright (c) 2026 ZettaScale Technology
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

use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use zenoh_buffers::ZSliceBuffer;

use crate::reader::reservable_arena::ReservableArenaInner;

#[derive(Debug)]
pub struct RxBuffer {
    data: &'static mut [u8],
    buf_id: u16,
    arena: Arc<ReservableArenaInner>,
}

impl Deref for RxBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl DerefMut for RxBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}

impl RxBuffer {
    pub(crate) fn new(
        data: &'static mut [u8],
        buf_id: u16,
        arena: Arc<ReservableArenaInner>,
    ) -> Self {
        Self {
            data,
            buf_id,
            arena,
        }
    }
}

impl Drop for RxBuffer {
    fn drop(&mut self) {
        self.arena.recycle_batch(self.buf_id);
    }
}

impl ZSliceBuffer for RxBuffer {
    fn as_slice(&self) -> &[u8] {
        &self
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
