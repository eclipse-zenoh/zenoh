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
#[cfg(feature = "shared-memory")]
use zenoh_result::bail;
use zenoh_result::ZResult;

use super::transport::TransportUnicastLowlatency;
#[cfg(feature = "shared-memory")]
use crate::shm::map_zmsg_to_partner;

impl TransportUnicastLowlatency {
    #[allow(unused_mut)] // When feature "shared-memory" is not enabled
    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(crate) fn internal_schedule(&self, mut msg: NetworkMessageMut) -> ZResult<()> {
        #[cfg(feature = "shared-memory")]
        {
            if let Err(e) = map_zmsg_to_partner(&mut msg, &self.config.shm) {
                bail!("Failed SHM conversion: {}", e);
            }
        }

        let msg = TransportMessageLowLatencyRef {
            body: TransportBodyLowLatencyRef::Network(msg.as_ref()),
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
