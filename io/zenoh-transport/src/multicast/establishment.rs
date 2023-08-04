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
use crate::{
    common::seq_num,
    multicast::{transport::TransportMulticastInner, TransportMulticast},
    TransportConfigMulticast, TransportManager,
};
use rand::Rng;
use std::sync::Arc;
use zenoh_core::zasynclock;
use zenoh_link::LinkMulticast;
use zenoh_protocol::{
    core::{Field, Priority},
    transport::PrioritySn,
};
use zenoh_result::{bail, ZResult};

pub(crate) async fn open_link(
    manager: &TransportManager,
    link: LinkMulticast,
) -> ZResult<TransportMulticast> {
    // Create and configure the multicast transport
    let mut prng = zasynclock!(manager.prng);

    // Generate initial SNs
    let sn_resolution = manager.config.resolution.get(Field::FrameSN);
    let max = seq_num::get_mask(sn_resolution);
    macro_rules! zgen_prioritysn {
        () => {
            PrioritySn {
                reliable: prng.gen_range(0..=max),
                best_effort: prng.gen_range(0..=max),
            }
        };
    }

    let initial_sns = if manager.config.multicast.is_qos {
        (0..Priority::NUM).map(|_| zgen_prioritysn!()).collect()
    } else {
        vec![zgen_prioritysn!()]
    }
    .into_boxed_slice();

    // Release the lock
    drop(prng);

    // Create the transport
    let locator = link.get_dst().to_owned();
    let config = TransportConfigMulticast {
        link,
        sn_resolution,
        initial_sns,
        #[cfg(feature = "shared-memory")]
        is_shm: manager.config.multicast.is_shm,
    };
    let ti = Arc::new(TransportMulticastInner::make(manager.clone(), config)?);

    // Add the link
    ti.start_tx()?;

    // Store the active transport
    let mut w_guard = zasynclock!(manager.state.multicast.transports);
    if w_guard.get(&locator).is_some() {
        bail!("A Multicast transport on {} already exist!", locator);
    }
    w_guard.insert(locator.clone(), ti.clone());
    drop(w_guard);

    // Notify the transport event handler
    let transport: TransportMulticast = (&ti).into();

    let callback = match manager.config.handler.new_multicast(transport.clone()) {
        Ok(c) => c,
        Err(e) => {
            zasynclock!(manager.state.multicast.transports).remove(&locator);
            let _ = ti.stop_tx();
            return Err(e);
        }
    };
    ti.set_callback(callback);
    if let Err(e) = ti.start_rx() {
        zasynclock!(manager.state.multicast.transports).remove(&locator);
        let _ = ti.stop_rx();
        return Err(e);
    }

    Ok(transport)
}
