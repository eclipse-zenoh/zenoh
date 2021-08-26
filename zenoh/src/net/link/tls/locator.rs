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
use super::{Locator, LocatorAddress, LocatorProperty};
use async_rustls::rustls::internal::pemfile;
pub use async_rustls::rustls::*;
pub use async_rustls::webpki::*;
use async_std::fs;
use async_std::net::{SocketAddr, ToSocketAddrs};
use std::fmt;
use std::io::Cursor;
use std::str::FromStr;
use std::sync::Arc;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::*;
use zenoh_util::{zerror, zerror2};

#[allow(unreachable_patterns)]
pub(super) async fn get_tls_addr(locator: &Locator) -> ZResult<SocketAddr> {
    match &locator.address {
        LocatorAddress::Tls(addr) => match addr {
            LocatorTls::SocketAddr(addr) => Ok(*addr),
            LocatorTls::DnsName(addr) => match addr.to_socket_addrs().await {
                Ok(mut addr_iter) => {
                    if let Some(addr) = addr_iter.next() {
                        Ok(addr)
                    } else {
                        let e = format!("Couldn't resolve TLS locator: {}", addr);
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
            let e = format!("Not a TLS locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
pub(super) async fn get_tls_dns(locator: &Locator) -> ZResult<DNSName> {
    match &locator.address {
        LocatorAddress::Tls(addr) => match addr {
            LocatorTls::SocketAddr(addr) => {
                let e = format!("Couldn't get domain from SocketAddr: {}", addr);
                zerror!(ZErrorKind::InvalidLocator { descr: e })
            }
            LocatorTls::DnsName(addr) => {
                // Separate the domain from the port.
                // E.g. zenoh.io:7447 returns (zenoh.io, 7447).
                let split: Vec<&str> = addr.split(':').collect();
                match split.get(0) {
                    Some(dom) => {
                        let domain = DNSNameRef::try_from_ascii_str(dom).map_err(|e| {
                            let e = e.to_string();
                            zerror2!(ZErrorKind::InvalidLocator { descr: e })
                        })?;
                        Ok(domain.to_owned())
                    }
                    None => {
                        let e = format!("Couldn't get domain for: {}", addr);
                        zerror!(ZErrorKind::InvalidLocator { descr: e })
                    }
                }
            }
        },
        _ => {
            let e = format!("Not a TLS locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
pub(super) fn get_tls_prop(property: &LocatorProperty) -> ZResult<&LocatorPropertyTls> {
    match property {
        LocatorProperty::Tls(prop) => Ok(prop),
        _ => {
            let e = "Not a TLS property".to_string();
            log::debug!("{}", e);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
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
    type Err = ZError;

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

#[derive(Clone)]
pub struct LocatorPropertyTls {
    pub client: Option<Arc<ClientConfig>>,
    pub server: Option<Arc<ServerConfig>>,
}

impl LocatorPropertyTls {
    pub fn new(
        client: Option<Arc<ClientConfig>>,
        server: Option<Arc<ServerConfig>>,
    ) -> LocatorPropertyTls {
        LocatorPropertyTls { client, server }
    }

    pub async fn from_properties(config: &ConfigProperties) -> ZResult<Option<LocatorProperty>> {
        let mut client_config: Option<ClientConfig> = None;
        if let Some(tls_ca_certificate) = config.get(&ZN_TLS_ROOT_CA_CERTIFICATE_KEY) {
            let ca = fs::read(tls_ca_certificate).await.map_err(|e| {
                zerror2!(ZErrorKind::Other {
                    descr: format!("Invalid TLS CA certificate file: {}", e)
                })
            })?;
            let mut cc = ClientConfig::new();
            let _ = cc
                .root_store
                .add_pem_file(&mut Cursor::new(ca))
                .map_err(|_| {
                    zerror2!(ZErrorKind::Other {
                        descr: "Invalid TLS CA certificate file".to_string()
                    })
                })?;
            client_config = Some(cc);
            log::debug!("TLS client is configured");
        }

        let mut server_config: Option<ServerConfig> = None;
        if let Some(tls_server_private_key) = config.get(&ZN_TLS_SERVER_PRIVATE_KEY_KEY) {
            if let Some(tls_server_certificate) = config.get(&ZN_TLS_SERVER_CERTIFICATE_KEY) {
                let pkey = fs::read(tls_server_private_key).await.map_err(|e| {
                    zerror2!(ZErrorKind::Other {
                        descr: format!("Invalid TLS private key file: {}", e)
                    })
                })?;
                let mut keys = pemfile::rsa_private_keys(&mut Cursor::new(pkey)).unwrap();

                let cert = fs::read(tls_server_certificate).await.map_err(|e| {
                    zerror2!(ZErrorKind::Other {
                        descr: format!("Invalid TLS server certificate file: {}", e)
                    })
                })?;
                let certs = pemfile::certs(&mut Cursor::new(cert)).unwrap();

                let mut sc = ServerConfig::new(NoClientAuth::new());
                sc.set_single_cert(certs, keys.remove(0)).unwrap();
                server_config = Some(sc);
                log::debug!("TLS server is configured");
            }
        }

        if client_config.is_none() && server_config.is_none() {
            Ok(None)
        } else {
            Ok(Some((client_config, server_config).into()))
        }
    }
}

impl From<LocatorPropertyTls> for LocatorProperty {
    fn from(property: LocatorPropertyTls) -> LocatorProperty {
        LocatorProperty::Tls(property)
    }
}

impl From<(Arc<ClientConfig>, Arc<ServerConfig>)> for LocatorProperty {
    fn from(tuple: (Arc<ClientConfig>, Arc<ServerConfig>)) -> LocatorProperty {
        Self::from(LocatorPropertyTls::new(Some(tuple.0), Some(tuple.1)))
    }
}

impl From<(ClientConfig, ServerConfig)> for LocatorProperty {
    fn from(tuple: (ClientConfig, ServerConfig)) -> LocatorProperty {
        Self::from((Arc::new(tuple.0), Arc::new(tuple.1)))
    }
}

impl From<Arc<ClientConfig>> for LocatorProperty {
    fn from(client: Arc<ClientConfig>) -> LocatorProperty {
        Self::from(LocatorPropertyTls::new(Some(client), None))
    }
}

impl From<ClientConfig> for LocatorProperty {
    fn from(client: ClientConfig) -> LocatorProperty {
        Self::from(Arc::new(client))
    }
}

impl From<Arc<ServerConfig>> for LocatorProperty {
    fn from(server: Arc<ServerConfig>) -> LocatorProperty {
        Self::from(LocatorPropertyTls::new(None, Some(server)))
    }
}

impl From<ServerConfig> for LocatorProperty {
    fn from(server: ServerConfig) -> LocatorProperty {
        Self::from(Arc::new(server))
    }
}

impl From<(Option<Arc<ClientConfig>>, Option<Arc<ServerConfig>>)> for LocatorProperty {
    fn from(tuple: (Option<Arc<ClientConfig>>, Option<Arc<ServerConfig>>)) -> LocatorProperty {
        Self::from(LocatorPropertyTls::new(tuple.0, tuple.1))
    }
}

impl From<(Option<Arc<ServerConfig>>, Option<Arc<ClientConfig>>)> for LocatorProperty {
    fn from(tuple: (Option<Arc<ServerConfig>>, Option<Arc<ClientConfig>>)) -> LocatorProperty {
        Self::from(LocatorPropertyTls::new(tuple.1, tuple.0))
    }
}

impl From<(Option<ClientConfig>, Option<ServerConfig>)> for LocatorProperty {
    fn from(mut tuple: (Option<ClientConfig>, Option<ServerConfig>)) -> LocatorProperty {
        let client_config = tuple.0.take().map(Arc::new);
        let server_config = tuple.1.take().map(Arc::new);
        Self::from((client_config, server_config))
    }
}

impl From<(Option<ServerConfig>, Option<ClientConfig>)> for LocatorProperty {
    fn from(mut tuple: (Option<ServerConfig>, Option<ClientConfig>)) -> LocatorProperty {
        let client_config = tuple.1.take().map(Arc::new);
        let server_config = tuple.0.take().map(Arc::new);
        Self::from((client_config, server_config))
    }
}
