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
use super::config::*;
use super::*;
use crate::config::Config;
pub use async_rustls::rustls::*;
pub use async_rustls::webpki::*;
use async_std::net::{SocketAddr, ToSocketAddrs};
use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;
use zenoh_core::Result as ZResult;
use zenoh_core::{bail, zerror};
use zenoh_util::properties::config::{ZN_FALSE, ZN_TRUE};
use zenoh_util::properties::Properties;

#[allow(unreachable_patterns)]
pub(super) async fn get_tls_addr(address: &LocatorAddress) -> ZResult<SocketAddr> {
    match &address {
        LocatorAddress::Tls(addr) => match addr {
            LocatorTls::SocketAddr(addr) => Ok(*addr),
            LocatorTls::DnsName(addr) => match addr.to_socket_addrs().await {
                Ok(mut addr_iter) => {
                    if let Some(addr) = addr_iter.next() {
                        Ok(addr)
                    } else {
                        bail!("Couldn't resolve TLS locator address: {}", addr);
                    }
                }
                Err(e) => {
                    bail!("{}: {}", e, addr);
                }
            },
        },
        _ => {
            bail!("Not a TLS locator address: {}", address);
        }
    }
}

pub(super) fn get_tls_host(address: &LocatorAddress) -> ZResult<String> {
    match address {
        LocatorAddress::Tls(addr) => match addr {
            LocatorTls::SocketAddr(addr) => {
                bail!("Couldn't get host from SocketAddr: {}", addr)
            }
            LocatorTls::DnsName(addr) => {
                let mut split = addr.split(':');
                match split.next() {
                    Some(domain) => Ok(domain.into()),
                    None => bail!("Couldn't get host for: {}", addr),
                }
            }
        },
        _ => bail!("Not a TLS locator address: {}", address),
    }
}

#[allow(unreachable_patterns)]
pub(super) async fn get_tls_dns(address: &LocatorAddress) -> ZResult<DNSName> {
    let domain = get_tls_host(address)?;
    let domain = DNSNameRef::try_from_ascii_str(domain.as_str()).map_err(|e| zerror!(e))?;
    Ok(domain.to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocatorTls {
    SocketAddr(SocketAddr),
    DnsName(String),
}

impl LocatorTls {
    pub fn is_multicast(&self) -> bool {
        false
    }
}

impl FromStr for LocatorTls {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(addr) => Ok(LocatorTls::SocketAddr(addr)),
            Err(_) => Ok(LocatorTls::DnsName(s.to_string())),
        }
    }
}

impl fmt::Display for LocatorTls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocatorTls::SocketAddr(addr) => write!(f, "{}", addr)?,
            LocatorTls::DnsName(addr) => write!(f, "{}", addr)?,
        }
        Ok(())
    }
}

/*************************************/
/*          LOCATOR CONFIG           */
/*************************************/
#[derive(Clone)]
pub struct LocatorConfigTls;

impl LocatorConfigTls {
    pub fn from_config(config: &Config) -> ZResult<Option<Properties>> {
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

        if properties.is_empty() {
            Ok(None)
        } else {
            Ok(Some(properties))
        }
    }
}
