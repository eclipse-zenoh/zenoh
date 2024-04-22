// Copyright (c) 2024 ZettaScale Technology
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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
use async_trait::async_trait;
use config::{
    TLS_CLIENT_AUTH, TLS_CLIENT_CERTIFICATE_BASE64, TLS_CLIENT_CERTIFICATE_FILE,
    TLS_CLIENT_PRIVATE_KEY_BASE64, TLS_CLIENT_PRIVATE_KEY_FILE, TLS_ROOT_CA_CERTIFICATE_BASE64,
    TLS_ROOT_CA_CERTIFICATE_FILE, TLS_SERVER_CERTIFICATE_BASE64, TLS_SERVER_CERTIFICATE_FILE,
    TLS_SERVER_NAME_VERIFICATION, TLS_SERVER_PRIVATE_KEY_BASE_64, TLS_SERVER_PRIVATE_KEY_FILE,
};
use rustls_pki_types::ServerName;
use secrecy::ExposeSecret;
use std::{convert::TryFrom, net::SocketAddr};
use zenoh_config::Config;
use zenoh_core::zconfigurable;
use zenoh_link_commons::{ConfigurationInspector, LocatorInspector};
use zenoh_protocol::core::{
    endpoint::{self, Address},
    Locator,
};
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

impl ConfigurationInspector<Config> for TlsConfigurator {
    fn inspect_config(&self, config: &Config) -> ZResult<String> {
        let mut ps: Vec<(&str, &str)> = vec![];

        let c = config.transport().link().tls();

        match (c.root_ca_certificate(), c.root_ca_certificate_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'root_ca_certificate' and 'root_ca_certificate_base64' can be present!")
            }
            (Some(ca_certificate), None) => {
                ps.push((TLS_ROOT_CA_CERTIFICATE_FILE, ca_certificate));
            }
            (None, Some(ca_certificate)) => {
                ps.push((
                    TLS_ROOT_CA_CERTIFICATE_BASE64,
                    ca_certificate.expose_secret(),
                ));
            }
            _ => {}
        }

        match (c.server_private_key(), c.server_private_key_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'server_private_key' and 'server_private_key_base64' can be present!")
            }
            (Some(server_private_key), None) => {
                ps.push((TLS_SERVER_PRIVATE_KEY_FILE, server_private_key));
            }
            (None, Some(server_private_key)) => {
                ps.push((
                    TLS_SERVER_PRIVATE_KEY_BASE_64,
                    server_private_key.expose_secret(),
                ));
            }
            _ => {}
        }

        match (c.server_certificate(), c.server_certificate_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'server_certificate' and 'server_certificate_base64' can be present!")
            }
            (Some(server_certificate), None) => {
                ps.push((TLS_SERVER_CERTIFICATE_FILE, server_certificate));
            }
            (None, Some(server_certificate)) => {
                ps.push((
                    TLS_SERVER_CERTIFICATE_BASE64,
                    server_certificate.expose_secret(),
                ));
            }
            _ => {}
        }

        if let Some(client_auth) = c.client_auth() {
            match client_auth {
                true => ps.push((TLS_CLIENT_AUTH, "true")),
                false => ps.push((TLS_CLIENT_AUTH, "false")),
            };
        }

        match (c.client_private_key(), c.client_private_key_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'client_private_key' and 'client_private_key_base64' can be present!")
            }
            (Some(client_private_key), None) => {
                ps.push((TLS_CLIENT_PRIVATE_KEY_FILE, client_private_key));
            }
            (None, Some(client_private_key)) => {
                ps.push((
                    TLS_CLIENT_PRIVATE_KEY_BASE64,
                    client_private_key.expose_secret(),
                ));
            }
            _ => {}
        }

        match (c.client_certificate(), c.client_certificate_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'client_certificate' and 'client_certificate_base64' can be present!")
            }
            (Some(client_certificate), None) => {
                ps.push((TLS_CLIENT_CERTIFICATE_FILE, client_certificate));
            }
            (None, Some(client_certificate)) => {
                ps.push((
                    TLS_CLIENT_CERTIFICATE_BASE64,
                    client_certificate.expose_secret(),
                ));
            }
            _ => {}
        }

        if let Some(server_name_verification) = c.server_name_verification() {
            match server_name_verification {
                true => ps.push((TLS_SERVER_NAME_VERIFICATION, "true")),
                false => ps.push((TLS_SERVER_NAME_VERIFICATION, "false")),
            };
        }

        let mut s = String::new();
        endpoint::Parameters::extend(ps.drain(..), &mut s);

        Ok(s)
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
    pub const TLS_ROOT_CA_CERTIFICATE_FILE: &str = "root_ca_certificate_file";
    pub const TLS_ROOT_CA_CERTIFICATE_RAW: &str = "root_ca_certificate_raw";
    pub const TLS_ROOT_CA_CERTIFICATE_BASE64: &str = "root_ca_certificate_base64";

    pub const TLS_SERVER_PRIVATE_KEY_FILE: &str = "server_private_key_file";
    pub const TLS_SERVER_PRIVATE_KEY_RAW: &str = "server_private_key_raw";
    pub const TLS_SERVER_PRIVATE_KEY_BASE_64: &str = "server_private_key_base64";

    pub const TLS_SERVER_CERTIFICATE_FILE: &str = "server_certificate_file";
    pub const TLS_SERVER_CERTIFICATE_RAW: &str = "server_certificate_raw";
    pub const TLS_SERVER_CERTIFICATE_BASE64: &str = "server_certificate_base64";

    pub const TLS_CLIENT_PRIVATE_KEY_FILE: &str = "client_private_key_file";
    pub const TLS_CLIENT_PRIVATE_KEY_RAW: &str = "client_private_key_raw";
    pub const TLS_CLIENT_PRIVATE_KEY_BASE64: &str = "client_private_key_base64";

    pub const TLS_CLIENT_CERTIFICATE_FILE: &str = "client_certificate_file";
    pub const TLS_CLIENT_CERTIFICATE_RAW: &str = "client_certificate_raw";
    pub const TLS_CLIENT_CERTIFICATE_BASE64: &str = "client_certificate_base64";

    pub const TLS_CLIENT_AUTH: &str = "client_auth";

    pub const TLS_SERVER_NAME_VERIFICATION: &str = "server_name_verification";
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
