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
#[cfg(any(feature = "transport_tcp", feature = "transport_udp"))]
use async_std::net::SocketAddr;
#[cfg(any(feature = "transport_tcp", feature = "transport_udp"))]
use async_std::net::ToSocketAddrs;
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
use async_std::path::PathBuf;
#[cfg(any(feature = "transport_tcp", feature = "transport_udp", feat))]
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
#[cfg(feature = "transport_tcp")]
pub const STR_TCP: &str = "tcp";
#[cfg(feature = "transport_udp")]
pub const STR_UDP: &str = "udp";
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
pub const STR_UNIXSOCK_STREAM: &str = "unixsock-stream";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocatorProtocol {
    #[cfg(feature = "transport_tcp")]
    Tcp,
    #[cfg(feature = "transport_udp")]
    Udp,
    #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
    UnixSockStream,
}

impl fmt::Display for LocatorProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "transport_tcp")]
            LocatorProtocol::Tcp => write!(f, "{}", STR_TCP)?,
            #[cfg(feature = "transport_udp")]
            LocatorProtocol::Udp => write!(f, "{}", STR_UDP)?,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorProtocol::UnixSockStream => write!(f, "{}", STR_UNIXSOCK_STREAM)?,
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Locator {
    #[cfg(feature = "transport_tcp")]
    Tcp(SocketAddr),
    #[cfg(feature = "transport_udp")]
    Udp(SocketAddr),
    #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
    UnixSockStream(PathBuf),
}

impl FromStr for Locator {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let sep_index = s.find(PROTO_SEPARATOR);

        let index = match sep_index {
            Some(index) => index,
            None => {
                return zerror!(ZErrorKind::InvalidLocator {
                    descr: format!("Invalid locator: {}", s)
                });
            }
        };

        let (proto, addr) = s.split_at(index);
        let addr = match addr.strip_prefix(PROTO_SEPARATOR) {
            Some(addr) => addr,
            None => {
                return zerror!(ZErrorKind::InvalidLocator {
                    descr: format!("Invalid locator: {}", s)
                });
            }
        };

        match proto {
            #[cfg(feature = "transport_tcp")]
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
                addr.map(Locator::Tcp)
            }
            #[cfg(feature = "transport_udp")]
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
                addr.map(Locator::Udp)
            }
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            STR_UNIXSOCK_STREAM => {
                let addr = task::block_on(async {
                    match PathBuf::from(addr).to_str() {
                        Some(path) => Ok(PathBuf::from(path)),
                        None => {
                            let e = format!("Invalid Unix locator: {:?}", addr);
                            log::warn!("{}", e);
                            zerror!(ZErrorKind::InvalidLocator { descr: e })
                        }
                    }
                });
                addr.map(Locator::UnixSockStream)
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
            #[cfg(feature = "transport_tcp")]
            Locator::Tcp(..) => LocatorProtocol::Tcp,
            #[cfg(feature = "transport_udp")]
            Locator::Udp(..) => LocatorProtocol::Udp,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            Locator::UnixSockStream(..) => LocatorProtocol::UnixSockStream,
        }
    }
}

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "transport_tcp")]
            Locator::Tcp(addr) => write!(f, "{}/{}", STR_TCP, addr)?,
            #[cfg(feature = "transport_udp")]
            Locator::Udp(addr) => write!(f, "{}/{}", STR_UDP, addr)?,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            Locator::UnixSockStream(addr) => {
                let path = addr.to_str().unwrap_or("None");
                write!(f, "{}/{}", STR_UNIXSOCK_STREAM, path)?
            }
        }
        Ok(())
    }
}

impl fmt::Debug for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (proto, addr): (&str, String) = match self {
            #[cfg(feature = "transport_tcp")]
            Locator::Tcp(addr) => (STR_TCP, addr.to_string()),
            #[cfg(feature = "transport_udp")]
            Locator::Udp(addr) => (STR_UDP, addr.to_string()),
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            Locator::UnixSockStream(addr) => {
                let path = addr.to_str().unwrap_or("None");
                (STR_UNIXSOCK_STREAM, path.to_string())
            }
        };

        f.debug_struct("Locator")
            .field("protocol", &proto)
            .field("address", &addr)
            .finish()
    }
}
