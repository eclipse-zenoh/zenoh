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
use std::{convert::TryFrom, net::SocketAddr};

use async_std::net::ToSocketAddrs;
use async_trait::async_trait;
use config::{
    TLS_CLIENT_AUTH, TLS_CLIENT_CERTIFICATE_FILE, TLS_CLIENT_PRIVATE_KEY_FILE,
    TLS_ROOT_CA_CERTIFICATE_FILE, TLS_SERVER_CERTIFICATE_FILE, TLS_SERVER_PRIVATE_KEY_FILE,
};
use zenoh_cfg_properties::Properties;
use zenoh_config::{Config, ZN_FALSE, ZN_TRUE};
use zenoh_core::zconfigurable;
use zenoh_link_commons::{ConfigurationInspector, LocatorInspector};
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
#[derive(Default, Clone, Copy, Debug)]
pub struct TlsConfigurator;
#[async_trait]
impl ConfigurationInspector<Config> for TlsConfigurator {
    async fn inspect_config(&self, config: &Config) -> ZResult<Properties> {
        let mut properties = Properties::default();

        let c = config.transport().link().tls();
        if let Some(tls_ca_certificate) = c.root_ca_certificate() {
            properties.insert(
                TLS_ROOT_CA_CERTIFICATE_FILE.into(),
                tls_ca_certificate.into(),
            );
        }
        if let Some(tls_server_private_key) = c.server_private_key() {
            properties.insert(
                TLS_SERVER_PRIVATE_KEY_FILE.into(),
                tls_server_private_key.into(),
            );
        }
        if let Some(tls_server_certificate) = c.server_certificate() {
            properties.insert(
                TLS_SERVER_CERTIFICATE_FILE.into(),
                tls_server_certificate.into(),
            );
        }
        if let Some(tls_client_auth) = c.client_auth() {
            match tls_client_auth {
                true => properties.insert(TLS_CLIENT_AUTH.into(), ZN_TRUE.into()),
                false => properties.insert(TLS_CLIENT_AUTH.into(), ZN_FALSE.into()),
            };
        }
        if let Some(tls_client_private_key) = c.client_private_key() {
            properties.insert(
                TLS_CLIENT_PRIVATE_KEY_FILE.into(),
                tls_client_private_key.into(),
            );
        }
        if let Some(tls_client_certificate) = c.client_certificate() {
            properties.insert(
                TLS_CLIENT_CERTIFICATE_FILE.into(),
                tls_client_certificate.into(),
            );
        }

        Ok(properties)
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

pub mod config {
    use zenoh_cfg_properties::config::*;
    pub const TLS_ROOT_CA_CERTIFICATE_FILE: &str = ZN_TLS_ROOT_CA_CERTIFICATE_STR;
    pub const TLS_ROOT_CA_CERTIFICATE_RAW: &str = "tls_root_ca_certificate_raw";

    pub const TLS_SERVER_PRIVATE_KEY_FILE: &str = ZN_TLS_SERVER_PRIVATE_KEY_STR;
    pub const TLS_SERVER_PRIVATE_KEY_RAW: &str = "tls_server_private_key_raw";

    pub const TLS_SERVER_CERTIFICATE_FILE: &str = ZN_TLS_SERVER_CERTIFICATE_STR;
    pub const TLS_SERVER_CERTIFICATE_RAW: &str = "tls_server_certificate_raw";

    pub const TLS_CLIENT_PRIVATE_KEY_FILE: &str = ZN_TLS_CLIENT_PRIVATE_KEY_STR;
    pub const TLS_CLIENT_PRIVATE_KEY_RAW: &str = "tls_client_private_key_raw";

    pub const TLS_CLIENT_CERTIFICATE_FILE: &str = ZN_TLS_CLIENT_CERTIFICATE_STR;
    pub const TLS_CLIENT_CERTIFICATE_RAW: &str = "tls_client_certificate_raw";

    pub const TLS_CLIENT_AUTH: &str = ZN_TLS_CLIENT_AUTH_STR;
    pub const TLS_CLIENT_AUTH_DEFAULT: &str = ZN_TLS_CLIENT_AUTH_DEFAULT;
}

pub async fn get_tls_addr(address: &Address<'_>) -> ZResult<SocketAddr> {
    match address.as_str().to_socket_addrs().await?.next() {
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

pub fn get_tls_server_name(address: &Address<'_>) -> ZResult<ServerName> {
    Ok(ServerName::try_from(get_tls_host(address)?).map_err(|e| zerror!(e))?)
}
