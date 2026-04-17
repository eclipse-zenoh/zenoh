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

use std::sync::Arc;

use zenoh_buffers::{ZBuf, ZSlice};
use zenoh_core::zerror;
use zenoh_result::ZResult;

use crate::api::reader::rx_buffer::RxBuffer;

pub enum DefragmentationState {
    Single(ZSlice),
    Fragmented(ZBuf),
}

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

    pub fn defragment(mut self) -> ZResult<DefragmentationState> {
        if self.buffers.len() == 1 {
            // SAFETY: buffers is guaranteed to have exactly one element, so pop will return Some
            let buf = unsafe { self.buffers.pop().unwrap_unchecked() };

            let slice = ZSlice::new(buf, self.data_offset, self.data_offset + self.size)
                .map_err(|_| zerror!("Error constructing slice...."))?;

            return Ok(DefragmentationState::Single(slice));
        }

        let mut result = ZBuf::empty();
        for (i, buf) in self.buffers.iter().cloned().enumerate() {
            let first = i == 0;
            let last = i == self.buffers.len() - 1;

            let start = if first { self.data_offset } else { 0 };
            let end = if last { self.size } else { buf.len() };

            self.size -= end - start;

            let slice = ZSlice::new(buf, start, end)
                .map_err(|_| zerror!("Error constructing slice...."))?;

            result.push_zslice(slice);
        }
        Ok(DefragmentationState::Fragmented(result))
    }
}

impl FragmentedBatch {
    pub fn size(&self) -> usize {
        self.size
    }

    pub fn iter(&self) -> impl Iterator<Item = &u8> {
        self.buffers
            .iter()
            .flat_map(|inner| inner.iter())
            .skip(self.data_offset)
            .take(self.size)
    }
}
