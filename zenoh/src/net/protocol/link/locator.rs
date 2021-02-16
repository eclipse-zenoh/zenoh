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
#[cfg(feature = "transport_tcp")]
use super::tcp::{LocatorPropertyTcp, LocatorTcp};
#[cfg(feature = "transport_tls")]
use super::tls::{LocatorPropertyTls, LocatorTls};
#[cfg(feature = "transport_udp")]
use super::udp::{LocatorPropertyUdp, LocatorUdp};
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
use super::unixsock_stream::{LocatorPropertyUnixSocketStream, LocatorUnixSocketStream};
use std::cmp::PartialEq;
use std::fmt;
use std::hash::Hash;
use std::str::FromStr;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::ConfigProperties;
use zenoh_util::zerror;

/*************************************/
/*        LOCATOR PROTOCOL           */
/*************************************/
pub const PROTO_SEPARATOR: char = '/';
// Protocol literals
#[cfg(feature = "transport_tcp")]
pub const STR_TCP: &str = "tcp";
#[cfg(feature = "transport_udp")]
pub const STR_UDP: &str = "udp";
#[cfg(feature = "transport_tls")]
pub const STR_TLS: &str = "tls";
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
pub const STR_UNIXSOCK_STREAM: &str = "unixsock-stream";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocatorProtocol {
    #[cfg(feature = "transport_tcp")]
    Tcp,
    #[cfg(feature = "transport_udp")]
    Udp,
    #[cfg(feature = "transport_tls")]
    Tls,
    #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
    UnixSocketStream,
}

impl fmt::Display for LocatorProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "transport_tcp")]
            LocatorProtocol::Tcp => write!(f, "{}", STR_TCP)?,
            #[cfg(feature = "transport_udp")]
            LocatorProtocol::Udp => write!(f, "{}", STR_UDP)?,
            #[cfg(feature = "transport_tls")]
            LocatorProtocol::Tls => write!(f, "{}", STR_TLS)?,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorProtocol::UnixSocketStream => write!(f, "{}", STR_UNIXSOCK_STREAM)?,
        }
        Ok(())
    }
}

/*************************************/
/*             LOCATOR               */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Locator {
    #[cfg(feature = "transport_tcp")]
    Tcp(LocatorTcp),
    #[cfg(feature = "transport_udp")]
    Udp(LocatorUdp),
    #[cfg(feature = "transport_tls")]
    Tls(LocatorTls),
    #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
    UnixSocketStream(LocatorUnixSocketStream),
}

impl FromStr for Locator {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let sep_index = s.find(PROTO_SEPARATOR);

        let index = sep_index.ok_or_else(|| {
            zerror2!(ZErrorKind::InvalidLocator {
                descr: format!("Invalid locator: {}", s)
            })
        })?;

        let (proto, addr) = s.split_at(index);
        let addr = addr.strip_prefix(PROTO_SEPARATOR).ok_or_else(|| {
            zerror2!(ZErrorKind::InvalidLocator {
                descr: format!("Invalid locator: {}", s)
            })
        })?;

        match proto {
            #[cfg(feature = "transport_tcp")]
            STR_TCP => addr.parse().map(Locator::Tcp),
            #[cfg(feature = "transport_udp")]
            STR_UDP => addr.parse().map(Locator::Udp),
            #[cfg(feature = "transport_tls")]
            STR_TLS => addr.parse().map(Locator::Tls),
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            STR_UNIXSOCK_STREAM => addr.parse().map(Locator::UnixSocketStream),
            _ => {
                let e = format!("Invalid protocol locator: {}", proto);
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
            #[cfg(feature = "transport_tls")]
            Locator::Tls(..) => LocatorProtocol::Tls,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            Locator::UnixSocketStream(..) => LocatorProtocol::UnixSocketStream,
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
            #[cfg(feature = "transport_tls")]
            Locator::Tls(addr) => write!(f, "{}/{}", STR_TLS, addr)?,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            Locator::UnixSocketStream(addr) => write!(f, "{}/{}", STR_UNIXSOCK_STREAM, addr)?,
        }
        Ok(())
    }
}

/*************************************/
/*        LOCATOR PROPERTY           */
/*************************************/
#[derive(Clone)]
pub enum LocatorProperty {
    #[cfg(feature = "transport_tcp")]
    Tcp(LocatorPropertyTcp),
    #[cfg(feature = "transport_udp")]
    Udp(LocatorPropertyUdp),
    #[cfg(feature = "transport_tls")]
    Tls(LocatorPropertyTls),
    #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
    UnixSocketStream(LocatorPropertyUnixSocketStream),
}

impl LocatorProperty {
    pub fn get_proto(&self) -> LocatorProtocol {
        match self {
            #[cfg(feature = "transport_tcp")]
            LocatorProperty::Tcp(..) => LocatorProtocol::Tcp,
            #[cfg(feature = "transport_udp")]
            LocatorProperty::Udp(..) => LocatorProtocol::Udp,
            #[cfg(feature = "transport_tls")]
            LocatorProperty::Tls(..) => LocatorProtocol::Tls,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorProperty::UnixSocketStream(..) => LocatorProtocol::UnixSocketStream,
        }
    }

    pub async fn from_properties(config: &ConfigProperties) -> ZResult<Vec<LocatorProperty>> {
        let mut ps: Vec<LocatorProperty> = vec![];
        #[cfg(feature = "transport_tls")]
        {
            let mut res = LocatorPropertyTls::from_properties(config).await?;
            if let Some(p) = res.take() {
                ps.push(p);
            }
        }
        Ok(ps)
    }
}

impl fmt::Display for LocatorProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "transport_tcp")]
            LocatorProperty::Tcp(..) => write!(f, "{}", STR_TCP)?,
            #[cfg(feature = "transport_udp")]
            LocatorProperty::Udp(..) => write!(f, "{}", STR_UDP)?,
            #[cfg(feature = "transport_tls")]
            LocatorProperty::Tls(..) => write!(f, "{}", STR_TLS)?,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorProperty::UnixSocketStream(..) => write!(f, "{}", STR_UNIXSOCK_STREAM)?,
        }
        Ok(())
    }
}

impl fmt::Debug for LocatorProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)?;
        Ok(())
    }
}
