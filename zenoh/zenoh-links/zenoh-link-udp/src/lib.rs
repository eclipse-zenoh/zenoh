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
mod multicast;
mod unicast;

use std::net::SocketAddr;

use async_std::net::ToSocketAddrs;
use async_trait::async_trait;
pub use multicast::*;
pub use unicast::*;
use zenoh_core::{bail, zconfigurable, Result as ZResult};
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol_core::{locator, Locator};

// NOTE: In case of using UDP in high-throughput scenarios, it is recommended to set the
//       UDP buffer size on the host to a reasonable size. Usually, default values for UDP buffers
//       size are undersized. Setting UDP buffers on the host to a size of 4M can be considered
//       as a safe choice.
//       Usually, on Linux systems this could be achieved by executing:
//           $ sysctl -w net.core.rmem_max=4194304
//           $ sysctl -w net.core.rmem_default=4194304

// Maximum MTU (UDP PDU) in bytes.
// NOTE: The UDP field size sets a theoretical limit of 65,535 bytes (8 byte header + 65,527 bytes of
//       data) for a UDP datagram. However the actual limit for the data length, which is imposed by
//       the underlying IPv4 protocol, is 65,507 bytes (65,535 − 8 byte UDP header − 20 byte IP header).
//       Although in IPv6 it is possible to have UDP datagrams of size greater than 65,535 bytes via
//       IPv6 Jumbograms, its usage in Zenoh is discouraged unless the consequences are very well
//       understood.
const UDP_MAX_MTU: u16 = 65_507;

pub const UDP_LOCATOR_PREFIX: &str = "udp";

#[cfg(any(target_os = "linux", target_os = "windows"))]
// Linux default value of a maximum datagram size is set to UDP MAX MTU.
const UDP_MTU_LIMIT: u16 = UDP_MAX_MTU;

#[cfg(target_os = "macos")]
// Mac OS X default value of a maximum datagram size is set to 9216 bytes.
const UDP_MTU_LIMIT: u16 = 9_216;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const UDP_MTU_LIMIT: u16 = 8_192;

zconfigurable! {
    // Default MTU (UDP PDU) in bytes.
    static ref UDP_DEFAULT_MTU: u16 = UDP_MTU_LIMIT;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref UDP_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[derive(Default, Clone, Copy)]
pub struct UdpLocatorInspector;
#[async_trait]
impl LocatorInspector for UdpLocatorInspector {
    fn protocol(&self) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(UDP_LOCATOR_PREFIX)
    }
    async fn is_multicast(&self, locator: &locator) -> ZResult<bool> {
        Ok(get_udp_addr(locator).await?.ip().is_multicast())
    }
}

pub mod config {
    pub const UDP_MULTICAST_SRC_IFACE: &str = "src_iface";
}

pub(crate) async fn get_udp_addr(locator: &locator) -> ZResult<SocketAddr> {
    let addr = locator.address();
    let mut addrs = addr.to_socket_addrs().await?;
    if let Some(addr) = addrs.next() {
        Ok(addr)
    } else {
        bail!("Couldn't resolve UDP locator address: {}", addr);
    }
}

pub(crate) fn socket_addr_to_udp_locator(addr: &SocketAddr) -> Locator {
    Locator::try_from(format!("udp/{}", addr)).unwrap()
}
