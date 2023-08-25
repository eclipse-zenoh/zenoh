use crate::transport_unicast_inner::TransportUnicastTrait;

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
use super::transport::TransportUnicastNet;
use crate::common::priority::TransportChannelRx;
use async_std::task;
use std::sync::MutexGuard;
use zenoh_buffers::{
    reader::{HasReader, Reader},
    ZSlice,
};
use zenoh_codec::{RCodec, Zenoh080};
use zenoh_core::{zlock, zread};
use zenoh_link::LinkUnicast;
use zenoh_protocol::{
    core::{Priority, Reliability},
    network::NetworkMessage,
    transport::{Close, Fragment, Frame, KeepAlive, TransportBody, TransportMessage, TransportSn},
};
use zenoh_result::{bail, zerror, ZResult};

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
impl TransportUnicastNet {
    fn trigger_callback(
        &self,
        #[allow(unused_mut)] // shared-memory feature requires mut
        mut msg: NetworkMessage,
    ) -> ZResult<()> {
        #[cfg(feature = "stats")]
        {
            use zenoh_buffers::SplitBuffer;
            use zenoh_protocol::network::NetworkBody;
            use zenoh_protocol::zenoh_new::{PushBody, RequestBody, ResponseBody};
            self.stats.inc_rx_n_msgs(1);
            match &msg.body {
                NetworkBody::Push(push) => match &push.payload {
                    PushBody::Put(p) => {
                        self.stats.inc_rx_z_put_user_msgs(1);
                        self.stats.inc_rx_z_put_user_pl_bytes(p.payload.len());
                    }
                    PushBody::Del(_) => self.stats.inc_rx_z_del_user_msgs(1),
                },
                NetworkBody::Request(req) => match &req.payload {
                    RequestBody::Put(p) => {
                        self.stats.inc_rx_z_put_user_msgs(1);
                        self.stats.inc_rx_z_put_user_pl_bytes(p.payload.len());
                    }
                    RequestBody::Del(_) => self.stats.inc_rx_z_del_user_msgs(1),
                    RequestBody::Query(q) => {
                        self.stats.inc_rx_z_query_user_msgs(1);
                        self.stats.inc_rx_z_query_user_pl_bytes(
                            q.ext_body.as_ref().map(|b| b.payload.len()).unwrap_or(0),
                        );
                    }
                    RequestBody::Pull(_) => (),
                },
                NetworkBody::Response(res) => match &res.payload {
                    ResponseBody::Put(p) => {
                        self.stats.inc_rx_z_put_user_msgs(1);
                        self.stats.inc_rx_z_put_user_pl_bytes(p.payload.len());
                    }
                    ResponseBody::Reply(r) => {
                        self.stats.inc_rx_z_reply_user_msgs(1);
                        self.stats.inc_rx_z_reply_user_pl_bytes(r.payload.len());
                    }
                    ResponseBody::Err(e) => {
                        self.stats.inc_rx_z_reply_user_msgs(1);
                        self.stats.inc_rx_z_reply_user_pl_bytes(
                            e.ext_body.as_ref().map(|b| b.payload.len()).unwrap_or(0),
                        );
                    }
                    ResponseBody::Ack(_) => (),
                },
                NetworkBody::ResponseFinal(_) => (),
                NetworkBody::Declare(_) => (),
                NetworkBody::OAM(_) => (),
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
            &self.priority_rx[priority as usize]
        } else if priority == Priority::default() {
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
            &self.priority_rx[qos.priority() as usize]
        } else if qos.priority() == Priority::default() {
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
