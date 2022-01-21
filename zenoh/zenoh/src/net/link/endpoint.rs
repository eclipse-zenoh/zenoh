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
#[cfg(feature = "transport_quic")]
use super::quic::{LocatorConfigQuic, LocatorQuic};
#[cfg(feature = "transport_tcp")]
use super::tcp::{LocatorConfigTcp, LocatorTcp};
#[cfg(feature = "transport_tls")]
use super::tls::{LocatorConfigTls, LocatorTls};
#[cfg(feature = "transport_udp")]
use super::udp::{LocatorConfigUdp, LocatorUdp};
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
use super::unixsock_stream::{LocatorConfigUnixSocketStream, LocatorUnixSocketStream};
use crate::config::Config;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::Arc;
use zenoh_cfg_properties::Properties;
use zenoh_core::{bail, zerror, Error as ZError, Result as ZResult};

/*************************************/
/*             CONSTS                */
/*************************************/
// Protocol literals
#[cfg(feature = "transport_tcp")]
pub const STR_TCP: &str = "tcp";
#[cfg(feature = "transport_udp")]
pub const STR_UDP: &str = "udp";
#[cfg(feature = "transport_tls")]
pub const STR_TLS: &str = "tls";
#[cfg(feature = "transport_quic")]
pub const STR_QUIC: &str = "quic";
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
pub const STR_UNIXSOCK_STREAM: &str = "unixsock-stream";

// Parsing chars
pub const PROTO_SEPARATOR: char = '/';
pub const METADATA_SEPARATOR: char = '?';
pub const CONFIG_SEPARATOR: char = '#';
pub const LIST_SEPARATOR: char = ';';
pub const FIELD_SEPARATOR: char = '=';

// @TODO: port it as a Properties parsing
fn str_to_properties(s: &str) -> Option<Properties> {
    let mut hm = Properties::default();
    for p in s.split(LIST_SEPARATOR) {
        let field_index = p.find(FIELD_SEPARATOR)?;
        let (key, value) = p.split_at(field_index);
        hm.insert(key.to_string(), value[1..].to_string());
    }
    Some(hm)
}

// @TODO: port it as a Properties display
fn properties_to_str(hm: &Properties) -> String {
    let mut s = "".to_string();
    let mut count = hm.len();
    for (key, value) in hm.iter() {
        s.push_str(&format!("{}{}{}", key, FIELD_SEPARATOR, value));
        count -= 1;
        if count > 0 {
            s.push(LIST_SEPARATOR);
        }
    }
    s
}

/*************************************/
/*            ENDPOINT               */
/*************************************/
#[derive(Clone)]
pub struct EndPoint {
    pub locator: Locator,
    pub config: Option<Arc<Properties>>,
}

impl FromStr for EndPoint {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let config_index = s.find(CONFIG_SEPARATOR).unwrap_or_else(|| s.len());
        // Parse the locator
        let locator: Locator = s[0..config_index].parse()?;
        // Parse the config if any
        let config = if config_index < s.len() {
            let c = &s[config_index + 1..];
            // If the metadata is after the configuration, it's an invalid locator
            if c.find(METADATA_SEPARATOR).is_some() {
                bail!("Invalid endpoint format: {}", s);
            }
            let config = str_to_properties(c)
                .ok_or_else(|| zerror!("Invalid endpoint configuration: {}", s))?;
            Some(Arc::new(config))
        } else {
            None
        };

        let endpoint = EndPoint { locator, config };
        Ok(endpoint)
    }
}

impl Eq for EndPoint {}

impl PartialEq for EndPoint {
    fn eq(&self, other: &Self) -> bool {
        self.locator == other.locator
            && match self.config.as_ref() {
                Some(sc) => match other.config.as_ref() {
                    Some(oc) => Arc::ptr_eq(sc, oc),
                    None => false,
                },
                None => other.config.is_none(),
            }
    }
}

impl Hash for EndPoint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.locator.hash(state);
        if let Some(config) = self.config.as_ref() {
            Arc::as_ptr(config).hash(state)
        }
    }
}

impl fmt::Display for EndPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.locator)?;
        Ok(())
    }
}

impl fmt::Debug for EndPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.locator)?;
        if let Some(config) = self.config.as_ref() {
            write!(f, "{}", CONFIG_SEPARATOR)?;
            write!(f, "{}", config)?;
        }
        Ok(())
    }
}

/*************************************/
/*        LOCATOR PROTOCOL           */
/*************************************/
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocatorProtocol {
    #[cfg(feature = "transport_tcp")]
    Tcp,
    #[cfg(feature = "transport_udp")]
    Udp,
    #[cfg(feature = "transport_tls")]
    Tls,
    #[cfg(feature = "transport_quic")]
    Quic,
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
            #[cfg(feature = "transport_quic")]
            LocatorProtocol::Quic => write!(f, "{}", STR_QUIC)?,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorProtocol::UnixSocketStream => write!(f, "{}", STR_UNIXSOCK_STREAM)?,
        }
        Ok(())
    }
}

/*************************************/
/*          LOCATOR CONFIG           */
/*************************************/
pub struct LocatorConfig;

impl LocatorConfig {
    pub fn from_config(config: &Config) -> ZResult<HashMap<LocatorProtocol, Properties>> {
        let mut ps: HashMap<LocatorProtocol, Properties> = HashMap::new();
        #[cfg(feature = "transport_tcp")]
        {
            let mut res = LocatorConfigTcp::from_config(config)?;
            if let Some(p) = res.take() {
                ps.insert(LocatorProtocol::Tcp, p);
            }
        }
        #[cfg(feature = "transport_udp")]
        {
            let mut res = LocatorConfigUdp::from_config(config)?;
            if let Some(p) = res.take() {
                ps.insert(LocatorProtocol::Udp, p);
            }
        }
        #[cfg(feature = "transport_tls")]
        {
            let mut res = LocatorConfigTls::from_config(config)?;
            if let Some(p) = res.take() {
                ps.insert(LocatorProtocol::Tls, p);
            }
        }
        #[cfg(feature = "transport_quic")]
        {
            let mut res = LocatorConfigQuic::from_config(config)?;
            if let Some(p) = res.take() {
                ps.insert(LocatorProtocol::Quic, p);
            }
        }
        #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
        {
            let mut res = LocatorConfigUnixSocketStream::from_config(config)?;
            if let Some(p) = res.take() {
                ps.insert(LocatorProtocol::UnixSocketStream, p);
            }
        }
        Ok(ps)
    }
}

/*************************************/
/*             LOCATOR               */
/*************************************/
#[derive(Clone, Debug)]
pub struct Locator {
    pub address: LocatorAddress,
    pub metadata: Option<Arc<Properties>>,
}

impl Eq for Locator {}

impl PartialEq for Locator {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
            && match self.metadata.as_ref() {
                Some(sc) => match other.metadata.as_ref() {
                    Some(oc) => Arc::ptr_eq(sc, oc),
                    None => false,
                },
                None => other.metadata.is_none(),
            }
    }
}

impl Hash for Locator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.address.hash(state);
        if let Some(metadata) = self.metadata.as_ref() {
            Arc::as_ptr(metadata).hash(state)
        }
    }
}

impl FromStr for Locator {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let metadata_index = s.find(METADATA_SEPARATOR).unwrap_or_else(|| s.len());

        // Parse the locator
        let address: LocatorAddress = s[0..metadata_index].parse()?;
        // Parse the metadata if any
        let metadata = if metadata_index < s.len() {
            let metadata = str_to_properties(&s[metadata_index + 1..])
                .ok_or_else(|| zerror!("Invalid locator metadata: {}", s))?;
            Some(metadata.into())
        } else {
            None
        };

        let locator = Locator { address, metadata };
        Ok(locator)
    }
}

struct LocatorVisitor;
impl<'de> serde::de::Visitor<'de> for LocatorVisitor {
    type Value = Locator;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a locator (ex: tcp/0.0.0.0:7447)")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse().map_err(E::custom)
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(v)
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(&v)
    }
}

impl<'de> serde::Deserialize<'de> for Locator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(LocatorVisitor)
    }
}

impl serde::Serialize for Locator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!("{}", self))
    }
}

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.address)?;
        if let Some(metadata) = self.metadata.as_ref() {
            write!(f, "{}", METADATA_SEPARATOR)?;
            write!(f, "{}", properties_to_str(metadata))?;
        }
        Ok(())
    }
}

/*************************************/
/*          LOCATOR ADDRESS          */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocatorAddress {
    #[cfg(feature = "transport_tcp")]
    Tcp(LocatorTcp),
    #[cfg(feature = "transport_udp")]
    Udp(LocatorUdp),
    #[cfg(feature = "transport_tls")]
    Tls(LocatorTls),
    #[cfg(feature = "transport_quic")]
    Quic(LocatorQuic),
    #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
    UnixSocketStream(LocatorUnixSocketStream),
}

impl FromStr for LocatorAddress {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let proto_index = s
            .find(PROTO_SEPARATOR)
            .ok_or_else(|| zerror!("Invalid locator address format: {}. Missing protocol.", s))?;
        let (proto, addr) = s.split_at(proto_index);
        let addr = &addr[1..];

        match proto {
            #[cfg(feature = "transport_tcp")]
            STR_TCP => Ok(LocatorAddress::Tcp(addr.parse()?)),
            #[cfg(feature = "transport_udp")]
            STR_UDP => Ok(LocatorAddress::Udp(addr.parse()?)),
            #[cfg(feature = "transport_tls")]
            STR_TLS => Ok(LocatorAddress::Tls(addr.parse()?)),
            #[cfg(feature = "transport_quic")]
            STR_QUIC => Ok(LocatorAddress::Quic(addr.parse()?)),
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            STR_UNIXSOCK_STREAM => Ok(LocatorAddress::UnixSocketStream(addr.parse()?)),
            unknown => {
                bail!(
                    "Invalid locator address: {}. Unknown protocol: {}.",
                    s,
                    unknown
                );
            }
        }
    }
}

impl LocatorAddress {
    pub fn get_proto(&self) -> LocatorProtocol {
        match self {
            #[cfg(feature = "transport_tcp")]
            LocatorAddress::Tcp(..) => LocatorProtocol::Tcp,
            #[cfg(feature = "transport_udp")]
            LocatorAddress::Udp(..) => LocatorProtocol::Udp,
            #[cfg(feature = "transport_tls")]
            LocatorAddress::Tls(..) => LocatorProtocol::Tls,
            #[cfg(feature = "transport_quic")]
            LocatorAddress::Quic(..) => LocatorProtocol::Quic,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorAddress::UnixSocketStream(..) => LocatorProtocol::UnixSocketStream,
        }
    }

    pub fn is_multicast(&self) -> bool {
        match self {
            #[cfg(feature = "transport_tcp")]
            LocatorAddress::Tcp(l) => l.is_multicast(),
            #[cfg(feature = "transport_udp")]
            LocatorAddress::Udp(l) => l.is_multicast(),
            #[cfg(feature = "transport_tls")]
            LocatorAddress::Tls(l) => l.is_multicast(),
            #[cfg(feature = "transport_quic")]
            LocatorAddress::Quic(l) => l.is_multicast(),
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorAddress::UnixSocketStream(l) => l.is_multicast(),
        }
    }
}

impl fmt::Display for LocatorAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "transport_tcp")]
            LocatorAddress::Tcp(addr) => write!(f, "{}{}{}", STR_TCP, PROTO_SEPARATOR, addr)?,
            #[cfg(feature = "transport_udp")]
            LocatorAddress::Udp(addr) => write!(f, "{}{}{}", STR_UDP, PROTO_SEPARATOR, addr)?,
            #[cfg(feature = "transport_tls")]
            LocatorAddress::Tls(addr) => write!(f, "{}{}{}", STR_TLS, PROTO_SEPARATOR, addr)?,
            #[cfg(feature = "transport_quic")]
            LocatorAddress::Quic(addr) => write!(f, "{}{}{}", STR_QUIC, PROTO_SEPARATOR, addr)?,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorAddress::UnixSocketStream(addr) => {
                write!(f, "{}{}{}", STR_UNIXSOCK_STREAM, PROTO_SEPARATOR, addr)?
            }
        }
        Ok(())
    }
}
