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
use zenoh_protocol::{
    network::{NetworkMessageExt, NetworkMessageMut},
    transport::{TransportBodyLowLatencyRef, TransportMessageLowLatencyRef},
};
use zenoh_result::ZResult;

use super::transport::TransportUnicastLowlatency;
#[cfg(feature = "shared-memory")]
use crate::shm::{collect_shm_bufs, map_zmsg_to_partner, PendingShmBuf, SHM_PENDING_TTL};
#[cfg(feature = "shared-memory")]
use std::time::Instant;

impl TransportUnicastLowlatency {
    #[allow(unused_mut)] // When feature "shared-memory" is not enabled
    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(crate) fn internal_schedule(&self, mut msg: NetworkMessageMut) -> ZResult<()> {
        #[cfg(feature = "shared-memory")]
        if let Some(shm_context) = &self.shm_context {
            map_zmsg_to_partner(&mut msg, &shm_context.shm_config, &shm_context.shm_provider);
        }

        // Collect before msg is re-bound as NetworkMessageRef.
        #[cfg(feature = "shared-memory")]
        let shm_bufs = collect_shm_bufs(&msg);

        let msg = msg.as_ref();
        let tmsg = TransportMessageLowLatencyRef {
            body: TransportBodyLowLatencyRef::Network(msg),
        };
        let res = self.send(tmsg);

        #[cfg(feature = "shared-memory")]
        if res.is_ok() && !shm_bufs.is_empty() {
            let now = Instant::now();
            let deadline = now + SHM_PENDING_TTL;
            let mut pending = self.shm_pending.lock().expect("shm_pending lock");
            pending.retain(|_, v| !v.buf.is_rx_acked() && v.deadline > now);
            for buf in shm_bufs {
                let key = buf.info.metadata.clone();
                pending.insert(key, PendingShmBuf { buf, deadline });
            }
        }

        #[cfg(feature = "stats")]
        if res.is_ok() {
            self.link_stats
                .get()
                .unwrap()
                .inc_network_message(zenoh_stats::Rx, msg);
        }

        res
    }
}
