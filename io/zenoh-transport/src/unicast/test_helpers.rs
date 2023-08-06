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

use crate::{TransportManagerBuilderUnicast, TransportManager};

pub fn make_transport_builder(
    max_links: usize,
    max_sessions: usize,
    #[cfg(feature = "shared-memory")] shm_transport: bool,
) -> TransportManagerBuilderUnicast {
    #[allow(unused_mut)] // unused mut when shared-memory is disabled
    let mut transport = TransportManager::config_unicast()
        .max_links(max_links)
        .max_sessions(max_sessions);
    #[cfg(feature = "shared-memory")]
    {
        transport = transport.shm(shm_transport);
    }
    transport
}
