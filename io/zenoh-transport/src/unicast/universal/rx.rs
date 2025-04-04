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

use zenoh_codec::network::NetworkMessageIter;
use zenoh_core::{zlock, zread};
use zenoh_link::Link;
use zenoh_protocol::{
    core::{Priority, Reliability},
    network::NetworkMessage,
    transport::{Close, Fragment, Frame, KeepAlive, TransportBody, TransportMessage, TransportSn},
};
use zenoh_result::{bail, zerror, ZResult};

use super::transport::TransportUnicastUniversal;
use crate::{
    common::{
        batch::{Decode, RBatch},
        priority::TransportChannelRx,
    },
    unicast::transport_unicast_inner::TransportUnicastTrait,
    TransportPeerEventHandler,
};

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
impl TransportUnicastUniversal {
    fn trigger_callback(
        &self,
        callback: &dyn TransportPeerEventHandler,
        #[allow(unused_mut)] // shared-memory feature requires mut
        mut msg: NetworkMessage,
    ) -> ZResult<()> {
        #[cfg(feature = "shared-memory")]
        {
            if self.config.shm.is_some() {
                if let Err(e) = crate::shm::map_zmsg_to_shmbuf(&mut msg, &self.manager.shmr) {
                    tracing::debug!("Error receiving SHM buffer: {e}");
                    return Ok(());
                }
            }
        }
        callback.handle_message(msg)
    }

    fn handle_close(&self, link: &Link, _reason: u8, session: bool) -> ZResult<()> {
        // Delete and clean up
        let c_transport = self.clone();
        let c_link = link.clone();
        // Spawn a task to avoid a deadlock waiting for this same task
        // to finish in the link close() joining the rx handle
        zenoh_runtime::ZRuntime::Net.spawn(async move {
            if session {
                let _ = c_transport.delete().await;
            } else {
                let _ = c_transport.del_link(c_link).await;
            }
        });

        Ok(())
    }

    fn handle_frame(&self, frame: Frame) -> ZResult<()> {
        let Frame {
            reliability,
            sn,
            ext_qos,
            mut payload,
        } = frame;

        let priority = ext_qos.priority();
        let c = if self.is_qos() {
            &self.priority_rx[priority as usize]
        } else if priority == Priority::DEFAULT {
            &self.priority_rx[0]
        } else {
            bail!(
                "Transport: {}. Unknown priority: {:?}.",
                self.config.zid,
                priority
            );
        };

        let mut guard = match reliability {
            Reliability::Reliable => zlock!(c.reliable),
            Reliability::BestEffort => zlock!(c.best_effort),
        };

        if !self.verify_sn("Frame", sn, &mut guard)? {
            // Drop invalid message and continue
            return Ok(());
        }
        let callback = zread!(self.callback).clone();
        if let Some(callback) = callback.as_ref() {
            for msg in NetworkMessageIter::new(reliability, &mut payload) {
                self.trigger_callback(callback.as_ref(), msg)?;
            }
        } else {
            tracing::debug!(
                "Transport: {}. No callback available, dropping messages: {:?}",
                self.config.zid,
                payload
            );
        }

        Ok(())
    }

    fn handle_fragment(&self, fragment: Fragment) -> ZResult<()> {
        let Fragment {
            reliability,
            more,
            sn,
            ext_qos: qos,
            ext_first,
            ext_drop,
            payload,
        } = fragment;

        let c = if self.is_qos() {
            &self.priority_rx[qos.priority() as usize]
        } else if qos.priority() == Priority::DEFAULT {
            &self.priority_rx[0]
        } else {
            bail!(
                "Transport: {}. Unknown priority: {:?}.",
                self.config.zid,
                qos.priority()
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
        if self.config.patch.has_fragmentation_markers() {
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
                let callback = zread!(self.callback).clone();
                if let Some(callback) = callback.as_ref() {
                    return self.trigger_callback(callback.as_ref(), msg);
                } else {
                    tracing::debug!(
                        "Transport: {}. No callback available, dropping messages: {:?}",
                        self.config.zid,
                        msg
                    );
                }
            } else {
                tracing::trace!("Transport: {}. Defragmentation error.", self.config.zid);
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
        let precedes = guard.sn.roll(sn)?;
        if !precedes {
            tracing::trace!(
                "Transport: {}. {} with invalid SN dropped: {}. Expected: {}.",
                self.config.zid,
                message_type,
                sn,
                guard.sn.next()
            );
            return Ok(false);
        }

        Ok(true)
    }

    pub(super) fn read_messages(&self, mut batch: RBatch, link: &Link) -> ZResult<()> {
        while !batch.is_empty() {
            let msg: TransportMessage = batch
                .decode()
                .map_err(|_| zerror!("{}: decoding error", link))?;

            tracing::trace!("Received: {:?}", msg);

            #[cfg(feature = "stats")]
            {
                self.stats.inc_rx_t_msgs(1);
            }

            match msg.body {
                TransportBody::Frame(msg) => self.handle_frame(msg)?,
                TransportBody::Fragment(fragment) => self.handle_fragment(fragment)?,
                TransportBody::Close(Close { reason, session }) => {
                    self.handle_close(link, reason, session)?
                }
                TransportBody::KeepAlive(KeepAlive { .. }) => {}
                _ => {
                    tracing::debug!(
                        "Transport: {}. Message handling not implemented: {:?}",
                        self.config.zid,
                        msg
                    );
                }
            }
        }

        // Process the received message

        Ok(())
    }
}
