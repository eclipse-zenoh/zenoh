use crate::{
    shm_unicast::oam_extensions::unpack_oam_close,
    transport_unicast_inner::TransportUnicastInnerTrait,
};

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
use super::transport::ShmTransportUnicastInner;
use async_std::task;
#[cfg(feature = "stats")]
use zenoh_buffers::SplitBuffer;
use zenoh_buffers::{
    reader::{HasReader, Reader},
    ZSlice,
};
use zenoh_codec::{RCodec, Zenoh080};
use zenoh_core::zread;
use zenoh_link::LinkUnicast;
#[cfg(feature = "stats")]
use zenoh_protocol::zenoh::ZenohBody;
use zenoh_protocol::{
    network::{
        oam::id::{OAM_CLOSE, OAM_KEEPALIVE},
        NetworkMessage,
    },
    transport::Close,
};
use zenoh_result::{zerror, ZResult};

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
impl ShmTransportUnicastInner {
    fn trigger_callback(
        &self,
        #[allow(unused_mut)] // shared-memory feature requires mut
        mut msg: NetworkMessage,
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
                // todo: need to re-engineer this!
                //if self.config.is_shm {
                //    crate::shm::map_zmsg_to_shmbuf(
                //        &mut msg,
                //        &self.manager.shm().reader,
                //    )?;
                //}
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

    fn handle_close(&self, link: &LinkUnicast, close: Close) -> ZResult<()> {
        // Stop now rx and tx tasks before doing the proper cleanup
        let _ = self.stop_rx(link);
        let _ = self.stop_tx(link);

        // Delete and clean up
        let c_transport = self.clone();
        let c_link = link.clone();
        // Spawn a task to avoid a deadlock waiting for this same task
        // to finish in the link close() joining the rx handle
        task::spawn(async move {
            if close.session {
                let _ = c_transport.delete().await;
            } else {
                let _ = c_transport.del_link(&c_link).await;
            }
        });

        Ok(())
    }

    pub(super) fn read_messages(&self, mut zslice: ZSlice, link: &LinkUnicast) -> ZResult<()> {
        let codec = Zenoh080::new();
        let mut reader = zslice.reader();
        while reader.can_read() {
            let msg: NetworkMessage = codec
                .read(&mut reader)
                .map_err(|_| zerror!("{}: decoding error", link))?;

            log::trace!("Received: {:?}", msg);

            #[cfg(feature = "stats")]
            {
                transport.stats.inc_rx_t_msgs(1);
            }

            match msg.body {
                zenoh_protocol::network::NetworkBody::OAM(ref oam) => match oam.id {
                    OAM_KEEPALIVE => {}
                    OAM_CLOSE => {
                        let oam_close = unpack_oam_close(oam)?;
                        self.handle_close(link, oam_close)?;
                    }
                    _ => {
                        self.trigger_callback(msg)?;
                    }
                },
                _ => {
                    self.trigger_callback(msg)?;
                }
            }
        }
        Ok(())
    }
}
