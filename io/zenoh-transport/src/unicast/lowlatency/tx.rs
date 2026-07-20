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
use zenoh_protocol::network::NetworkMessageMut;
use zenoh_result::ZResult;

use super::transport::TransportUnicastLowlatency;

impl TransportUnicastLowlatency {
    #[allow(unused_mut)] // When feature "shared-memory" is not enabled
    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(crate) fn internal_schedule(&self, mut msg: NetworkMessageMut) -> ZResult<()> {
        let res = self.send(msg);

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
