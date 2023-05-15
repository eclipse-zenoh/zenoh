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
use super::common::conduit::TransportChannelRx;
use super::transport::TransportUnicastInner;
use async_std::task;
use std::sync::MutexGuard;
#[cfg(feature = "stats")]
use zenoh_buffers::SplitBuffer;
use zenoh_core::{zlock, zread};
use zenoh_link::LinkUnicast;
#[cfg(feature = "stats")]
use zenoh_protocol::zenoh::ZenohBody;
use zenoh_protocol::{
    core::{Priority, Reliability, ZInt, ZenohId},
    transport::{tmsg, Close, Frame, FramePayload, KeepAlive, TransportBody, TransportMessage},
    zenoh::ZenohMessage,
};
use zenoh_result::{bail, zerror, ZResult};

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
impl TransportUnicastInner {
    fn trigger_callback(
        &self,
        #[allow(unused_mut)] // shared-memory feature requires mut
        mut msg: ZenohMessage,
    ) -> ZResult<()> {
        #[cfg(feature = "stats")]
        {
            self.stats.inc_rx_z_msgs(1);
            match &msg.body {
                ZenohBody::Data(data) => match data.reply_context {
                    Some(_) => {
                        self.stats.inc_rx_z_data_reply_msgs(1);
                        self.stats
                            .inc_rx_z_data_reply_payload_bytes(data.payload.len());
                    }
                    None => {
                        self.stats.inc_rx_z_data_msgs(1);
                        self.stats.inc_rx_z_data_payload_bytes(data.payload.len());
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
        }

        let callback = zread!(self.callback).clone();
        if let Some(callback) = callback.as_ref() {
            #[cfg(feature = "shared-memory")]
            {
                crate::shm::map_zmsg_to_shmbuf(&mut msg, &self.config.manager.shmr)?;
            }
            callback.handle_message(msg)
        } else {
            log::debug!(
                "Transport: {}. No callback available, dropping message: {}",
                self.config.zid,
                msg
            );
            Ok(())
        }
    }

    fn handle_close(
        &self,
        link: &LinkUnicast,
        zid: Option<ZenohId>,
        reason: u8,
        link_only: bool,
    ) -> ZResult<()> {
        // Check if the PID is correct when provided
        if let Some(zid) = zid {
            if zid != self.config.zid {
                bail!(
                    "Transport: {}. Invalid close zid: {} reason: {}.",
                    self.config.zid,
                    zid,
                    tmsg::close_reason_to_str(reason)
                );
            }
        }

        // Stop now rx and tx tasks before doing the proper cleanup
        let _ = self.stop_rx(link);
        let _ = self.stop_tx(link);

        // Delete and clean up
        let c_transport = self.clone();
        let c_link = link.clone();
        // Spawn a task to avoid a deadlock waiting for this same task
        // to finish in the link close() joining the rx handle
        task::spawn(async move {
            if link_only {
                let _ = c_transport.del_link(&c_link).await;
            } else {
                let _ = c_transport.delete().await;
            }
        });

        Ok(())
    }

    fn handle_frame(
        &self,
        sn: ZInt,
        payload: FramePayload,
        mut guard: MutexGuard<'_, TransportChannelRx>,
    ) -> ZResult<()> {
        let precedes = guard.sn.precedes(sn)?;
        if !precedes {
            log::debug!(
                "Transport: {}. Frame with invalid SN dropped: {}. Expected: {}.",
                self.config.zid,
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
                        zerror!("Transport: {}. Defragmentation error.", self.config.zid)
                    })?;
                    self.trigger_callback(msg)
                } else {
                    Ok(())
                }
            }
            FramePayload::Messages { mut messages } => {
                for msg in messages.drain(..) {
                    self.trigger_callback(msg)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn receive_message(&self, msg: TransportMessage, link: &LinkUnicast) -> ZResult<()> {
        log::trace!("Received: {:?}", msg);
        // Process the received message
        match msg.body {
            TransportBody::Frame(Frame {
                channel,
                sn,
                payload,
            }) => {
                let c = if self.is_qos() {
                    &self.conduit_rx[channel.priority as usize]
                } else if channel.priority == Priority::default() {
                    &self.conduit_rx[0]
                } else {
                    bail!(
                        "Transport: {}. Unknown conduit: {:?}.",
                        self.config.zid,
                        channel.priority
                    );
                };

                match channel.reliability {
                    Reliability::Reliable => self.handle_frame(sn, payload, zlock!(c.reliable)),
                    Reliability::BestEffort => {
                        self.handle_frame(sn, payload, zlock!(c.best_effort))
                    }
                }
            }
            TransportBody::Close(Close {
                zid,
                reason,
                link_only,
            }) => self.handle_close(link, zid, reason, link_only),
            TransportBody::KeepAlive(KeepAlive { .. }) => Ok(()),
            _ => {
                log::debug!(
                    "Transport: {}. Message handling not implemented: {:?}",
                    self.config.zid,
                    msg
                );
                Ok(())
            }
        }
    }
}
