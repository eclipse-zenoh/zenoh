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
use zenoh_core::zread;
use zenoh_protocol::network::NetworkMessage;
use zenoh_result::ZResult;

use super::transport::TransportMulticastInner;
#[cfg(feature = "shared-memory")]
use crate::shm::map_zmsg_to_partner;

//noinspection ALL
impl TransportMulticastInner {
    fn schedule_on_link(&self, msg: NetworkMessage) -> ZResult<bool> {
        macro_rules! zpush {
            ($guard:expr, $pipeline:expr, $msg:expr) => {
                // Drop the guard before the push_zenoh_message since
                // the link could be congested and this operation could
                // block for fairly long time
                let pl = $pipeline.clone();
                drop($guard);
                return Ok(pl.push_network_message($msg)?);
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
                tracing::trace!(
                    "Message dropped because the transport has no links: {}",
                    msg
                );
            }
        }

        Ok(false)
    }

    #[allow(unused_mut)] // When feature "shared-memory" is not enabled
    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(super) fn schedule(&self, mut msg: NetworkMessage) -> ZResult<bool> {
        #[cfg(feature = "shared-memory")]
        {
            if let Err(e) = map_zmsg_to_partner(&mut msg, &self.shm) {
                tracing::trace!("Failed SHM conversion: {}", e);
                return Ok(false);
            }
        }

        let res = self.schedule_on_link(msg)?;

        #[cfg(feature = "stats")]
        if res {
            self.stats.inc_tx_n_msgs(1);
        } else {
            self.stats.inc_tx_n_dropped(1);
        }

        Ok(res)
    }
}
