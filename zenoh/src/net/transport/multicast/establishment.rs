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
use super::protocol::core::{ConduitSn, ConduitSnList, Priority};
use super::{TransportMulticast, TransportMulticastConfig, TransportMulticastInner};
use crate::net::link::LinkMulticast;
use rand::Rng;
use std::sync::Arc;
use zenoh_util::core::Result as ZResult;
use zenoh_util::zasynclock;

pub(crate) async fn open_link(
    manager: &TransportManager,
    link: LinkMulticast,
) -> ZResult<TransportMulticast> {
    // Create and configure the multicast transport
    let mut prng = zasynclock!(manager.prng);

    let sn_resolution = manager.config.sn_bytes.resolution();

    macro_rules! zgen_conduit_sn {
        () => {
            ConduitSn {
                reliable: prng.gen_range(0..sn_resolution),
                best_effort: prng.gen_range(0..sn_resolution),
            }
        };
    }

    let locator = link.get_dst();
    let initial_sns = if manager.config.multicast.is_qos {
        let mut initial_sns = [ConduitSn::default(); Priority::NUM];
        for isn in initial_sns.iter_mut() {
            *isn = zgen_conduit_sn!();
        }
        ConduitSnList::QoS(initial_sns)
    } else {
        ConduitSnList::Plain(zgen_conduit_sn!())
    };
    let config = TransportMulticastConfig {
        manager: manager.clone(),
        initial_sns,
        link: link.clone(),
    };
    let ti = Arc::new(TransportMulticastInner::make(config)?);

    // Store the active transport
    let transport: TransportMulticast = (&ti).into();
    zlock!(manager.state.multicast.transports).insert(locator.clone(), ti.clone());

    // Notify the transport event handler
    let batch_size = manager.config.batch_size.min(link.get_mtu());
    ti.start_tx(batch_size).map_err(|e| {
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
