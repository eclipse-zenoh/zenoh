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
use super::{Locator, LocatorAddress};
use async_std::net::{SocketAddr, ToSocketAddrs};
use std::fmt;
use std::str::FromStr;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

#[allow(unreachable_patterns)]
pub(super) async fn get_udp_addr(locator: &Locator) -> ZResult<SocketAddr> {
    match &locator.address {
        LocatorAddress::Udp(addr) => match addr {
            LocatorUdp::SocketAddr(addr) => Ok(*addr),
            LocatorUdp::DnsName(addr) => match addr.to_socket_addrs().await {
                Ok(mut addr_iter) => {
                    if let Some(addr) = addr_iter.next() {
                        Ok(addr)
                    } else {
                        let e = format!("Couldn't resolve UDP locator: {}", addr);
                        zerror!(ZErrorKind::InvalidLocator { descr: e })
                    }
                }
                Err(e) => {
                    let e = format!("{}: {}", e, addr);
                    zerror!(ZErrorKind::InvalidLocator { descr: e })
                }
            },
        },
        _ => {
            let e = format!("Not a UDP locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocatorUdp {
    SocketAddr(SocketAddr),
    DnsName(String),
}

impl LocatorUdp {
    pub fn is_multicast(&self) -> bool {
        match self {
            LocatorUdp::SocketAddr(addr) => addr.ip().is_multicast(),
            LocatorUdp::DnsName(_) => false,
        }
    }
}

impl FromStr for LocatorUdp {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(addr) => Ok(LocatorUdp::SocketAddr(addr)),
            Err(_) => Ok(LocatorUdp::DnsName(s.to_string())),
        }
    }
}

impl fmt::Display for LocatorUdp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocatorUdp::SocketAddr(addr) => write!(f, "{}", addr)?,
            LocatorUdp::DnsName(addr) => write!(f, "{}", addr)?,
        }
        Ok(())
    }
}

pub type LocatorPropertyUdp = ();

pub mod options {
    pub const IFACE: &str = "iface";
}
