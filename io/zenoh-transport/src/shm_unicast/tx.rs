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
#[cfg(feature = "stats")]
use zenoh_buffers::SplitBuffer;
use zenoh_core::zasyncread;
use zenoh_protocol::network::NetworkMessage;
#[cfg(feature = "stats")]
use zenoh_protocol::zenoh::ZenohBody;
use zenoh_result::{bail, ZResult};

impl ShmTransportUnicastInner {
    async fn schedule_on_link(&self, msg: NetworkMessage) -> ZResult<()> {
        let guard = zasyncread!(self.link);
        match guard.as_ref() {
            Some(l) => l.send(msg).await,
            None => {
                // No Link found
                log::trace!("Message dropped because the transport has no link: {}", msg);
                bail!("Message dropped because the transport has no link: {}", msg)
            }
        }
    }

    #[allow(unused_mut)] // When feature "shared-memory" is not enabled
    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(crate) async fn internal_schedule(&self, mut msg: NetworkMessage) -> ZResult<()> {
        #[cfg(feature = "shared-memory")]
        {
            let res = if self.config.is_shm {
                crate::shm::map_zmsg_to_shminfo(&mut msg)
            } else {
                crate::shm::map_zmsg_to_shmbuf(&mut msg, &self.manager.shm().reader)
            };
            if let Err(e) = res {
                bail!("Failed SHM conversion: {}", e);
            }
        }

        #[cfg(feature = "stats")]
        match &msg.body {
            ZenohBody::Data(data) => match data.reply_context {
                Some(_) => {
                    self.stats.inc_tx_z_data_reply_msgs(1);
                    self.stats
                        .inc_tx_z_data_reply_payload_bytes(data.payload.len());
                }
                None => {
                    self.stats.inc_tx_z_data_msgs(1);
                    self.stats.inc_tx_z_data_payload_bytes(data.payload.len());
                }
            },
            ZenohBody::Unit(unit) => match unit.reply_context {
                Some(_) => self.stats.inc_tx_z_unit_reply_msgs(1),
                None => self.stats.inc_tx_z_unit_msgs(1),
            },
            ZenohBody::Pull(_) => self.stats.inc_tx_z_pull_msgs(1),
            ZenohBody::Query(_) => self.stats.inc_tx_z_query_msgs(1),
            ZenohBody::Declare(_) => self.stats.inc_tx_z_declare_msgs(1),
            ZenohBody::LinkStateList(_) => self.stats.inc_tx_z_linkstate_msgs(1),
        }

        let res = self.schedule_on_link(msg).await;

        #[cfg(feature = "stats")]
        if res {
            self.stats.inc_tx_z_msgs(1);
        } else {
            self.stats.inc_tx_z_dropped(1);
        }

        res
    }
}
