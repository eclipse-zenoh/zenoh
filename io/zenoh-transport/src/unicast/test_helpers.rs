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
use zenoh_core::zcondfeat;

use crate::{unicast::TransportManagerBuilderUnicast, TransportManager};

pub fn make_transport_manager_builder(
    #[cfg(feature = "transport_multilink")] max_links: usize,
    lowlatency_transport: bool,
) -> TransportManagerBuilderUnicast {
    let transport = make_basic_transport_manager_builder(lowlatency_transport);

    zcondfeat!(
        "transport_multilink",
        {
            println!("...with max links: {max_links}...");
            transport.max_links(max_links)
        },
        transport
    )
}

pub fn make_basic_transport_manager_builder(
    lowlatency_transport: bool,
) -> TransportManagerBuilderUnicast {
    println!("Create transport manager builder...");
    let config = TransportManager::config_unicast();
    if lowlatency_transport {
        println!("...with LowLatency transport...");
    }
    match lowlatency_transport {
        true => config.lowlatency(true).qos(false),
        false => config,
    }
}
