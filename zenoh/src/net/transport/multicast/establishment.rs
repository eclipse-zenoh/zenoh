//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::super::TransportManager;
use super::{TransportMulticast, TransportMulticastConfig, TransportMulticastInner};
use crate::net::link::LinkMulticast;
use rand::Rng;
use std::sync::Arc;
use zenoh_util::core::ZResult;
use zenoh_util::zasynclock;

pub(crate) async fn open_link(
    manager: &TransportManager,
    link: LinkMulticast,
) -> ZResult<TransportMulticast> {
    let locator = link.get_dst();

    // Create and configure the multicast transport
    let config = TransportMulticastConfig {
        manager: manager.clone(),
        pid: manager.config.pid.clone(),
        whatami: manager.config.whatami,
        sn_resolution: manager.config.sn_resolution,
        initial_sn_tx: zasynclock!(manager.prng).gen_range(0..manager.config.sn_resolution),
        is_shm: false, // @TODO: allow dynamic configuration
        is_qos: true,  // @TODO: allow dynamic configuration
        link,
    };
    let ti = Arc::new(TransportMulticastInner::new(config));

    // Store the active transport
    let transport: TransportMulticast = (&ti).into();
    zlock!(manager.state.multicast.transports).insert(locator.clone(), ti.clone());

    // Notify the transport event handler
    ti.start_tx(manager.config.batch_size).map_err(|e| {
        zlock!(manager.state.multicast.transports).remove(&locator);
        let _ = ti.stop_tx();
        e
    })?;
    let callback = manager
        .config
        .handler
        .new_multicast(transport.clone())
        .map_err(|e| {
            zlock!(manager.state.multicast.transports).remove(&locator);
            let _ = ti.stop_tx();
            e
        })?;
    ti.set_callback(callback);
    ti.start_rx().map_err(|e| {
        zlock!(manager.state.multicast.transports).remove(&locator);
        let _ = ti.stop_rx();
        e
    })?;

    Ok(transport)
}
