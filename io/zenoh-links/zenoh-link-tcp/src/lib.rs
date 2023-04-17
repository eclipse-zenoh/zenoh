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
//! [Click here for Zenoh's documentation](../zenoh/index.html)
use async_std::net::ToSocketAddrs;
use async_trait::async_trait;
use std::net::SocketAddr;
use zenoh_core::zconfigurable;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::core::{endpoint::Address, Locator};
use zenoh_result::{zerror, ZResult};

mod unicast;
pub use unicast::*;

// Default MTU (TCP PDU) in bytes.
// NOTE: Since TCP is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the TCP MTU is constrained to
//       2^16 - 1 bytes (i.e., 65535).
const TCP_MAX_MTU: u16 = u16::MAX;

pub const TCP_LOCATOR_PREFIX: &str = "tcp";

#[derive(Default, Clone, Copy)]
pub struct TcpLocatorInspector;
#[async_trait]
impl LocatorInspector for TcpLocatorInspector {
    fn protocol(&self) -> &str {
        TCP_LOCATOR_PREFIX
    }

    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }
}

zconfigurable! {
    // Default MTU (TCP PDU) in bytes.
    static ref TCP_DEFAULT_MTU: u16 = TCP_MAX_MTU;
    // The LINGER option causes the shutdown() call to block until (1) all application data is delivered
    // to the remote end or (2) a timeout expires. The timeout is expressed in seconds.
    // More info on the LINGER option and its dynamics can be found at:
    // https://blog.netherlabs.nl/articles/2009/01/18/the-ultimate-so_linger-page-or-why-is-my-tcp-not-reliable
    static ref TCP_LINGER_TIMEOUT: i32 = 10;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref TCP_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

pub async fn get_tcp_addrs(address: Address<'_>) -> ZResult<impl Iterator<Item = SocketAddr>> {
    let iter = address
        .as_str()
        .to_socket_addrs()
        .await
        .map_err(|e| zerror!("{}", e))?
        .filter(|x| !x.ip().is_multicast());
    Ok(iter)
}
