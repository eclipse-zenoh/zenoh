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
use std::sync::MutexGuard;

use zenoh_core::{zlock, zread};
use zenoh_protocol::{
    core::{Locator, Priority, Reliability},
    network::NetworkMessage,
    transport::{
        BatchSize, Close, Fragment, Frame, Join, KeepAlive, TransportBody, TransportMessage,
        TransportSn,
    },
};
use zenoh_result::{bail, zerror, ZResult};

use super::transport::{TransportMulticastInner, TransportMulticastPeer};
use crate::common::{
    batch::{Decode, RBatch},
    priority::TransportChannelRx,
};

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
//noinspection ALL
impl TransportMulticastInner {
    fn trigger_callback(
        &self,
        #[allow(unused_mut)] // shared-memory feature requires mut
        mut msg: NetworkMessage,
        peer: &TransportMulticastPeer,
    ) -> ZResult<()> {
        #[cfg(feature = "shared-memory")]
        {
            if self.manager.config.multicast.is_shm {
                if let Err(e) = crate::shm::map_zmsg_to_shmbuf(&mut msg, &self.manager.shmr) {
                    tracing::debug!("Error receiving SHM buffer: {e}");
                    return Ok(());
                }
            }
        }

        peer.handler.handle_message(msg)
    }

    pub(super) fn handle_join_from_peer(
        &self,
        join: Join,
        peer: &TransportMulticastPeer,
    ) -> ZResult<()> {
        // Check if parameters are ok
        if join.version != peer.version
            || join.zid != peer.zid
            || join.whatami != peer.whatami
            || join.resolution != peer.resolution
            || join.lease != peer.lease
            || join.ext_qos.is_some() != peer.is_qos()
        {
            let e = format!(
                "Ignoring Join on {} of peer: {}. Inconsistent parameters.",
                peer.locator, peer.zid,
            );
            tracing::debug!("{}", e);
            bail!("{}", e);
        }

        Ok(())
    }

    pub(super) fn handle_join_from_unknown(
        &self,
        join: Join,
        locator: &Locator,
        batch_size: BatchSize,
    ) -> ZResult<()> {
        if zread!(self.peers).len() >= self.manager.config.multicast.max_sessions {
            tracing::debug!(
                "Ignoring Join on {} from peer: {}. Max sessions reached: {}.",
                locator,
                join.zid,
                self.manager.config.multicast.max_sessions,
            );
            return Ok(());
        }

        if join.version != self.manager.config.version {
            tracing::debug!(
                "Ignoring Join on {} from peer: {}. Unsupported version: {}. Expected: {}.",
                locator,
                join.zid,
                join.version,
                self.manager.config.version,
            );
            return Ok(());
        }

        if join.resolution != self.manager.config.resolution {
            tracing::debug!(
                "Ignoring Join on {} from peer: {}. Unsupported SN resolution: {:?}. Expected: {:?}.",
                locator,
                join.zid,
                join.resolution,
                self.manager.config.resolution,
            );
            return Ok(());
        }

        if join.batch_size != batch_size {
            tracing::debug!(
                "Ignoring Join on {} from peer: {}. Unsupported Batch Size: {:?}. Expected: {:?}.",
                locator,
                join.zid,
                join.batch_size,
                batch_size
            );
            return Ok(());
        }

        if !self.manager.config.multicast.is_qos && join.ext_qos.is_some() {
            tracing::debug!(
                "Ignoring Join on {} from peer: {}. QoS is not supported.",
                locator,
                join.zid,
            );
            return Ok(());
        }

        self.new_peer(locator, join)
    }

    fn handle_frame(&self, frame: Frame, peer: &TransportMulticastPeer) -> ZResult<()> {
        let Frame {
            reliability,
            sn,
            ext_qos,
            mut payload,
        } = frame;

        let priority = ext_qos.priority();
        let c = if self.is_qos() {
            &peer.priority_rx[priority as usize]
        } else if priority == Priority::DEFAULT {
            &peer.priority_rx[0]
        } else {
            bail!(
                "Transport: {}. Peer: {}. Unknown priority: {:?}.",
                self.manager.config.zid,
                peer.zid,
                priority
            );
        };

        let mut guard = match reliability {
            Reliability::Reliable => zlock!(c.reliable),
            Reliability::BestEffort => zlock!(c.best_effort),
        };

        self.verify_sn(sn, &mut guard)?;

        for msg in payload.drain(..) {
            self.trigger_callback(msg, peer)?;
        }
        Ok(())
    }

    fn handle_fragment(&self, fragment: Fragment, peer: &TransportMulticastPeer) -> ZResult<()> {
        let Fragment {
            reliability,
            more,
            sn,
            ext_qos,
            payload,
        } = fragment;

        let priority = ext_qos.priority();
        let c = if self.is_qos() {
            &peer.priority_rx[priority as usize]
        } else if priority == Priority::DEFAULT {
            &peer.priority_rx[0]
        } else {
            bail!(
                "Transport: {}. Peer: {}. Unknown priority: {:?}.",
                self.manager.config.zid,
                peer.zid,
                priority
            );
        };

        let mut guard = match reliability {
            Reliability::Reliable => zlock!(c.reliable),
            Reliability::BestEffort => zlock!(c.best_effort),
        };

        self.verify_sn(sn, &mut guard)?;

        if guard.defrag.is_empty() {
            let _ = guard.defrag.sync(sn);
        }
        guard.defrag.push(sn, payload)?;
        if !more {
            // When shared-memory feature is disabled, msg does not need to be mutable
            let msg = guard.defrag.defragment().ok_or_else(|| {
                zerror!(
                    "Transport: {}. Peer: {}. Priority: {:?}. Defragmentation error.",
                    self.manager.config.zid,
                    peer.zid,
                    priority
                )
            })?;
            return self.trigger_callback(msg, peer);
        }

        Ok(())
    }

    fn verify_sn(
        &self,
        sn: TransportSn,
        guard: &mut MutexGuard<'_, TransportChannelRx>,
    ) -> ZResult<()> {
        let precedes = guard.sn.precedes(sn)?;
        if !precedes {
            tracing::debug!(
                "Transport: {}. Frame with invalid SN dropped: {}. Expected: {}.",
                self.manager.config.zid,
                sn,
                guard.sn.next()
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

        Ok(())
    }

    pub(super) fn read_messages(
        &self,
        mut batch: RBatch,
        locator: Locator,
        batch_size: BatchSize,
        #[cfg(feature = "stats")] transport: &TransportMulticastInner,
    ) -> ZResult<()> {
        while !batch.is_empty() {
            let msg: TransportMessage = batch
                .decode()
                .map_err(|_| zerror!("{}: decoding error", locator))?;

            tracing::trace!("Received: {:?}", msg);

            #[cfg(feature = "stats")]
            {
                transport.stats.inc_rx_t_msgs(1);
            }

            let r_guard = zread!(self.peers);
            match r_guard.get(&locator) {
                Some(peer) => {
                    peer.set_active();
                    match msg.body {
                        TransportBody::Frame(msg) => self.handle_frame(msg, peer)?,
                        TransportBody::Fragment(fragment) => {
                            self.handle_fragment(fragment, peer)?
                        }
                        TransportBody::Join(join) => self.handle_join_from_peer(join, peer)?,
                        TransportBody::KeepAlive(KeepAlive { .. }) => {}
                        TransportBody::Close(Close { reason, .. }) => {
                            drop(r_guard);
                            self.del_peer(&locator, reason)?;
                        }
                        _ => {
                            tracing::debug!(
                                "Transport: {}. Message handling not implemented: {:?}",
                                self.manager.config.zid,
                                msg
                            );
                        }
                    }
                }
                None => {
                    drop(r_guard);
                    if let TransportBody::Join(join) = msg.body {
                        self.handle_join_from_unknown(join, &locator, batch_size)?;
                    }
                }
            }
        }

        Ok(())
    }
}
