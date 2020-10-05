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
#[cfg(any(feature = "tcp", feature = "udp"))]
use async_std::net::SocketAddr;
#[cfg(any(feature = "tcp", feature = "udp"))]
use async_std::net::ToSocketAddrs;
#[cfg(any(feature = "tcp", feature = "udp"))]
use async_std::task;

use std::cmp::PartialEq;
use std::fmt;
use std::hash::Hash;
use std::str::FromStr;

use zenoh_util::core::{ZError, ZErrorKind};
use zenoh_util::zerror;

/*************************************/
/*          LOCATOR                  */
/*************************************/
pub const PROTO_SEPARATOR: char = '/';
pub const PORT_SEPARATOR: char = ':';
// Protocol literals
#[cfg(feature = "tcp")]
pub const STR_TCP: &str = "tcp";
#[cfg(feature = "udp")]
pub const STR_UDP: &str = "udp";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocatorProtocol {
    #[cfg(feature = "tcp")]
    Tcp,
    #[cfg(feature = "udp")]
    Udp,
}

impl fmt::Display for LocatorProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "tcp")]
            LocatorProtocol::Tcp => write!(f, "{}", STR_TCP)?,
            #[cfg(feature = "udp")]
            LocatorProtocol::Udp => write!(f, "{}", STR_UDP)?,
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Locator {
    #[cfg(feature = "tcp")]
    Tcp(SocketAddr),
    #[cfg(feature = "udp")]
    Udp(SocketAddr),
}

impl FromStr for Locator {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split: Vec<&str> = s.split(PROTO_SEPARATOR).collect();
        if split.len() != 2 {
            return zerror!(ZErrorKind::InvalidLocator {
                descr: format!("Missing protocol: {}", s)
            });
        }
        let (proto, addr) = (split[0], split[1]);
        match proto {
            #[cfg(feature = "tcp")]
            STR_TCP => {
                let addr = task::block_on(async {
                    match addr.to_socket_addrs().await {
                        Ok(mut addr_iter) => {
                            if let Some(addr) = addr_iter.next() {
                                Ok(addr)
                            } else {
                                let e = format!("Couldn't resolve TCP locator: {}", s.to_string());
                                log::warn!("{}", e);
                                zerror!(ZErrorKind::InvalidLocator { descr: e })
                            }
                        }
                        Err(e) => {
                            let e = format!("{}: {}", e, addr);
                            log::warn!("{}", e);
                            zerror!(ZErrorKind::InvalidLocator { descr: e })
                        }
                    }
                });
                addr.map(|a| Locator::Tcp(a))
            }
            #[cfg(feature = "udp")]
            STR_UDP => {
                let addr = task::block_on(async {
                    match addr.to_socket_addrs().await {
                        Ok(mut addr_iter) => {
                            if let Some(addr) = addr_iter.next() {
                                Ok(addr)
                            } else {
                                let e = format!("Couldn't resolve UDP locator: {}", s.to_string());
                                log::warn!("{}", e);
                                zerror!(ZErrorKind::InvalidLocator { descr: e })
                            }
                        }
                        Err(e) => {
                            let e = format!("Invalid UDP locator: {}", e);
                            log::warn!("{}", e);
                            zerror!(ZErrorKind::InvalidLocator { descr: e })
                        }
                    }
                });
                addr.map(|a| Locator::Udp(a))
            }
            _ => {
                let e = format!("Invalid protocol locator: {}", proto);
                log::warn!("{}", e);
                zerror!(ZErrorKind::InvalidLocator { descr: e })
            }
        }
    }
}

impl Locator {
    pub fn get_proto(&self) -> LocatorProtocol {
        match self {
            #[cfg(feature = "tcp")]
            Locator::Tcp(..) => LocatorProtocol::Tcp,
            #[cfg(feature = "udp")]
            Locator::Udp(..) => LocatorProtocol::Udp,
        }
    }
}

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "tcp")]
            Locator::Tcp(addr) => write!(f, "{}/{}", STR_TCP, addr)?,
            #[cfg(feature = "udp")]
            Locator::Udp(addr) => write!(f, "{}/{}", STR_UDP, addr)?,
        }
        Ok(())
    }
}

impl fmt::Debug for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (proto, addr): (&str, String) = match self {
            #[cfg(feature = "tcp")]
            Locator::Tcp(addr) => (STR_TCP, addr.to_string()),
            #[cfg(feature = "udp")]
            Locator::Udp(addr) => (STR_UDP, addr.to_string()),
        };

        f.debug_struct("Locator")
            .field("protocol", &proto)
            .field("address", &addr)
            .finish()
    }
}
