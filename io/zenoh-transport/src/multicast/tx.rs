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
use zenoh_protocol::network::{NetworkMessageExt, NetworkMessageMut, NetworkMessageRef};
use zenoh_result::ZResult;

use super::transport::TransportMulticastInner;
#[cfg(feature = "shared-memory")]
use crate::shm::map_zmsg_to_partner;

//noinspection ALL
impl TransportMulticastInner {
    fn schedule_on_link(&self, msg: NetworkMessageRef) -> ZResult<bool> {
        let guard = zread!(self.link);
        match guard.as_ref() {
            Some(l) => {
                if let Some(pl) = l.pipeline.as_ref() {
                    let pl = pl.clone();
                    drop(guard);
                    return Ok(pl.push_network_message(msg)?);
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
    pub(super) fn schedule(&self, mut msg: NetworkMessageMut) -> ZResult<bool> {
        #[cfg(feature = "shared-memory")]
        if let Some(shm_context) = &self.shm_context {
            map_zmsg_to_partner(&mut msg, &shm_context.shm_config, &shm_context.shm_provider);
        }

        let res = self.schedule_on_link(msg.as_ref())?;

        #[cfg(feature = "stats")]
        if res {
            self.stats.tx_n_msgs.inc_net(1);
        } else {
            self.stats.inc_tx_n_dropped(1);
        }

        Ok(res)
    }
}
