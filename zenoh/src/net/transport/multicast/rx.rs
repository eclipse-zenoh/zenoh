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
use super::protocol::message::{
    mid, Close, Fragment, Frame, Join, KeepAlive, TransportId, ZMessage, ZenohMessage,
};
use super::transport::{TransportMulticastInner, TransportMulticastPeer};
use crate::net::link::Locator;
use crate::net::protocol::io::ZBuf;
use std::convert::TryFrom;
use zenoh_util::core::Result as ZResult;
use zenoh_util::zerror;

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
impl TransportMulticastInner {
    fn trigger_callback(
        &self,
        #[allow(unused_mut)] // shared-memory feature requires mut
        mut msg: ZenohMessage,
        peer: &TransportMulticastPeer,
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

        #[cfg(feature = "shared-memory")]
        let _ = msg.map_to_shmbuf(self.manager.shmr.clone())?;
        peer.handler.handle_message(msg)
    }

    fn deserialize_frame(
        &self,
        zbuf: &mut ZBuf,
        header: u8,
        peer: &TransportMulticastPeer,
    ) -> ZResult<()> {
        let (reliability, sn, exts) = Frame::read_header(zbuf, header).unwrap();

        let c = if self.is_qos() {
            match exts.qos.as_ref() {
                Some(q) => &peer.conduit_rx[q.priority.id() as usize],
                None => &peer.conduit_rx[Priority::default().id() as usize],
            }
        } else {
            &peer.conduit_rx[0]
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
                    self.trigger_callback(msg, peer)?;
                } else if zbuf.set_pos(pos) {
                    break;
                } else {
                    bail!("decoding error");
                }
            }
        } else {
            log::debug!(
                "Transport: {}. Frame with invalid SN dropped: {}. Expected: {}.",
                self.manager.config.zid,
                sn,
                guard.sn.get()
            );
            Frame::skip_body(zbuf).unwrap();
        }

        Ok(())
    }

    fn deserialize_fragment(
        &self,
        zbuf: &mut ZBuf,
        header: u8,
        peer: &TransportMulticastPeer,
    ) -> ZResult<()> {
        let (reliability, sn, has_more, exts) = Fragment::read_header(zbuf, header).unwrap();

        let c = if self.is_qos() {
            match exts.qos.as_ref() {
                Some(q) => &peer.conduit_rx[q.priority.id() as usize],
                None => &peer.conduit_rx[Priority::default().id() as usize],
            }
        } else {
            &peer.conduit_rx[0]
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
                    zerror!(
                        "Transport: {}. Defragmentation error.",
                        self.manager.config.zid
                    )
                })?;
                self.trigger_callback(msg, peer)?;
            }
        } else {
            log::debug!(
                "Transport: {}. Fragment with invalid SN dropped: {}. Expected: {}.",
                self.manager.config.zid,
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

    pub(super) fn handle_join_from_peer(
        &self,
        join: Join,
        peer: &TransportMulticastPeer,
    ) -> ZResult<()> {
        // Check if parameters are ok
        if join.version() != peer.version
            || join.zid != peer.zid
            || join.whatami != peer.whatami
            || join.sn_bytes != peer.sn_bytes
            || join.lease != peer.lease
            || join.exts.qos.is_some() != peer.is_qos()
        {
            let e = format!(
                "Ingoring Join on {} of peer: {}. Inconsistent parameters. Version",
                peer.locator, peer.zid,
            );
            log::debug!("{}", e);
            bail!("{}", e);
        }

        Ok(())
    }

    pub(super) fn handle_join_from_unknown(&self, join: Join, locator: &Locator) -> ZResult<()> {
        if zread!(self.peers).len() >= self.manager.config.multicast.max_sessions {
            log::debug!(
                "Ingoring Join on {} from peer: {}. Max sessions reached: {}.",
                locator,
                join.zid,
                self.manager.config.multicast.max_sessions,
            );
            return Ok(());
        }

        if join.version() != self.manager.config.version {
            log::debug!(
                "Ingoring Join on {} from peer: {}. Unsupported version: {:?}. Expected: {}.",
                locator,
                join.zid,
                join.version(),
                self.manager.config.version.stable, // @TODO: handle experimental versions
            );
            return Ok(());
        }

        if join.sn_bytes > self.manager.config.sn_bytes {
            log::debug!(
                "Ingoring Join on {} from peer: {}. Unsupported SN bytes: {}. Expected: <= {}.",
                locator,
                join.zid,
                join.sn_bytes.value(),
                self.manager.config.sn_bytes.value(),
            );
            return Ok(());
        }

        if !self.manager.config.multicast.is_qos && join.exts.qos.is_some() {
            log::debug!(
                "Ingoring Join on {} from peer: {}. QoS is not supported.",
                locator,
                join.zid,
            );
            return Ok(());
        }

        let _ = self.new_peer(locator, join);

        Ok(())
    }

    pub(super) fn deserialize(&self, zbuf: &mut ZBuf, locator: &Locator) -> ZResult<()> {
        // Process the received message
        let r_guard = zread!(self.peers);
        match r_guard.get(locator) {
            Some(peer) => {
                peer.active();
                // Deserialize all the messages from the current ZBuf
                while zbuf.can_read() {
                    let header = zbuf.read().unwrap();
                    let tid = TransportId::try_from(mid(header)).unwrap();
                    match tid {
                        TransportId::Frame => self.deserialize_frame(zbuf, header, peer)?,
                        TransportId::Fragment => self.deserialize_fragment(zbuf, header, peer)?,
                        TransportId::KeepAlive => {
                            let _ka = KeepAlive::read(zbuf, header).unwrap();
                        }
                        TransportId::Join => {
                            let jn = Join::read(zbuf, header).unwrap();
                            self.handle_join_from_peer(jn, peer)?;
                        }
                        TransportId::Close => {
                            let cl = Close::read(zbuf, header).unwrap();
                            drop(r_guard);
                            self.del_peer(locator, cl.reason)?;
                            break;
                        }
                        _ => {
                            bail!("{}: decoding error", locator);
                        }
                    }
                }

                Ok(())
            }
            None => {
                drop(r_guard);

                let header = zbuf.read().unwrap();
                let tid = TransportId::try_from(mid(header)).unwrap();
                match tid {
                    TransportId::Join => {
                        let jn = Join::read(zbuf, header).unwrap();
                        self.handle_join_from_unknown(jn, locator)
                    }
                    _ => Ok(()),
                }
            }
        }
    }
}
