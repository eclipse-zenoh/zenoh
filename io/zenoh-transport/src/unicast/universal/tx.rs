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
use super::transport::TransportUnicastUniversal;
use zenoh_core::zread;
use zenoh_protocol::network::NetworkMessage;

impl TransportUnicastUniversal {
    fn schedule_on_link(&self, msg: NetworkMessage) -> bool {
        macro_rules! zpush {
            ($guard:expr, $pipeline:expr, $msg:expr) => {
                // Drop the guard before the push_zenoh_message since
                // the link could be congested and this operation could
                // block for fairly long time
                let pl = $pipeline.clone();
                drop($guard);
                log::trace!("Scheduled: {:?}", $msg);
                return pl.push_network_message($msg);
            };
        }

        let guard = zread!(self.links);
        // First try to find the best match between msg and link reliability
        if let Some(pl) = guard.iter().find_map(|tl| {
            if msg.is_reliable() == tl.link.link.is_reliable() {
                Some(&tl.pipeline)
            } else {
                None
            }
        }) {
            zpush!(guard, pl, msg);
        }

        // No best match found, take the first available link
        if let Some(pl) = guard.iter().map(|tl| &tl.pipeline).next() {
            zpush!(guard, pl, msg);
        }

        // No Link found
        log::trace!(
            "Message dropped because the transport has no links: {}",
            msg
        );

        false
    }

    #[allow(unused_mut)] // When feature "shared-memory" is not enabled
    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(crate) fn internal_schedule(&self, mut msg: NetworkMessage) -> bool {
        #[cfg(feature = "shared-memory")]
        {
            let res = if self.config.is_shm {
                crate::shm::map_zmsg_to_shminfo(&mut msg)
            } else {
                crate::shm::map_zmsg_to_shmbuf(&mut msg, &self.manager.shm().reader)
            };
            if let Err(e) = res {
                log::trace!("Failed SHM conversion: {}", e);
                return false;
            }
        }

        let res = self.schedule_on_link(msg);

        #[cfg(feature = "stats")]
        if res {
            self.stats.inc_tx_n_msgs(1);
        } else {
            self.stats.inc_tx_n_dropped(1);
        }

        res
    }
}
