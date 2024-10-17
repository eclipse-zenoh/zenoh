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
use std::{net::SocketAddr, str::FromStr};

use async_trait::async_trait;
use url::Url;
use zenoh_core::zconfigurable;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::{
    core::{endpoint::Address, Locator, Metadata, Reliability},
    transport::BatchSize,
};
use zenoh_result::{bail, ZResult};
mod unicast;
pub use unicast::*;

// Default MTU (WSS PDU) in bytes.
// NOTE: Since TCP is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the TCP MTU is constrained to
//       2^16 - 1 bytes (i.e., 65535).
const WS_MAX_MTU: BatchSize = BatchSize::MAX;

pub const WS_LOCATOR_PREFIX: &str = "ws";

const IS_RELIABLE: bool = true;

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
    // Default MTU (TCP PDU) in bytes.
    static ref WS_DEFAULT_MTU: BatchSize = WS_MAX_MTU;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref TCP_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

pub async fn get_ws_addr(address: Address<'_>) -> ZResult<SocketAddr> {
    match tokio::net::lookup_host(address.as_str()).await?.next() {
        Some(addr) => Ok(addr),
        None => bail!("Couldn't resolve WebSocket locator address: {}", address),
    }
}

pub async fn get_ws_url(address: Address<'_>) -> ZResult<Url> {
    match Url::parse(&format!(
        "{}://{}",
        WS_LOCATOR_PREFIX,
        get_ws_addr(address).await?
    )) {
        Ok(url) => Ok(url),
        Err(e) => bail!(
            "Couldn't resolve WebSocket locator address: {}: {}",
            address,
            e
        ),
    }
}
