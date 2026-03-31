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

use std::{ops::Deref, sync::Arc};

use crate::api::reader::rx_buffer::RxBuffer;

#[derive(Debug)]
pub struct FragmentedBatch {
    pub(crate) size: usize,
    pub data_offset: usize,
    pub(crate) buffers: Vec<Arc<RxBuffer>>,
}

impl FragmentedBatch {
    pub(crate) fn new(size: usize, data_offset: usize, buffers: Vec<Arc<RxBuffer>>) -> Self {
        Self {
            size,
            data_offset,
            buffers,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &u8> {
        self.buffers
            .iter()
            .flat_map(|inner| inner.iter())
            .skip(self.data_offset)
            .take(self.size)
    }

    pub fn try_contagious_zerocopy(&self) -> Option<Arc<RxBuffer>> {
        if self.buffers.len() == 1 {
            return Some(self.buffers[0].clone());
        }
        None
    }

    // TODO: change to zerocopy approach with multi-slice support for read codec
    pub fn contagious_copy(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.size);

        let mut leftover = self.size;

        for (i, buf) in self.buffers.iter().enumerate() {
            let first = i == 0;
            let last = i == self.buffers.len() - 1;

            let mut slice: &[u8] = buf.deref(); // assuming RxBuffer exposes this

            if first {
                slice = &slice[self.data_offset..];
            }

            if last {
                slice = &slice[..leftover];
            }

            result.extend_from_slice(slice);

            leftover -= slice.len();
        }

        result
    }

    pub fn size(&self) -> usize {
        self.size
    }
}
