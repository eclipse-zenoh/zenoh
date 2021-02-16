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
use super::core::{Reliability, ZInt};
use super::io::RBuf;
use super::proto::ZenohMessage;
use super::SeqNum;

use zenoh_util::core::{ZError, ZErrorKind, ZResult};

pub(super) struct DefragBuffer {
    // Keep track of the next expected fragment
    sn: SeqNum,
    buffer: RBuf,
    reliability: Reliability,
}

impl DefragBuffer {
    pub(super) fn new(
        initial_sn: ZInt,
        sn_resolution: ZInt,
        reliability: Reliability,
    ) -> DefragBuffer {
        DefragBuffer {
            sn: SeqNum::new(initial_sn, sn_resolution),
            buffer: RBuf::new(),
            reliability,
        }
    }

    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    #[inline]
    pub(super) fn clear(&mut self) {
        self.buffer.clear()
    }

    #[inline]
    pub(super) fn sync(&mut self, sn: ZInt) -> ZResult<()> {
        self.sn.set(sn)
    }

    pub(super) fn push(&mut self, sn: ZInt, mut buffer: RBuf) -> ZResult<()> {
        if sn != self.sn.get() {
            self.clear();
            return zerror!(ZErrorKind::InvalidMessage {
                descr: format!("Expected SN {}, received {}", self.sn.get(), sn)
            });
        }

        buffer.drain_into_rbuf(&mut self.buffer);
        self.sn.increment();

        Ok(())
    }

    #[inline]
    pub(super) fn defragment(&mut self) -> Option<ZenohMessage> {
        let res = self.buffer.read_zenoh_message(self.reliability);
        self.clear();
        res
    }
}
