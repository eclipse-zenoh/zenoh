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
use super::transport::TransportUnicastLowlatency;
use zenoh_protocol::{
    network::NetworkMessage,
    transport::{TransportBodyLowLatency, TransportMessageLowLatency},
};
#[cfg(feature = "shared-memory")]
use zenoh_result::bail;
use zenoh_result::ZResult;

impl TransportUnicastLowlatency {
    #[allow(unused_mut)] // When feature "shared-memory" is not enabled
    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(crate) fn internal_schedule(&self, mut msg: NetworkMessage) -> ZResult<()> {
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

        let msg = TransportMessageLowLatency {
            body: TransportBodyLowLatency::Network(msg),
        };
        let res = self.send(msg);

        #[cfg(feature = "stats")]
        if res.is_ok() {
            self.stats.inc_tx_n_msgs(1);
        } else {
            self.stats.inc_tx_n_dropped(1);
        }

        res
    }
}
