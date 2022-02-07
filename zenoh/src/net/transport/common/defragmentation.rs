//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::protocol::core::{Reliability, SeqNumBytes, ZInt};
use super::protocol::io::{ZBuf, ZSlice};
use super::protocol::message::ZenohMessage;
use super::seq_num::SeqNum;

use zenoh_util::core::Result as ZResult;

#[derive(Debug)]
pub(crate) struct DefragBuffer {
    reliability: Reliability,
    pub(crate) sn: SeqNum,
    capacity: usize,
    buffer: ZBuf,
}

impl DefragBuffer {
    pub(crate) fn make(
        reliability: Reliability,
        bytes: SeqNumBytes,
        capacity: usize,
    ) -> ZResult<DefragBuffer> {
        let db = DefragBuffer {
            reliability,
            sn: SeqNum::make(0, bytes)?,
            capacity,
            buffer: ZBuf::new(),
        };
        Ok(db)
    }

    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    #[inline(always)]
    pub(crate) fn clear(&mut self) {
        self.buffer.clear()
    }

    #[inline(always)]
    pub(crate) fn sync(&mut self, sn: ZInt) -> ZResult<()> {
        self.sn.set(sn)
    }

    pub(crate) fn push(&mut self, sn: ZInt, zslice: ZSlice) -> ZResult<()> {
        if sn != self.sn.get() {
            self.clear();
            bail!("Expected SN {}, received {}", self.sn.get(), sn)
        }

        let new_len = zslice.len() + self.buffer.len();
        if new_len > self.capacity {
            self.clear();
            bail!(
                "Defragmentation buffer full: {} bytes. Capacity: {}.",
                new_len,
                self.capacity
            )
        }

        self.buffer.add_zslice(zslice);
        self.sn.increment();

        Ok(())
    }

    #[inline(always)]
    pub(crate) fn defragment(&mut self) -> Option<ZenohMessage> {
        let res = self.buffer.read_zenoh_message(self.reliability);
        self.clear();
        res
    }
}
