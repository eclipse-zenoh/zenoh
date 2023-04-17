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
use super::transport::TransportMulticastInner;
use zenoh_core::zread;
#[cfg(feature = "stats")]
use zenoh_protocol::zenoh::ZenohBody;
use zenoh_protocol::zenoh::ZenohMessage;

//noinspection ALL
impl TransportMulticastInner {
    fn schedule_on_link(&self, msg: ZenohMessage) -> bool {
        macro_rules! zpush {
            ($guard:expr, $pipeline:expr, $msg:expr) => {
                // Drop the guard before the push_zenoh_message since
                // the link could be congested and this operation could
                // block for fairly long time
                let pl = $pipeline.clone();
                drop($guard);
                return pl.push_zenoh_message($msg);
            };
        }

        let guard = zread!(self.link);
        match guard.as_ref() {
            Some(l) => {
                if let Some(pl) = l.pipeline.as_ref() {
                    zpush!(guard, pl, msg);
                }
            }
            None => {
                log::trace!(
                    "Message dropped because the transport has no links: {}",
                    msg
                );
            }
        }

        false
    }

    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(super) fn schedule_first_fit(&self, msg: ZenohMessage) -> bool {
        #[cfg(feature = "stats")]
        use zenoh_buffers::SplitBuffer;
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

        let res = self.schedule_on_link(msg);

        #[cfg(feature = "stats")]
        if res {
            self.stats.inc_tx_z_msgs(1);
        } else {
            self.stats.inc_tx_z_dropped(1);
        }

        res
    }
}
