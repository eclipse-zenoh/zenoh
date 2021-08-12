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
use super::core::{Priority, Reliability, ZInt};
use super::defragmentation::DefragBuffer;
use super::seq_num::*;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub(crate) struct SessionTransportChannelTx {
    pub(crate) sn: SeqNumGenerator,
}

impl SessionTransportChannelTx {
    pub(crate) fn new(initial_sn: ZInt, sn_resolution: ZInt) -> SessionTransportChannelTx {
        SessionTransportChannelTx {
            sn: SeqNumGenerator::new(initial_sn, sn_resolution),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SessionTransportChannelRx {
    pub(crate) sn: SeqNum,
    pub(crate) defrag: DefragBuffer,
}

impl SessionTransportChannelRx {
    pub(crate) fn new(
        reliability: Reliability,
        initial_sn: ZInt,
        sn_resolution: ZInt,
    ) -> SessionTransportChannelRx {
        // Set the sequence number in the state as it had received a message with initial_sn - 1
        let last_initial_sn = if initial_sn == 0 {
            sn_resolution - 1
        } else {
            initial_sn - 1
        };

        SessionTransportChannelRx {
            sn: SeqNum::new(last_initial_sn, sn_resolution),
            defrag: DefragBuffer::new(reliability, initial_sn, sn_resolution),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SessionTransportConduitTx {
    pub(crate) id: Priority,
    pub(crate) reliable: Arc<Mutex<SessionTransportChannelTx>>,
    pub(crate) best_effort: Arc<Mutex<SessionTransportChannelTx>>,
}

impl SessionTransportConduitTx {
    pub(crate) fn new(
        priority: Priority,
        initial_sn: ZInt,
        sn_resolution: ZInt,
    ) -> SessionTransportConduitTx {
        SessionTransportConduitTx {
            id: priority,
            reliable: Arc::new(Mutex::new(SessionTransportChannelTx::new(
                initial_sn,
                sn_resolution,
            ))),
            best_effort: Arc::new(Mutex::new(SessionTransportChannelTx::new(
                initial_sn,
                sn_resolution,
            ))),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SessionTransportConduitRx {
    pub(crate) priority: Priority,
    pub(crate) reliable: Arc<Mutex<SessionTransportChannelRx>>,
    pub(crate) best_effort: Arc<Mutex<SessionTransportChannelRx>>,
}

impl SessionTransportConduitRx {
    pub(crate) fn new(
        priority: Priority,
        initial_sn: ZInt,
        sn_resolution: ZInt,
    ) -> SessionTransportConduitRx {
        SessionTransportConduitRx {
            priority,
            reliable: Arc::new(Mutex::new(SessionTransportChannelRx::new(
                Reliability::Reliable,
                initial_sn,
                sn_resolution,
            ))),
            best_effort: Arc::new(Mutex::new(SessionTransportChannelRx::new(
                Reliability::BestEffort,
                initial_sn,
                sn_resolution,
            ))),
        }
    }
}
