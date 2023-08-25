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
use super::transport::TransportUnicastShm;
use zenoh_buffers::{
    reader::{HasReader, Reader},
    ZSlice,
};
use zenoh_codec::{RCodec, Zenoh080};
use zenoh_core::zread;
use zenoh_link::LinkUnicast;
use zenoh_protocol::{network::NetworkMessage, transport::TransportMessageShm};
use zenoh_result::{zerror, ZResult};

/*************************************/
/*            TRANSPORT RX           */
/*************************************/
impl TransportUnicastShm {
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
                    crate::shm::map_zmsg_to_shmbuf(&mut msg, &self.manager.shm().reader)?;
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

    pub(super) async fn read_messages(
        &self,
        mut zslice: ZSlice,
        link: &LinkUnicast,
    ) -> ZResult<()> {
        let codec = Zenoh080::new();
        let mut reader = zslice.reader();
        while reader.can_read() {
            let msg: TransportMessageShm = codec
                .read(&mut reader)
                .map_err(|_| zerror!("{}: decoding error", link))?;

            log::trace!("Received: {:?}", msg);

            // #[cfg(feature = "stats")]
            // {
            //     transport.stats.inc_rx_t_msgs(1);
            // }

            match msg.body {
                zenoh_protocol::transport::TransportBodyShm::Close(_) => {
                    let _ = self.delete().await;
                }
                zenoh_protocol::transport::TransportBodyShm::KeepAlive(_) => {}
                zenoh_protocol::transport::TransportBodyShm::Network(msg) => {
                    let _ = self.trigger_callback(*msg);
                }
            }
        }
        Ok(())
    }
}
