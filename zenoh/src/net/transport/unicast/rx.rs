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
use super::protocol::core::{Priority, Reliability};
#[cfg(feature = "stats")]
use super::protocol::message::ZenohBody;
use super::transport::TransportUnicastInner;
use crate::net::link::LinkUnicast;
use crate::net::protocol::io::ZBuf;
use crate::net::protocol::message::{
    mid, Close, CloseReason, Fragment, Frame, KeepAlive, TransportId, ZMessage, ZenohMessage,
};
use async_std::task;
use std::convert::TryFrom;
use zenoh_util::core::Result as ZResult;
use zenoh_util::{zerror, zread};

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
        }

        let callback = zread!(self.callback).clone();
        if let Some(callback) = callback.as_ref() {
            #[cfg(feature = "shared-memory")]
            let _ = msg.map_to_shmbuf(self.config.manager.shmr.clone())?;
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

    fn handle_close(&self, link: &LinkUnicast, reason: CloseReason) -> ZResult<()> {
        log::debug!(
            "Transport: {}. Received close on link: {}. Reason: {:?}",
            self.config.zid,
            link,
            reason
        );

        // Stop now rx and tx tasks before doing the proper cleanup
        let _ = self.stop_rx(link);
        let _ = self.stop_tx(link);

        // Delete and clean up
        let c_transport = self.clone();
        let c_link = link.clone();
        // Spawn a task to avoid a deadlock waiting for this same task
        // to finish in the link close() joining the rx handle
        task::spawn(async move {
            let _ = c_transport.del_link(&c_link).await;
            let _ = c_transport.delete().await;
        });

        Ok(())
    }

    fn deserialize_frame(&self, zbuf: &mut ZBuf, header: u8) -> ZResult<()> {
        let (reliability, sn, exts) = Frame::read_header(zbuf, header).unwrap();

        let c = if self.is_qos() {
            match exts.qos.as_ref() {
                Some(q) => &self.conduit_rx[q.priority.id() as usize],
                None => &self.conduit_rx[Priority::default().id() as usize],
            }
        } else {
            &self.conduit_rx[0]
        };

        let mut guard = match reliability {
            Reliability::Reliable => zlock!(c.reliable),
            Reliability::BestEffort => zlock!(c.best_effort),
        };

        let precedes = guard.sn.precedes(sn)?;
        if precedes {
            let _ = guard.sn.set(sn);
            while zbuf.can_read() {
                let pos = zbuf.get_pos();
                if let Some(msg) = zbuf.read_zenoh_message(reliability) {
                    self.trigger_callback(msg)?;
                } else if zbuf.set_pos(pos) {
                    break;
                } else {
                    bail!("decoding error");
                }
            }
        } else {
            log::debug!(
                "Transport: {}. Frame with invalid SN dropped: {}. Expected: {}.",
                self.config.zid,
                sn,
                guard.sn.get()
            );
            Frame::skip_body(zbuf).unwrap();
        }

        Ok(())
    }

    fn deserialize_fragment(&self, zbuf: &mut ZBuf, header: u8) -> ZResult<()> {
        let (reliability, sn, has_more, exts) = Fragment::read_header(zbuf, header).unwrap();

        let c = if self.is_qos() {
            match exts.qos.as_ref() {
                Some(q) => &self.conduit_rx[q.priority.id() as usize],
                None => &self.conduit_rx[Priority::default().id() as usize],
            }
        } else {
            &self.conduit_rx[0]
        };

        let mut guard = match reliability {
            Reliability::Reliable => zlock!(c.reliable),
            Reliability::BestEffort => zlock!(c.best_effort),
        };

        let precedes = guard.sn.precedes(sn)?;
        if precedes {
            let _ = guard.sn.set(sn);
            if guard.defrag.is_empty() {
                let _ = guard.defrag.sync(sn);
            }
            let body = Fragment::read_body(zbuf).unwrap();
            let _ = guard.defrag.push(sn, body)?;
            if !has_more {
                // When shared-memory feature is disabled, msg does not need to be mutable
                let msg = guard.defrag.defragment().ok_or_else(|| {
                    zerror!("Transport: {}. Defragmentation error.", self.config.zid)
                })?;
                self.trigger_callback(msg)?;
            }
        } else {
            log::debug!(
                "Transport: {}. Fragment with invalid SN dropped: {}. Expected: {}.",
                self.config.zid,
                sn,
                guard.sn.get()
            );
            // Drop the fragments if needed
            if !guard.defrag.is_empty() {
                guard.defrag.clear();
            }
            Fragment::skip_body(zbuf).unwrap();
        }

        Ok(())
    }

    pub(super) fn deserialize(&self, zbuf: &mut ZBuf, link: &LinkUnicast) -> ZResult<()> {
        // Deserialize all the messages from the current ZBuf
        while zbuf.can_read() {
            let header = zbuf.read().unwrap();

            let tid = TransportId::try_from(mid(header)).unwrap();
            match tid {
                TransportId::Frame => self.deserialize_frame(zbuf, header)?,
                TransportId::Fragment => self.deserialize_fragment(zbuf, header)?,
                TransportId::KeepAlive => {
                    let _ka = KeepAlive::read(zbuf, header).unwrap();
                }
                TransportId::Close => {
                    let cl = Close::read(zbuf, header).unwrap();
                    self.handle_close(link, cl.reason)?;
                }
                _ => {
                    bail!("{}: decoding error", link);
                }
            }
        }

        Ok(())
    }
}
