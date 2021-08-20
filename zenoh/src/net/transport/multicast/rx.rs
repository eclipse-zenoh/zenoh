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
use super::protocol::core::{PeerId, Priority, Reliability, ZInt};
use super::protocol::proto::{
    Frame, FramePayload, KeepAlive, TransportBody, TransportMessage, ZenohMessage,
};
use super::transport::TransportMulticastInner;
use crate::net::link::Locator;
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
                    "Transport: {}. No callback available, dropping message: {}",
                    self.pid,
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
                self.pid,
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
                        let e = format!("Transport: {}. Defragmentation error.", self.pid);
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

    pub(super) fn receive_message(&self, msg: TransportMessage, locator: &Locator) -> ZResult<()> {
        log::trace!("Received: {:?}", msg);
        // Process the received message
        match msg.body {
            TransportBody::Frame(Frame {
                channel,
                sn,
                payload,
            }) => match zlock!(self.peers).get(locator) {
                Some(p) => {
                    let c = if self.is_qos() {
                        &p.conduit_rx[channel.priority as usize]
                    } else if channel.priority == Priority::default() {
                        &p.conduit_rx[0]
                    } else {
                        let e = format!(
                            "Transport: {}. Unknown conduit: {:?}.",
                            self.pid, channel.priority
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
                        "Transport: {}. {} is not a multicast member.",
                        self.pid,
                        locator
                    );
                    Ok(())
                }
            },
            // TransportBody::Sync(Sync { pid, sn, .. }) => {
            //                     let mut guard = zlock!(self.peers);
            //     if !guard.contains_key(locator) {
            //         let mut conduit_rx = vec![];
            //         if self.is_qos() {
            //             for c in 0..Priority::NUM {
            //                 conduit_rx.push(TransportConduitRx::new(
            //                     (c as u8).try_into().unwrap(),
            //                     config.initial_sn_rx,
            //                     config.sn_resolution,
            //                 ));
            //             }
            //         } else {

            //         }
            // }
            //         let conduit_rx = TransportMulticastPeer {pid, conduit_rx: };
            //         guard.insert(locator.clone(), pid);
            //     Ok(())
            // }
            TransportBody::Close(_) => panic!(),
            TransportBody::KeepAlive(KeepAlive { .. }) => Ok(()),
            _ => {
                log::debug!(
                    "Transport: {}. Message handling not implemented: {:?}",
                    self.pid,
                    msg
                );
                Ok(())
            }
        }
    }
}
