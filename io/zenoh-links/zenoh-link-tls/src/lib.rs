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
use async_trait::async_trait;
use rustls_pki_types::ServerName;
use std::{convert::TryFrom, net::SocketAddr};
use zenoh_core::zconfigurable;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::core::{endpoint::Address, Locator};
use zenoh_result::{bail, zerror, ZResult};

mod unicast;
pub use unicast::*;

// Default MTU (TLS PDU) in bytes.
// NOTE: Since TLS is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the TLS MTU is constrained to
//       2^16 - 1 bytes (i.e., 65535).
const TLS_MAX_MTU: u16 = u16::MAX;
pub const TLS_LOCATOR_PREFIX: &str = "tls";

#[derive(Default, Clone, Copy)]
pub struct TlsLocatorInspector;
#[async_trait]
impl LocatorInspector for TlsLocatorInspector {
    fn protocol(&self) -> &str {
        TLS_LOCATOR_PREFIX
    }

    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }
}

zconfigurable! {
    // Default MTU (TLS PDU) in bytes.
    static ref TLS_DEFAULT_MTU: u16 = TLS_MAX_MTU;
    // The LINGER option causes the shutdown() call to block until (1) all application data is delivered
    // to the remote end or (2) a timeout expires. The timeout is expressed in seconds.
    // More info on the LINGER option and its dynamics can be found at:
    // https://blog.netherlabs.nl/articles/2009/01/18/the-ultimate-so_linger-page-or-why-is-my-tcp-not-reliable
    static ref TLS_LINGER_TIMEOUT: i32 = 10;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref TLS_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

pub async fn get_tls_addr(address: &Address<'_>) -> ZResult<SocketAddr> {
    match tokio::net::lookup_host(address.as_str()).await?.next() {
        Some(addr) => Ok(addr),
        None => bail!("Couldn't resolve TLS locator address: {}", address),
    }
}

pub fn get_tls_host<'a>(address: &'a Address<'a>) -> ZResult<&'a str> {
    address
        .as_str()
        .split(':')
        .next()
        .ok_or_else(|| zerror!("Invalid TLS address").into())
}

pub fn get_tls_server_name<'a>(address: &'a Address<'a>) -> ZResult<ServerName<'a>> {
    Ok(ServerName::try_from(get_tls_host(address)?).map_err(|e| zerror!(e))?)
}

pub fn base64_decode(data: &str) -> ZResult<Vec<u8>> {
    use base64::engine::general_purpose;
    use base64::Engine;
    Ok(general_purpose::STANDARD
        .decode(data)
        .map_err(|e| zerror!("Unable to perform base64 decoding: {e:?}"))?)
}
