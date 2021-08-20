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
use super::common::conduit::{TransportChannelRx, TransportConduitRx};
use super::protocol::core::{ConduitSnList, PeerId, Priority, Reliability, ZInt};
use super::protocol::proto::{
    Close, Frame, FramePayload, Join, KeepAlive, TransportBody, TransportMessage, ZenohMessage,
};
use super::transport::{TransportMulticastInner, TransportMulticastPeer};
use crate::net::link::Locator;
use std::convert::TryInto;
use std::sync::MutexGuard;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror2;

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
impl TransportMulticastInner {
    #[allow(unused_mut)]
    fn trigger_callback(&self, mut msg: ZenohMessage, peer: &PeerId) -> ZResult<()> {
        let callback = zread!(self.callback).clone();
        match callback.as_ref() {
            Some(callback) => {
                #[cfg(feature = "zero-copy")]
                let _ = msg.map_to_shmbuf(self.manager.shmr.clone())?;
                callback.handle_message(msg, peer)
            }
            None => {
                log::debug!(
                    "Transport {}: {}. No callback available, dropping message: {}",
                    self.manager.config.pid,
                    self.locator,
                    msg
                );
                Ok(())
            }
        }
    }

    fn handle_frame(
        &self,
        sn: ZInt,
        payload: FramePayload,
        mut guard: MutexGuard<'_, TransportChannelRx>,
        peer: &PeerId,
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
                    // When zero-copy feature is disabled, msg does not need to be mutable
                    let msg = guard.defrag.defragment().ok_or_else(|| {
                        let e = format!(
                            "Transport {}: {}. Defragmentation error.",
                            self.manager.config.pid, self.locator
                        );
                        zerror2!(ZErrorKind::InvalidMessage { descr: e })
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
        if join.version != self.manager.config.version {
            return Ok(());
        }
        if join.sn_resolution > self.manager.config.sn_resolution {
            return Ok(());
        }

        let mut guard = zwrite!(self.peers);
        if !guard.contains_key(locator) {
            let conduit_rx = match join.initial_sns {
                ConduitSnList::Plain(sn) => {
                    vec![TransportConduitRx::new(
                        Priority::default(),
                        join.sn_resolution,
                        sn,
                    )]
                }
                ConduitSnList::QoS(ref sns) => sns
                    .iter()
                    .enumerate()
                    .map(|(prio, sn)| {
                        TransportConduitRx::new(
                            (prio as u8).try_into().unwrap(),
                            join.sn_resolution,
                            *sn,
                        )
                    })
                    .collect(),
            }
            .into_boxed_slice();

            let peer = TransportMulticastPeer {
                pid: join.pid,
                whatami: join.whatami,
                conduit_rx,
            };
            guard.insert(locator.clone(), peer);
        }

        Ok(())
    }

    pub(super) fn handle_close(&self, close: Close, locator: &Locator) -> ZResult<()> {
        let mut guard = zwrite!(self.peers);
        if let Some(peer) = guard.remove(locator) {
            log::debug!(
                "Peer {}/{}/{} has left multicast {} with reason: {}",
                peer.pid,
                peer.whatami,
                locator,
                self.locator,
                close.reason
            );
        }
        Ok(())
    }

    pub(super) fn receive_message(&self, msg: TransportMessage, locator: &Locator) -> ZResult<()> {
        // Process the received message
        match msg.body {
            TransportBody::Frame(Frame {
                channel,
                sn,
                payload,
            }) => match zread!(self.peers).get(locator) {
                Some(p) => {
                    let c = if self.is_qos() {
                        &p.conduit_rx[channel.priority as usize]
                    } else if channel.priority == Priority::default() {
                        &p.conduit_rx[0]
                    } else {
                        let e = format!(
                            "Transport {}: {}. Unknown conduit {:?} from {}.",
                            self.manager.config.pid, self.locator, channel.priority, locator
                        );
                        return zerror!(ZErrorKind::InvalidMessage { descr: e });
                    };

                    match channel.reliability {
                        Reliability::Reliable => {
                            self.handle_frame(sn, payload, zlock!(c.reliable), &p.pid)
                        }
                        Reliability::BestEffort => {
                            self.handle_frame(sn, payload, zlock!(c.best_effort), &p.pid)
                        }
                    }
                }
                None => {
                    log::debug!(
                        "Transport {}: {}. {} is not a multicast member.",
                        self.manager.config.pid,
                        self.locator,
                        locator
                    );
                    Ok(())
                }
            },
            TransportBody::Join(join) => self.handle_join(join, locator),
            TransportBody::Close(close) => self.handle_close(close, locator),
            TransportBody::KeepAlive(KeepAlive { .. }) => Ok(()),
            _ => {
                log::debug!(
                    "Transport: {}. Message handling not implemented: {:?}",
                    self.manager.config.pid,
                    msg
                );
                Ok(())
            }
        }
    }
}
