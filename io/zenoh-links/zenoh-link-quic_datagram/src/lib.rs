//
// Copyright (c) 2025 ZettaScale Technology
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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use std::{str::FromStr, sync::Arc, time::Duration};

use async_trait::async_trait;
use zenoh_core::{zconfigurable, zerror};
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::{
    core::{Config, Locator, Metadata, Reliability},
    transport::BatchSize,
};
use zenoh_result::ZResult;

mod unicast;
pub use unicast::*;
pub use zenoh_link_commons::quic::TlsConfigurator as QuicDatagramConfigurator;

pub const QUIC_DATAGRAM_LOCATOR_PREFIX: &str = "quic";

// NOTE: this was copied from `zenoh-link-udp`
#[cfg(any(target_os = "linux", target_os = "windows"))]
const QUIC_DATAGRAM_MAX_MTU: BatchSize = u16::MAX - 8 - 40;
#[cfg(target_os = "macos")]
const QUIC_DATAGRAM_MAX_MTU: BatchSize = 9_216;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const QUIC_DATAGRAM_MAX_MTU: BatchSize = 8_192;

const IS_RELIABLE: bool = false;

#[derive(Default, Clone, Copy, Debug)]
pub struct QuicDatagramLocatorInspector;

#[async_trait]
impl LocatorInspector for QuicDatagramLocatorInspector {
    fn protocol(&self) -> &str {
        QUIC_DATAGRAM_LOCATOR_PREFIX
    }

    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }

    fn is_reliable(&self, locator: &Locator) -> ZResult<bool> {
        if let Some(reliability) = locator
            .metadata()
            .get(Metadata::RELIABILITY)
            .map(Reliability::from_str)
            .transpose()?
        {
            Ok(reliability == Reliability::Reliable)
        } else {
            Ok(IS_RELIABLE)
        }
    }
}

zconfigurable! {
    // Default MTU (QUIC PDU) in bytes.
    static ref QUIC_DATAGRAM_DEFAULT_MTU: BatchSize = QUIC_DATAGRAM_MAX_MTU;
    // The LINGER option causes the shutdown() call to block until (1) all application data is delivered
    // to the remote end or (2) a timeout expires. The timeout is expressed in seconds.
    // More info on the LINGER option and its dynamics can be found at:
    // https://blog.netherlabs.nl/articles/2009/01/18/the-ultimate-so_linger-page-or-why-is-my-tcp-not-reliable
    static ref QUIC_DATAGRAM_LINGER_TIMEOUT: i32 = 10;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref QUIC_DATAGRAM_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

pub(crate) fn set_mtu_config(
    config: Config,
    server_config: &mut quinn::ServerConfig,
) -> ZResult<()> {
    if let Some(initial_mtu) = config.get("initial_mtu") {
        let initial_mtu = initial_mtu.parse::<u16>().map_err(|err| {
            zerror!(
                "could not parse QUIC Datagram endpoint's initial_mtu value `{initial_mtu}`: {err}"
            )
        })?;
        Arc::get_mut(&mut server_config.transport)
            .unwrap()
            .initial_mtu(initial_mtu);
    }

    if let Some(mtu_discovery_interval_secs) = config.get("mtu_discovery_interval_secs") {
        let mtu_discovery_interval = mtu_discovery_interval_secs
            .parse::<u64>()
            .map_err(|err| {
                zerror!(
                    "could not parse QUIC Datagram endpoint's mtu_discovery_interval_secs value `{mtu_discovery_interval_secs}`: {err}"
                )
            })
            .map(Duration::from_secs)?;
        let mut mtu_discovery_config = quinn::MtuDiscoveryConfig::default();
        mtu_discovery_config.interval(mtu_discovery_interval);
        Arc::get_mut(&mut server_config.transport)
            .unwrap()
            .mtu_discovery_config(Some(mtu_discovery_config));
    }

    Ok(())
}
