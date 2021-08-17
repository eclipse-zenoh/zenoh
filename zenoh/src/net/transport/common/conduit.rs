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
use super::protocol::core::{Priority, Reliability, ZInt};
use super::seq_num::*;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub(crate) struct TransportChannelTx {
    pub(crate) sn: SeqNumGenerator,
}

impl TransportChannelTx {
    pub(crate) fn new(initial_sn: ZInt, sn_resolution: ZInt) -> TransportChannelTx {
        TransportChannelTx {
            sn: SeqNumGenerator::new(initial_sn, sn_resolution),
        }
    }
}

#[derive(Debug)]
pub(crate) struct TransportChannelRx {
    pub(crate) sn: SeqNum,
    pub(crate) defrag: DefragBuffer,
}

impl TransportChannelRx {
    pub(crate) fn new(
        reliability: Reliability,
        initial_sn: ZInt,
        sn_resolution: ZInt,
    ) -> TransportChannelRx {
        // Set the sequence number in the state as it had received a message with initial_sn - 1
        let last_initial_sn = if initial_sn == 0 {
            sn_resolution - 1
        } else {
            initial_sn - 1
        };

        TransportChannelRx {
            sn: SeqNum::new(last_initial_sn, sn_resolution),
            defrag: DefragBuffer::new(reliability, initial_sn, sn_resolution),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TransportConduitTx {
    pub(crate) id: Priority,
    pub(crate) reliable: Arc<Mutex<TransportChannelTx>>,
    pub(crate) best_effort: Arc<Mutex<TransportChannelTx>>,
}

impl TransportConduitTx {
    pub(crate) fn new(
        priority: Priority,
        initial_sn: ZInt,
        sn_resolution: ZInt,
    ) -> TransportConduitTx {
        TransportConduitTx {
            id: priority,
            reliable: Arc::new(Mutex::new(TransportChannelTx::new(
                initial_sn,
                sn_resolution,
            ))),
            best_effort: Arc::new(Mutex::new(TransportChannelTx::new(
                initial_sn,
                sn_resolution,
            ))),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TransportConduitRx {
    pub(crate) priority: Priority,
    pub(crate) reliable: Arc<Mutex<TransportChannelRx>>,
    pub(crate) best_effort: Arc<Mutex<TransportChannelRx>>,
}

impl TransportConduitRx {
    pub(crate) fn new(
        priority: Priority,
        initial_sn: ZInt,
        sn_resolution: ZInt,
    ) -> TransportConduitRx {
        TransportConduitRx {
            priority,
            reliable: Arc::new(Mutex::new(TransportChannelRx::new(
                Reliability::Reliable,
                initial_sn,
                sn_resolution,
            ))),
            best_effort: Arc::new(Mutex::new(TransportChannelRx::new(
                Reliability::BestEffort,
                initial_sn,
                sn_resolution,
            ))),
        }
    }
}
