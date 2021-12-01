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
use super::*;
use async_std::net::{SocketAddr, ToSocketAddrs};
use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;
use zenoh_util::core::Result as ZResult;
use zenoh_util::properties::Properties;

#[allow(unreachable_patterns)]
pub(super) async fn get_tcp_addr(address: &LocatorAddress) -> ZResult<SocketAddr> {
    match address {
        LocatorAddress::Tcp(addr) => match addr {
            LocatorTcp::SocketAddr(addr) => Ok(*addr),
            LocatorTcp::DnsName(addr) => match addr.to_socket_addrs().await {
                Ok(mut addr_iter) => {
                    if let Some(addr) = addr_iter.next() {
                        Ok(addr)
                    } else {
                        bail!("Couldn't resolve TCP locator address: {}", addr);
                    }
                }
                Err(e) => {
                    bail!("{}: {}", e, addr);
                }
            },
        },
        _ => {
            bail!("Not a TCP locator address: {}", address);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocatorTcp {
    SocketAddr(SocketAddr),
    DnsName(String),
}

impl LocatorTcp {
    pub fn is_multicast(&self) -> bool {
        false
    }
}

impl FromStr for LocatorTcp {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(addr) => Ok(LocatorTcp::SocketAddr(addr)),
            Err(_) => Ok(LocatorTcp::DnsName(s.to_string())),
        }
    }
}

impl fmt::Display for LocatorTcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocatorTcp::SocketAddr(addr) => write!(f, "{}", addr)?,
            LocatorTcp::DnsName(addr) => write!(f, "{}", addr)?,
        }
        Ok(())
    }
}

/*************************************/
/*          LOCATOR CONFIG           */
/*************************************/
pub struct LocatorConfigTcp;

impl LocatorConfigTcp {
    pub fn from_config(_config: &crate::config::Config) -> ZResult<Option<Properties>> {
        Ok(None)
    }
}
