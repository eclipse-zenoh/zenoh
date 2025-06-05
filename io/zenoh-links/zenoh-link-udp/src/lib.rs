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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
mod multicast;
mod pktinfo;
mod unicast;

use std::{net::SocketAddr, str::FromStr};

use async_trait::async_trait;
pub use multicast::*;
pub use unicast::*;
use zenoh_core::zconfigurable;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::{
    core::{endpoint::Address, Locator, Metadata, Reliability},
    transport::BatchSize,
};
use zenoh_result::{zerror, ZResult};

// NOTE: In case of using UDP in high-throughput scenarios, it is recommended to set the
//       UDP buffer size on the host to a reasonable size. Usually, default values for UDP buffers
//       size are undersized. Setting UDP buffers on the host to a size of 4M can be considered
//       as a safe choice.
//       Usually, on Linux systems this could be achieved by executing:
//           $ sysctl -w net.core.rmem_max=4194304
//           $ sysctl -w net.core.rmem_default=4194304

/// Maximum MTU (UDP PDU) in bytes.
///
/// # Note
///
/// The theoretical Maximum Transmission Unit (MTU) of UDP is `u16::MAX`. From that we subtract the
/// size of a UDP header (8 bytes) and the size of IPv4/IPv6 headers (resp. 20 and 40 bytes).
///
/// Although in IPv6 it is possible to have UDP datagrams of size greater than 65,535 bytes via IPv6
/// Jumbograms, its usage in Zenoh is discouraged unless the consequences are very well understood.
const UDP_MAX_MTU: BatchSize = u16::MAX - 8 - 40;

pub const UDP_LOCATOR_PREFIX: &str = "udp";

#[cfg(any(target_os = "linux", target_os = "windows"))]
// Linux default value of a maximum datagram size is set to UDP MAX MTU.
const UDP_MTU_LIMIT: BatchSize = UDP_MAX_MTU;

#[cfg(target_os = "macos")]
// Mac OS X default value of a maximum datagram size is set to 9216 bytes.
const UDP_MTU_LIMIT: BatchSize = 9_216;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const UDP_MTU_LIMIT: BatchSize = 8_192;

const IS_RELIABLE: bool = false;

zconfigurable! {
    // Default MTU (UDP PDU) in bytes.
    static ref UDP_DEFAULT_MTU: BatchSize = UDP_MTU_LIMIT;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref UDP_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[derive(Default, Clone, Copy)]
pub struct UdpLocatorInspector;
#[async_trait]
impl LocatorInspector for UdpLocatorInspector {
    fn protocol(&self) -> &str {
        UDP_LOCATOR_PREFIX
    }

    async fn is_multicast(&self, locator: &Locator) -> ZResult<bool> {
        let is_multicast = get_udp_addrs(locator.address())
            .await?
            .any(|x| x.ip().is_multicast());
        Ok(is_multicast)
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

pub mod config {
    pub const UDP_MULTICAST_IFACE: &str = "iface";
    pub const UDP_MULTICAST_JOIN: &str = "join";
    pub const UDP_MULTICAST_TTL: &str = "ttl";
}

pub async fn get_udp_addrs(address: Address<'_>) -> ZResult<impl Iterator<Item = SocketAddr>> {
    let iter = tokio::net::lookup_host(address.as_str().to_string())
        .await
        .map_err(|e| zerror!("{}", e))?;
    Ok(iter)
}

pub(crate) fn socket_addr_to_udp_locator(addr: &SocketAddr) -> Locator {
    Locator::new(UDP_LOCATOR_PREFIX, addr.to_string(), "").unwrap()
}
