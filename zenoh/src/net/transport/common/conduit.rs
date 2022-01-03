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
use super::defragmentation::DefragBuffer;
use super::protocol::core::{ConduitSn, Priority, Reliability, ZInt};
use super::seq_num::{SeqNum, SeqNumGenerator};
use std::sync::Mutex;
use zenoh_util::core::zresult::ZResult;
use zenoh_util::zlock;

#[derive(Debug)]
pub(crate) struct TransportChannelTx {
    pub(crate) sn: SeqNumGenerator,
}

impl TransportChannelTx {
    pub(crate) fn make(sn_resolution: ZInt) -> ZResult<TransportChannelTx> {
        let tch = TransportChannelTx {
            sn: SeqNumGenerator::make(0, sn_resolution)?,
        };
        Ok(tch)
    }

    pub(crate) fn sync(&mut self, sn: ZInt) -> ZResult<()> {
        self.sn.set(sn)
    }
}

#[derive(Debug)]
pub(crate) struct TransportChannelRx {
    pub(crate) sn: SeqNum,
    pub(crate) defrag: DefragBuffer,
}

impl TransportChannelRx {
    pub(crate) fn make(
        reliability: Reliability,
        sn_resolution: ZInt,
        defrag_buff_size: usize,
    ) -> ZResult<TransportChannelRx> {
        let sn = SeqNum::make(0, sn_resolution)?;
        let defrag = DefragBuffer::make(reliability, sn_resolution, defrag_buff_size)?;
        let tch = TransportChannelRx { sn, defrag };
        Ok(tch)
    }

    pub(crate) fn sync(&mut self, sn: ZInt) -> ZResult<()> {
        // Set the sequence number in the state as it had received a message with sn - 1
        let sn = if sn == 0 {
            self.sn.resolution() - 1
        } else {
            sn - 1
        };

        let _ = self.sn.set(sn)?;
        self.defrag.sync(sn)
    }
}

#[derive(Debug)]
pub(crate) struct TransportConduitTx {
    pub(crate) priority: Priority,
    pub(crate) reliable: Mutex<TransportChannelTx>,
    pub(crate) best_effort: Mutex<TransportChannelTx>,
}

impl TransportConduitTx {
    pub(crate) fn make(priority: Priority, sn_resolution: ZInt) -> ZResult<TransportConduitTx> {
        let rch = TransportChannelTx::make(sn_resolution)?;
        let bch = TransportChannelTx::make(sn_resolution)?;
        let ctx = TransportConduitTx {
            priority,
            reliable: Mutex::new(rch),
            best_effort: Mutex::new(bch),
        };
        Ok(ctx)
    }

    pub(crate) fn sync(&self, sn: ConduitSn) -> ZResult<()> {
        let _ = zlock!(self.reliable).sync(sn.reliable)?;
        zlock!(self.best_effort).sync(sn.best_effort)
    }
}

#[derive(Debug)]
pub(crate) struct TransportConduitRx {
    pub(crate) priority: Priority,
    pub(crate) reliable: Mutex<TransportChannelRx>,
    pub(crate) best_effort: Mutex<TransportChannelRx>,
}

impl TransportConduitRx {
    pub(crate) fn make(
        priority: Priority,
        sn_resolution: ZInt,
        defrag_buff_size: usize,
    ) -> ZResult<TransportConduitRx> {
        let rch = TransportChannelRx::make(Reliability::Reliable, sn_resolution, defrag_buff_size)?;
        let bch =
            TransportChannelRx::make(Reliability::BestEffort, sn_resolution, defrag_buff_size)?;
        let ctr = TransportConduitRx {
            priority,
            reliable: Mutex::new(rch),
            best_effort: Mutex::new(bch),
        };
        Ok(ctr)
    }

    pub(crate) fn sync(&self, sn: ConduitSn) -> ZResult<()> {
        let _ = zlock!(self.reliable).sync(sn.reliable)?;
        zlock!(self.best_effort).sync(sn.best_effort)
    }
}
