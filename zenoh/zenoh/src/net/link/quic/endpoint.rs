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
use crate::net::link::{quic::config::*, LocatorAddress};
use async_std::net::{SocketAddr, ToSocketAddrs};
use std::fmt;
use std::str::FromStr;
use webpki::{DnsName, DnsNameRef};
use zenoh_cfg_properties::Properties;
use zenoh_core::{zerror, Result as ZResult};

#[allow(unreachable_patterns)]
pub(super) async fn get_quic_addr(address: &LocatorAddress) -> ZResult<SocketAddr> {
    match &address {
        LocatorAddress::Quic(addr) => match addr {
            LocatorQuic::SocketAddr(addr) => Ok(*addr),
            LocatorQuic::DnsName(addr) => match addr.to_socket_addrs().await {
                Ok(mut addr_iter) => {
                    if let Some(addr) = addr_iter.next() {
                        Ok(addr)
                    } else {
                        bail!("Couldn't resolve QUIC locator address: {}", addr);
                    }
                }
                Err(e) => {
                    bail!("{}: {}", e, addr);
                }
            },
        },
        _ => {
            bail!("Not a QUIC locator address: {}", address);
        }
    }
}

#[allow(unreachable_patterns)]
pub(super) async fn get_quic_dns(address: &LocatorAddress) -> ZResult<DnsName> {
    match &address {
        LocatorAddress::Quic(addr) => match addr {
            LocatorQuic::SocketAddr(addr) => {
                bail!("Couldn't get domain from SocketAddr: {}", addr);
            }
            LocatorQuic::DnsName(addr) => {
                // Separate the domain from the port.
                // E.g. zenoh.io:7447 returns (zenoh.io, 7447).
                let split: Vec<&str> = addr.split(':').collect();
                match split.get(0) {
                    Some(dom) => {
                        let domain = DnsNameRef::try_from_ascii_str(dom).map_err(|e| zerror!(e))?;
                        Ok(domain.to_owned())
                    }
                    None => {
                        bail!("Couldn't get domain for: {}", addr);
                    }
                }
            }
        },
        _ => {
            bail!("Not a QUIC locator address: {}", address);
        }
    }
}

/*************************************/
/*             LOCATOR               */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocatorQuic {
    SocketAddr(SocketAddr),
    DnsName(String),
}

impl LocatorQuic {
    pub fn is_multicast(&self) -> bool {
        false
    }
}

impl FromStr for LocatorQuic {
    type Err = zenoh_core::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(addr) => Ok(LocatorQuic::SocketAddr(addr)),
            Err(_) => Ok(LocatorQuic::DnsName(s.to_string())),
        }
    }
}

impl fmt::Display for LocatorQuic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocatorQuic::SocketAddr(addr) => write!(f, "{}", addr)?,
            LocatorQuic::DnsName(addr) => write!(f, "{}", addr)?,
        }
        Ok(())
    }
}

/*************************************/
/*          LOCATOR CONFIG           */
/*************************************/
pub struct LocatorConfigQuic;

impl LocatorConfigQuic {
    pub fn from_config(config: &crate::config::Config) -> ZResult<Option<Properties>> {
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

        if properties.is_empty() {
            Ok(None)
        } else {
            Ok(Some(properties))
        }
    }
}
