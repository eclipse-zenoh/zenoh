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

use zenoh_buffers::ZSlice;
use zenoh_codec::transport::frame::FrameReader;
use zenoh_core::{zlock, zread};
use zenoh_protocol::{
    core::{Locator, Priority, Reliability},
    network::NetworkMessage,
    transport::{
        BatchSize, Close, Fragment, Join, KeepAlive, TransportBody, TransportMessage, TransportSn,
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
        #[cfg(feature = "stats")]
        {
            #[cfg(feature = "shared-memory")]
            {
                use zenoh_protocol::network::NetworkMessageExt;
                if msg.is_shm() {
                    self.stats.rx_n_msgs.inc_shm(1);
                } else {
                    self.stats.rx_n_msgs.inc_net(1);
                }
            }
            #[cfg(not(feature = "shared-memory"))]
            self.stats.rx_n_msgs.inc_net(1);
        }
        #[cfg(feature = "shared-memory")]
        {
            if let Some(shm_context) = &self.shm_context {
                if let Err(e) = crate::shm::map_zmsg_to_shmbuf(&mut msg, &shm_context.shm_reader) {
                    tracing::debug!("Error receiving SHM buffer: {e}");
                    return Ok(());
                }
            }
        }
        peer.handler.handle_message(msg.as_mut())
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

    fn handle_frame(
        &self,
        frame: FrameReader<ZSlice>,
        peer: &TransportMulticastPeer,
    ) -> ZResult<()> {
        let priority = frame.ext_qos.priority();
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

        let mut guard = match frame.reliability {
            Reliability::Reliable => zlock!(c.reliable),
            Reliability::BestEffort => zlock!(c.best_effort),
        };

        if !self.verify_sn("Frame", frame.sn, &mut guard)? {
            // Drop invalid message and continue
            return Ok(());
        }
        for msg in frame {
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
            ext_first,
            ext_drop,
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

        if !self.verify_sn("Fragment", sn, &mut guard)? {
            // Drop invalid message and continue
            return Ok(());
        }
        if peer.patch.has_fragmentation_markers() {
            if ext_first.is_some() {
                guard.defrag.clear();
            } else if guard.defrag.is_empty() {
                tracing::trace!(
                    "Transport: {}. First fragment received without start marker.",
                    self.manager.config.zid,
                );
                return Ok(());
            }
            if ext_drop.is_some() {
                guard.defrag.clear();
                return Ok(());
            }
        }
        if guard.defrag.is_empty() {
            let _ = guard.defrag.sync(sn);
        }
        if let Err(e) = guard.defrag.push(sn, payload) {
            // Defrag errors don't close transport
            tracing::trace!("{}", e);
            return Ok(());
        }
        if !more {
            // When shared-memory feature is disabled, msg does not need to be mutable
            if let Some(msg) = guard.defrag.defragment() {
                return self.trigger_callback(msg, peer);
            } else {
                tracing::trace!(
                    "Transport: {}. Peer: {}. Priority: {:?}. Defragmentation error.",
                    self.manager.config.zid,
                    peer.zid,
                    priority
                );
            }
        }

        Ok(())
    }

    fn verify_sn(
        &self,
        message_type: &str,
        sn: TransportSn,
        guard: &mut MutexGuard<'_, TransportChannelRx>,
    ) -> ZResult<bool> {
        let precedes = guard.sn.precedes(sn)?;
        if !precedes {
            tracing::debug!(
                "Transport: {}. {} with invalid SN dropped: {}. Expected: {}.",
                self.manager.config.zid,
                message_type,
                sn,
                guard.sn.next()
            );
            return Ok(false);
        }

        // Set will always return OK because we have already checked
        // with precedes() that the sn has the right resolution
        let _ = guard.sn.set(sn);

        Ok(true)
    }

    pub(super) fn read_messages(
        &self,
        mut batch: RBatch,
        locator: Locator,
        batch_size: BatchSize,
        #[cfg(feature = "stats")] transport: &TransportMulticastInner,
    ) -> ZResult<()> {
        while !batch.is_empty() {
            if let Ok(frame) = batch.decode() {
                tracing::trace!("Received: {:?}", frame);
                #[cfg(feature = "stats")]
                {
                    transport.stats.inc_rx_t_msgs(1);
                }
                if let Some(peer) = zread!(self.peers).get(&locator) {
                    peer.set_active();
                    self.handle_frame(frame, peer)?;
                }
                continue;
            }
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
                        TransportBody::Frame(_) => unreachable!(),
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
