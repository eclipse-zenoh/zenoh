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
use super::common::conduit::TransportChannelRx;
use super::protocol::core::{Priority, Reliability, ZInt};
#[cfg(feature = "stats")]
use super::protocol::proto::ZenohBody;
use super::protocol::proto::{
    Close, Frame, FramePayload, Join, TransportBody, TransportMessage, ZenohMessage,
};
use super::transport::{TransportMulticastInner, TransportMulticastPeer};
use crate::net::link::Locator;
use std::sync::MutexGuard;
use zenoh_util::core::Result as ZResult;
use zenoh_util::zerror;

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
impl TransportMulticastInner {
    #[allow(unused_mut)]
    fn trigger_callback(
        &self,
        mut msg: ZenohMessage,
        peer: &TransportMulticastPeer,
    ) -> ZResult<()> {
        #[cfg(feature = "stats")]
        self.stats.inc_rx_z_msgs(1);
        #[cfg(feature = "stats")]
        match &msg.body {
            ZenohBody::Data(data) => match data.reply_context {
                Some(_) => {
                    self.stats.inc_rx_z_data_reply_msgs(1);
                    self.stats
                        .inc_rx_z_data_reply_payload_bytes(data.payload.readable());
                }
                None => {
                    self.stats.inc_rx_z_data_msgs(1);
                    self.stats
                        .inc_rx_z_data_payload_bytes(data.payload.readable());
                }
            },
            ZenohBody::Unit(unit) => match unit.reply_context {
                Some(_) => self.stats.inc_rx_z_unit_reply_msgs(1),
                None => self.stats.inc_rx_z_unit_msgs(1),
            },
            ZenohBody::Pull(_) => self.stats.inc_rx_z_pull_msgs(1),
            ZenohBody::Query(_) => self.stats.inc_rx_z_query_msgs(1),
            ZenohBody::Declare(_) => self.stats.inc_rx_z_declare_msgs(1),
            ZenohBody::LinkStateList(_) => self.stats.inc_rx_z_linkstate_msgs(1),
        }

        #[cfg(feature = "shared-memory")]
        let _ = msg.map_to_shmbuf(self.manager.shmr.clone())?;
        peer.handler.handle_message(msg)
    }

    fn handle_frame(
        &self,
        sn: ZInt,
        payload: FramePayload,
        mut guard: MutexGuard<'_, TransportChannelRx>,
        peer: &TransportMulticastPeer,
    ) -> ZResult<()> {
        let precedes = guard.sn.precedes(sn)?;
        if !precedes {
            log::debug!(
                "Transport: {}. Frame with invalid SN dropped: {}. Expected: {}.",
                self.manager.config.pid,
                sn,
                guard.sn.get()
            );
            // Drop the fragments if needed
            if !guard.defrag.is_empty() {
                guard.defrag.clear();
            }
            // Keep reading
            return Ok(());
        }

        // Set will always return OK because we have already checked
        // with precedes() that the sn has the right resolution
        let _ = guard.sn.set(sn);
        match payload {
            FramePayload::Fragment { buffer, is_final } => {
                if guard.defrag.is_empty() {
                    let _ = guard.defrag.sync(sn);
                }
                guard.defrag.push(sn, buffer)?;
                if is_final {
                    // When shared-memory feature is disabled, msg does not need to be mutable
                    let msg = guard.defrag.defragment().ok_or_else(|| {
                        zerror!(
                            "Transport {}: {}. Defragmentation error.",
                            self.manager.config.pid,
                            self.locator
                        )
                    })?;
                    self.trigger_callback(msg, peer)
                } else {
                    Ok(())
                }
            }
            FramePayload::Messages { mut messages } => {
                for msg in messages.drain(..) {
                    self.trigger_callback(msg, peer)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn handle_join(&self, join: Join, locator: &Locator) -> ZResult<()> {
        if zread!(self.peers).len() >= self.manager.config.multicast.max_sessions {
            log::debug!(
                "Ingoring Join on {} from peer: {}. Max sessions reached: {}.",
                locator,
                join.pid,
                self.manager.config.multicast.max_sessions,
            );
            return Ok(());
        }

        if join.version != self.manager.config.version {
            log::debug!(
                "Ingoring Join on {} from peer: {}. Unsupported version: {}. Expected: {}.",
                locator,
                join.pid,
                join.version,
                self.manager.config.version,
            );
            return Ok(());
        }
        if join.sn_resolution > self.manager.config.sn_resolution {
            log::debug!(
                "Ingoring Join on {} from peer: {}. Unsupported SN resolution: {}. Expected: <= {}.",
                locator,
                join.pid,
                join.sn_resolution,
                self.manager.config.sn_resolution,
            );
            return Ok(());
        }
        if !self.manager.config.multicast.is_qos && join.is_qos() {
            log::debug!(
                "Ingoring Join on {} from peer: {}. QoS is not supported.",
                locator,
                join.pid,
            );
            return Ok(());
        }

        self.new_peer(locator, join)
    }

    pub(super) fn handle_close(&self, close: Close, locator: &Locator) -> ZResult<()> {
        self.del_peer(locator, close.reason)
    }

    pub(super) fn receive_message(&self, msg: TransportMessage, locator: &Locator) -> ZResult<()> {
        // Process the received message
        let guard = zread!(self.peers);
        match guard.get(locator) {
            Some(peer) => {
                peer.active();

                match msg.body {
                    TransportBody::Frame(Frame {
                        channel,
                        sn,
                        payload,
                    }) => {
                        let c = if self.is_qos() {
                            &peer.conduit_rx[channel.priority as usize]
                        } else if channel.priority == Priority::default() {
                            &peer.conduit_rx[0]
                        } else {
                            bail!(
                                "Transport {}: {}. Unknown conduit {:?} from {}.",
                                self.manager.config.pid,
                                self.locator,
                                channel.priority,
                                locator
                            );
                        };

                        let guard = match channel.reliability {
                            Reliability::Reliable => zlock!(c.reliable),
                            Reliability::BestEffort => zlock!(c.best_effort),
                        };
                        self.handle_frame(sn, payload, guard, peer)
                    }
                    TransportBody::Close(close) => {
                        drop(guard);
                        self.handle_close(close, locator)
                    }
                    _ => Ok(()),
                }
            }
            None => {
                drop(guard);
                match msg.body {
                    TransportBody::Join(join) => self.handle_join(join, locator),
                    _ => Ok(()),
                }
            }
        }
    }
}
