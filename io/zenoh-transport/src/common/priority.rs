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
use std::sync::{Arc, Mutex};

use zenoh_core::zlock;
use zenoh_protocol::{
    core::{Bits, Reliability},
    transport::{PrioritySn, TransportSn},
};
use zenoh_result::ZResult;

use super::{
    defragmentation::DefragBuffer,
    seq_num::{SeqNum, SeqNumGenerator, SeqNumWindow},
};

#[derive(Debug)]
pub(crate) struct TransportChannelTx {
    pub(crate) sn: SeqNumGenerator,
}

impl TransportChannelTx {
    pub(crate) fn make(resolution: Bits) -> ZResult<TransportChannelTx> {
        let tch = TransportChannelTx {
            sn: SeqNumGenerator::make(0, resolution)?,
        };
        Ok(tch)
    }

    pub(crate) fn sync(&mut self, sn: TransportSn) -> ZResult<()> {
        self.sn.set(sn)
    }
}

#[derive(Debug)]
pub(crate) struct TransportChannelRx {
    pub(crate) sn: SeqNum,
    pub(crate) frame_sn: Option<SeqNumWindow>,
    pub(crate) defrag: DefragBuffer,
}

impl TransportChannelRx {
    pub(crate) fn make(
        reliability: Reliability,
        resolution: Bits,
        defrag_buff_size: usize,
        best_effort_reorder_window: usize,
    ) -> ZResult<TransportChannelRx> {
        let sn = SeqNum::make(0, resolution)?;
        let frame_sn = (reliability == Reliability::BestEffort
            && best_effort_reorder_window != 0)
            .then(|| SeqNumWindow::make(0, resolution, best_effort_reorder_window))
            .transpose()?;
        let defrag = DefragBuffer::make(reliability, resolution, defrag_buff_size)?;
        let tch = TransportChannelRx {
            sn,
            frame_sn,
            defrag,
        };
        Ok(tch)
    }

    pub(crate) fn sync(&mut self, sn: TransportSn) -> ZResult<()> {
        // Set the sequence number in the state as it had received a message with sn - 1
        let sn = if sn == 0 {
            self.sn.resolution()
        } else {
            sn - 1
        };

        self.sn.set(sn)?;
        if let Some(frame_sn) = self.frame_sn.as_mut() {
            frame_sn.reset(sn)?;
        }
        self.defrag.sync(sn)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TransportPriorityTx {
    pub(crate) reliable: Arc<Mutex<TransportChannelTx>>,
    pub(crate) best_effort: Arc<Mutex<TransportChannelTx>>,
}

impl TransportPriorityTx {
    pub(crate) fn make(resolution: Bits) -> ZResult<TransportPriorityTx> {
        let rch = TransportChannelTx::make(resolution)?;
        let bch = TransportChannelTx::make(resolution)?;
        let ctx = TransportPriorityTx {
            reliable: Arc::new(Mutex::new(rch)),
            best_effort: Arc::new(Mutex::new(bch)),
        };
        Ok(ctx)
    }

    pub(crate) fn sync(&self, sn: PrioritySn) -> ZResult<()> {
        zlock!(self.reliable).sync(sn.reliable)?;
        zlock!(self.best_effort).sync(sn.best_effort)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TransportPriorityRx {
    pub(crate) reliable: Arc<Mutex<TransportChannelRx>>,
    pub(crate) best_effort: Arc<Mutex<TransportChannelRx>>,
}

impl TransportPriorityRx {
    pub(crate) fn make(
        resolution: Bits,
        defrag_buff_size: usize,
        best_effort_reorder_window: usize,
    ) -> ZResult<TransportPriorityRx> {
        let rch = TransportChannelRx::make(
            Reliability::Reliable,
            resolution,
            defrag_buff_size,
            best_effort_reorder_window,
        )?;
        let bch = TransportChannelRx::make(
            Reliability::BestEffort,
            resolution,
            defrag_buff_size,
            best_effort_reorder_window,
        )?;
        let ctr = TransportPriorityRx {
            reliable: Arc::new(Mutex::new(rch)),
            best_effort: Arc::new(Mutex::new(bch)),
        };
        Ok(ctr)
    }

    pub(crate) fn sync(&self, sn: PrioritySn) -> ZResult<()> {
        zlock!(self.reliable).sync(sn.reliable)?;
        zlock!(self.best_effort).sync(sn.best_effort)
    }
}
