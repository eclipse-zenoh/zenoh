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
use super::protocol::core::{ConduitSn, Priority, Reliability, SeqNumBytes, ZInt};
use super::seq_num::{SeqNum, SeqNumGenerator};
use std::sync::{Arc, Mutex};
use zenoh_util::core::zresult::ZResult;
use zenoh_util::zlock;

#[derive(Debug)]
pub(crate) struct TransportChannelTx {
    pub(crate) sn: SeqNumGenerator,
}

impl TransportChannelTx {
    pub(crate) fn make(sn_bytes: SeqNumBytes) -> ZResult<TransportChannelTx> {
        let tch = TransportChannelTx {
            sn: SeqNumGenerator::make(0, sn_bytes)?,
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
        sn_bytes: SeqNumBytes,
        defrag_buff_size: usize,
    ) -> ZResult<TransportChannelRx> {
        let sn = SeqNum::make(0, sn_bytes)?;
        let defrag = DefragBuffer::make(reliability, sn_bytes, defrag_buff_size)?;
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

#[derive(Clone, Debug)]
pub(crate) struct TransportConduitTx {
    pub(crate) priority: Priority,
    pub(crate) reliable: Arc<Mutex<TransportChannelTx>>,
    pub(crate) best_effort: Arc<Mutex<TransportChannelTx>>,
}

impl TransportConduitTx {
    pub(crate) fn make(priority: Priority, sn_bytes: SeqNumBytes) -> ZResult<TransportConduitTx> {
        let rch = TransportChannelTx::make(sn_bytes)?;
        let bch = TransportChannelTx::make(sn_bytes)?;
        let ctx = TransportConduitTx {
            priority,
            reliable: Arc::new(Mutex::new(rch)),
            best_effort: Arc::new(Mutex::new(bch)),
        };
        Ok(ctx)
    }

    pub(crate) fn sync(&self, sn: ConduitSn) -> ZResult<()> {
        let _ = zlock!(self.reliable).sync(sn.reliable)?;
        zlock!(self.best_effort).sync(sn.best_effort)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TransportConduitRx {
    pub(crate) priority: Priority,
    pub(crate) reliable: Arc<Mutex<TransportChannelRx>>,
    pub(crate) best_effort: Arc<Mutex<TransportChannelRx>>,
}

impl TransportConduitRx {
    pub(crate) fn make(
        priority: Priority,
        sn_bytes: SeqNumBytes,
        defrag_buff_size: usize,
    ) -> ZResult<TransportConduitRx> {
        let rch = TransportChannelRx::make(Reliability::Reliable, sn_bytes, defrag_buff_size)?;
        let bch = TransportChannelRx::make(Reliability::BestEffort, sn_bytes, defrag_buff_size)?;
        let ctr = TransportConduitRx {
            priority,
            reliable: Arc::new(Mutex::new(rch)),
            best_effort: Arc::new(Mutex::new(bch)),
        };
        Ok(ctr)
    }

    pub(crate) fn sync(&self, sn: ConduitSn) -> ZResult<()> {
        let _ = zlock!(self.reliable).sync(sn.reliable)?;
        zlock!(self.best_effort).sync(sn.best_effort)
    }
}
