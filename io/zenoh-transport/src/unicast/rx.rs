//
// Copyright (c) 2022 ZettaScale Technology
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
use zenoh_buffers::{
    reader::{HasReader, Reader},
    ZSlice,
};
use zenoh_codec::{RCodec, Zenoh080};
use zenoh_core::{zlock, zread};
use zenoh_link::LinkUnicast;
#[cfg(feature = "stats")]
use zenoh_protocol::zenoh::ZenohBody;
use zenoh_protocol::{
    core::{Priority, Reliability},
    transport::{Close, Fragment, Frame, KeepAlive, TransportBody, TransportMessage, TransportSn},
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
                if self.config.is_shm {
                    crate::shm::map_zmsg_to_shmbuf(
                        &mut msg,
                        &self.manager.state.unicast.shm.reader,
                    )?;
                }
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

    fn handle_close(&self, link: &LinkUnicast, _reason: u8, session: bool) -> ZResult<()> {
        // Stop now rx and tx tasks before doing the proper cleanup
        let _ = self.stop_rx(link);
        let _ = self.stop_tx(link);

        // Delete and clean up
        let c_transport = self.clone();
        let c_link = link.clone();
        // Spawn a task to avoid a deadlock waiting for this same task
        // to finish in the link close() joining the rx handle
        task::spawn(async move {
            if session {
                let _ = c_transport.delete().await;
            } else {
                let _ = c_transport.del_link(&c_link).await;
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
            &self.conduit_rx[priority as usize]
        } else if priority == Priority::default() {
            &self.conduit_rx[0]
        } else {
            bail!(
                "Transport: {}. Unknown conduit: {:?}.",
                self.config.zid,
                priority
            );
        };

        let mut guard = match reliability {
            Reliability::Reliable => zlock!(c.reliable),
            Reliability::BestEffort => zlock!(c.best_effort),
        };

        self.verify_sn(sn, &mut guard)?;

        for msg in payload.drain(..) {
            self.trigger_callback(msg)?;
        }
        Ok(())
    }

    fn handle_fragment(&self, fragment: Fragment) -> ZResult<()> {
        let Fragment {
            reliability,
            more,
            sn,
            ext_qos: qos,
            payload,
        } = fragment;

        let c = if self.is_qos() {
            &self.conduit_rx[qos.priority() as usize]
        } else if qos.priority() == Priority::default() {
            &self.conduit_rx[0]
        } else {
            bail!(
                "Transport: {}. Unknown conduit: {:?}.",
                self.config.zid,
                qos.priority()
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
            let msg = guard
                .defrag
                .defragment()
                .ok_or_else(|| zerror!("Transport: {}. Defragmentation error.", self.config.zid))?;
            return self.trigger_callback(msg);
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

        Ok(())
    }

    pub(super) fn read_messages(&self, mut zslice: ZSlice, link: &LinkUnicast) -> ZResult<()> {
        let codec = Zenoh080::new();
        let mut reader = zslice.reader();
        while reader.can_read() {
            let msg: TransportMessage = codec
                .read(&mut reader)
                .map_err(|_| zerror!("{}: decoding error", link))?;

            log::trace!("Received: {:?}", msg);

            #[cfg(feature = "stats")]
            {
                transport.stats.inc_rx_t_msgs(1);
            }

            match msg.body {
                TransportBody::Frame(msg) => self.handle_frame(msg)?,
                TransportBody::Fragment(fragment) => self.handle_fragment(fragment)?,
                TransportBody::Close(Close { reason, session }) => {
                    self.handle_close(link, reason, session)?
                }
                TransportBody::KeepAlive(KeepAlive { .. }) => {}
                _ => {
                    log::debug!(
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
