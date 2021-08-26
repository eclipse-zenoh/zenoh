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
use super::{Locator, LocatorAddress, LocatorProperty, ALPN_QUIC_HTTP};
use async_std::fs;
use async_std::net::{SocketAddr, ToSocketAddrs};
use quinn::{
    Certificate, CertificateChain, ClientConfigBuilder, PrivateKey, ServerConfig,
    ServerConfigBuilder, TransportConfig,
};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use webpki::{DnsName, DnsNameRef};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::*;

#[allow(unreachable_patterns)]
pub(super) async fn get_quic_addr(locator: &Locator) -> ZResult<SocketAddr> {
    match &locator.address {
        LocatorAddress::Quic(addr) => match addr {
            LocatorQuic::SocketAddr(addr) => Ok(*addr),
            LocatorQuic::DnsName(addr) => match addr.to_socket_addrs().await {
                Ok(mut addr_iter) => {
                    if let Some(addr) = addr_iter.next() {
                        Ok(addr)
                    } else {
                        let e = format!("Couldn't resolve QUIC locator: {}", addr);
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
            let e = format!("Not a QUIC locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
pub(super) async fn get_quic_dns(locator: &Locator) -> ZResult<DnsName> {
    match &locator.address {
        LocatorAddress::Quic(addr) => match addr {
            LocatorQuic::SocketAddr(addr) => {
                let e = format!("Couldn't get domain from SocketAddr: {}", addr);
                zerror!(ZErrorKind::InvalidLocator { descr: e })
            }
            LocatorQuic::DnsName(addr) => {
                // Separate the domain from the port.
                // E.g. zenoh.io:7447 returns (zenoh.io, 7447).
                let split: Vec<&str> = addr.split(':').collect();
                match split.get(0) {
                    Some(dom) => {
                        let domain = DnsNameRef::try_from_ascii_str(dom).map_err(|e| {
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
            let e = format!("Not a QUIC locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
pub(super) fn get_quic_prop(property: &LocatorProperty) -> ZResult<&LocatorPropertyQuic> {
    match property {
        LocatorProperty::Quic(prop) => Ok(prop),
        _ => {
            let e = "Not a QUIC property".to_string();
            log::debug!("{}", e);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
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
    type Err = ZError;

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
/*            PROPERTY               */
/*************************************/
#[derive(Clone)]
pub struct LocatorPropertyQuic {
    pub client: Option<ClientConfigBuilder>,
    pub server: Option<ServerConfigBuilder>,
}

impl LocatorPropertyQuic {
    pub fn new(
        client: Option<ClientConfigBuilder>,
        server: Option<ServerConfigBuilder>,
    ) -> LocatorPropertyQuic {
        LocatorPropertyQuic { client, server }
    }

    pub async fn from_properties(config: &ConfigProperties) -> ZResult<Option<LocatorProperty>> {
        let mut client_config: Option<ClientConfigBuilder> = None;
        if let Some(tls_ca_certificate) = config.get(&ZN_TLS_ROOT_CA_CERTIFICATE_KEY) {
            let ca = fs::read(tls_ca_certificate).await.map_err(|e| {
                let e = format!("Invalid QUIC CA certificate file: {}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })?;
            let ca = Certificate::from_pem(&ca).map_err(|e| {
                let e = format!("Invalid QUIC CA certificate file: {}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })?;

            let mut cc = ClientConfigBuilder::default();
            cc.protocols(ALPN_QUIC_HTTP);
            cc.add_certificate_authority(ca).map_err(|e| {
                let e = format!("Invalid QUIC CA certificate file: {}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })?;

            client_config = Some(cc);
            log::debug!("QUIC client is configured");
        }

        let mut server_config: Option<ServerConfigBuilder> = None;
        if let Some(tls_server_private_key) = config.get(&ZN_TLS_SERVER_PRIVATE_KEY_KEY) {
            if let Some(tls_server_certificate) = config.get(&ZN_TLS_SERVER_CERTIFICATE_KEY) {
                let pkey = fs::read(tls_server_private_key).await.map_err(|e| {
                    let e = format!("Invalid TLS private key file: {}", e);
                    zerror2!(ZErrorKind::IoError { descr: e })
                })?;
                let keys = PrivateKey::from_pem(&pkey).unwrap();

                let certs = fs::read(tls_server_certificate).await.map_err(|e| {
                    let e = format!("Invalid TLS server certificate file: {}", e);
                    zerror2!(ZErrorKind::IoError { descr: e })
                })?;
                let certs = CertificateChain::from_pem(&certs).map_err(|e| {
                    let e = format!("Invalid TLS server certificate file: {}", e);
                    zerror2!(ZErrorKind::IoError { descr: e })
                })?;

                let mut tc = TransportConfig::default();
                // We do not accept unidireactional streams.
                tc.max_concurrent_uni_streams(0).map_err(|e| {
                    let e = format!("Invalid QUIC server configuration: {}", e);
                    zerror2!(ZErrorKind::IoError { descr: e })
                })?;
                // For the time being we only allow one bidirectional stream
                tc.max_concurrent_bidi_streams(1).map_err(|e| {
                    let e = format!("Invalid QUIC server configuration: {}", e);
                    zerror2!(ZErrorKind::IoError { descr: e })
                })?;
                let mut sc = ServerConfig::default();
                sc.transport = Arc::new(tc);
                let mut sc = ServerConfigBuilder::new(sc);
                sc.protocols(ALPN_QUIC_HTTP);
                sc.certificate(certs, keys).map_err(|e| {
                    let e = format!("Invalid TLS server configuration: {}", e);
                    zerror2!(ZErrorKind::Other { descr: e })
                })?;

                server_config = Some(sc);
                log::debug!("QUIC server is configured");
            }
        }

        if client_config.is_none() && server_config.is_none() {
            Ok(None)
        } else {
            Ok(Some((client_config, server_config).into()))
        }
    }
}

impl From<LocatorPropertyQuic> for LocatorProperty {
    fn from(property: LocatorPropertyQuic) -> LocatorProperty {
        LocatorProperty::Quic(property)
    }
}

impl From<ClientConfigBuilder> for LocatorProperty {
    fn from(client: ClientConfigBuilder) -> LocatorProperty {
        Self::from(LocatorPropertyQuic::new(Some(client), None))
    }
}

impl From<ServerConfigBuilder> for LocatorProperty {
    fn from(server: ServerConfigBuilder) -> LocatorProperty {
        Self::from(LocatorPropertyQuic::new(None, Some(server)))
    }
}

impl From<(Option<ClientConfigBuilder>, Option<ServerConfigBuilder>)> for LocatorProperty {
    fn from(tuple: (Option<ClientConfigBuilder>, Option<ServerConfigBuilder>)) -> LocatorProperty {
        Self::from(LocatorPropertyQuic::new(tuple.0, tuple.1))
    }
}

impl From<(Option<ServerConfigBuilder>, Option<ClientConfigBuilder>)> for LocatorProperty {
    fn from(tuple: (Option<ServerConfigBuilder>, Option<ClientConfigBuilder>)) -> LocatorProperty {
        Self::from(LocatorPropertyQuic::new(tuple.1, tuple.0))
    }
}

impl From<(ServerConfigBuilder, ClientConfigBuilder)> for LocatorProperty {
    fn from(tuple: (ServerConfigBuilder, ClientConfigBuilder)) -> LocatorProperty {
        Self::from((Some(tuple.0), Some(tuple.1)))
    }
}

impl From<(ClientConfigBuilder, ServerConfigBuilder)> for LocatorProperty {
    fn from(tuple: (ClientConfigBuilder, ServerConfigBuilder)) -> LocatorProperty {
        Self::from((Some(tuple.0), Some(tuple.1)))
    }
}
