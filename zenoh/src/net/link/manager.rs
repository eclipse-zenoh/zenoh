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
use super::udp::LinkManagerUnicastUdp;
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
use super::unixsock_stream::LinkManagerUnicastUnixSocketStream;
use super::{Link, Locator, LocatorProperty, LocatorProtocol};
use crate::net::transport::SessionManager;
use async_std::sync::Arc;
use async_trait::async_trait;
use zenoh_util::core::ZResult;

#[async_trait]
pub trait LinkManagerUnicastTrait: Send + Sync {
    async fn new_link(&self, dst: &Locator, property: Option<&LocatorProperty>) -> ZResult<Link>;
    async fn new_listener(
        &self,
        locator: &Locator,
        property: Option<&LocatorProperty>,
    ) -> ZResult<Locator>;
    async fn del_listener(&self, locator: &Locator) -> ZResult<()>;
    fn get_listeners(&self) -> Vec<Locator>;
    fn get_locators(&self) -> Vec<Locator>;
}

pub type LinkManagerUnicast = Arc<dyn LinkManagerUnicastTrait>;

pub struct LinkManagerBuilderUnicast;

impl LinkManagerBuilderUnicast {
    pub(crate) fn make(
        manager: SessionManager,
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
