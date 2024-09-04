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
use zenoh_buffers::{buffer::Buffer, reader::HasReader, ZBuf, ZSlice};
use zenoh_codec::{RCodec, Zenoh080Reliability};
use zenoh_protocol::{
    core::{Bits, Reliability},
    network::NetworkMessage,
    transport::TransportSn,
};
use zenoh_result::{bail, ZResult};

use super::seq_num::SeqNum;

#[derive(Debug)]
pub(crate) struct DefragBuffer {
    reliability: Reliability,
    pub(crate) sn: SeqNum,
    buffer: ZBuf,
    capacity: usize,
    len: usize,
}

impl DefragBuffer {
    pub(crate) fn make(
        reliability: Reliability,
        resolution: Bits,
        capacity: usize,
    ) -> ZResult<DefragBuffer> {
        let db = DefragBuffer {
            reliability,
            sn: SeqNum::make(0, resolution)?,
            buffer: ZBuf::empty(),
            capacity,
            len: 0,
        };
        Ok(db)
    }

    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    #[inline(always)]
    pub(crate) fn clear(&mut self) {
        self.buffer.clear();
        self.len = 0;
    }

    #[inline(always)]
    pub(crate) fn sync(&mut self, sn: TransportSn) -> ZResult<()> {
        self.sn.set(sn)
    }

    pub(crate) fn push(&mut self, sn: TransportSn, zslice: ZSlice) -> ZResult<()> {
        if sn != self.sn.get() {
            self.clear();
            bail!(
                "Defragmentation SN error: expected SN {}, received {}",
                self.sn.get(),
                sn
            )
        }

        let new_len = self.len + zslice.len();
        if new_len > self.capacity {
            self.clear();
            bail!(
                "Defragmentation buffer full: {} bytes. Capacity: {}.",
                new_len,
                self.capacity
            )
        }

        self.sn.increment();
        self.buffer.push_zslice(zslice);
        self.len = new_len;

        Ok(())
    }

    #[inline(always)]
    pub(crate) fn defragment(&mut self) -> Option<NetworkMessage> {
        let mut reader = self.buffer.reader();
        let rcodec = Zenoh080Reliability::new(self.reliability);
        let res: Option<NetworkMessage> = rcodec.read(&mut reader).ok();
        self.clear();
        res
    }
}
