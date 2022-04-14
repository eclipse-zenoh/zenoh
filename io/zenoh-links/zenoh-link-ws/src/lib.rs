//
// Copyright (c) 2022 ZettaScale Technology
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

use async_trait::async_trait;
use std::{net::SocketAddr, str::FromStr};
use zenoh_link_commons::LocatorInspector;
use zenoh_core::{bail, zconfigurable, Result as ZResult};
use zenoh_protocol_core::Locator;
use url::Url;
mod unicast;
pub use unicast::*;

// Default MTU (WSS PDU) in bytes.
// NOTE: Since TCP is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the TCP MTU is constrained to
//       2^16 - 1 bytes (i.e., 65535).
const WS_MAX_MTU: u16 = u16::MAX;

pub const WS_LOCATOR_PREFIX: &str = "ws";

#[derive(Default, Clone, Copy)]
pub struct WsLocatorInspector;
#[async_trait]
impl LocatorInspector for WsLocatorInspector {
    fn protocol(&self) -> &str {
        WS_LOCATOR_PREFIX
    }
    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }
}

zconfigurable! {
    // Default MTU (TCP PDU) in bytes.
    static ref WS_DEFAULT_MTU: u16 = WS_MAX_MTU;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref TCP_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

pub fn get_ws_addr(address: &Locator) -> ZResult<SocketAddr> {
    match SocketAddr::from_str(address.address()) {
        Ok(address) => Ok(address),
        Err(e) => bail!("Couldn't resolve WebSocket locator address: {}: {}", address, e),
    }
}


pub fn get_ws_url(address: &Locator) -> ZResult<Url> {
    match  Url::parse(&format!("{}://{}", address.protocol(), address.address())) {
        Ok(url) => Ok(url),
        Err(e) => bail!("Couldn't resolve WebSocket locator address: {}: {}", address, e),
    }
}