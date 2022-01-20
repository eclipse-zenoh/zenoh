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
use super::quic::LinkManagerUnicastQuic;
#[cfg(feature = "transport_tcp")]
use super::tcp::LinkManagerUnicastTcp;
#[cfg(feature = "transport_tls")]
use super::tls::LinkManagerUnicastTls;
#[cfg(feature = "transport_udp")]
use super::udp::{LinkManagerMulticastUdp, LinkManagerUnicastUdp};
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
use super::unixsock_stream::LinkManagerUnicastUnixSocketStream;
use super::{EndPoint, LinkMulticast, LinkUnicast, Locator, LocatorProtocol};
use crate::net::transport::TransportManager;
use async_std::sync::Arc;
use async_trait::async_trait;
use zenoh_core::{bail, Result as ZResult};

/*************************************/
/*             UNICAST               */
/*************************************/
pub(crate) type LinkManagerUnicast = Arc<dyn LinkManagerUnicastTrait>;
#[async_trait]
pub(crate) trait LinkManagerUnicastTrait: Send + Sync {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast>;
    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator>;
    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()>;
    fn get_listeners(&self) -> Vec<EndPoint>;
    fn get_locators(&self) -> Vec<Locator>;
}

pub(crate) struct LinkManagerBuilderUnicast;

impl LinkManagerBuilderUnicast {
    pub(crate) fn make(
        manager: TransportManager,
        protocol: &LocatorProtocol,
    ) -> ZResult<LinkManagerUnicast> {
        match protocol {
            #[cfg(feature = "transport_tcp")]
            LocatorProtocol::Tcp => Ok(Arc::new(LinkManagerUnicastTcp::new(manager))),
            #[cfg(feature = "transport_udp")]
            LocatorProtocol::Udp => Ok(Arc::new(LinkManagerUnicastUdp::new(manager))),
            #[cfg(feature = "transport_tls")]
            LocatorProtocol::Tls => Ok(Arc::new(LinkManagerUnicastTls::new(manager))),
            #[cfg(feature = "transport_quic")]
            LocatorProtocol::Quic => Ok(Arc::new(LinkManagerUnicastQuic::new(manager))),
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorProtocol::UnixSocketStream => {
                Ok(Arc::new(LinkManagerUnicastUnixSocketStream::new(manager)))
            }
        }
    }
}

/*************************************/
/*            MULTICAST              */
/*************************************/
#[async_trait]
pub(crate) trait LinkManagerMulticastTrait: Send + Sync {
    async fn new_link(&self, endpoint: &EndPoint) -> ZResult<LinkMulticast>;
}

pub(crate) type LinkManagerMulticast = Arc<dyn LinkManagerMulticastTrait>;

pub(crate) struct LinkManagerBuilderMulticast;

impl LinkManagerBuilderMulticast {
    pub(crate) fn make(protocol: &LocatorProtocol) -> ZResult<LinkManagerMulticast> {
        match protocol {
            #[cfg(feature = "transport_udp")]
            LocatorProtocol::Udp => Ok(Arc::new(LinkManagerMulticastUdp::default())),
            _ => bail!("Multicast not supported for {} protocol", protocol),
        }
    }
}
