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

// #[cfg(feature = "transport_quic")]
// use crate::net::link::quic::LinkManagerUnicastQuic;
// #[cfg(feature = "transport_tcp")]
// use crate::net::link::tcp::LinkManagerUnicastTcp;
// #[cfg(feature = "transport_tls")]
// use crate::net::link::tls::LinkManagerUnicastTls;
// #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
// use crate::net::link::unixsock_stream::LinkManagerUnicastUnixSocketStream;
use zenoh_core::{bail, Result as ZResult};
use zenoh_link_commons::{LinkManagerMulticast, LinkManagerUnicast, NewLinkChannelSender};
#[cfg(feature = "transport_udp")]
use zenoh_link_udp::{LinkManagerMulticastUdp, LinkManagerUnicastUdp, UDP_LOCATOR_PREFIX};

pub use zenoh_link_commons::*;
pub use zenoh_protocol_core::{endpoint, locator, EndPoint, Locator};

/*************************************/
/*             UNICAST               */
/*************************************/

pub struct LinkManagerBuilderUnicast;

impl LinkManagerBuilderUnicast {
    pub fn make(manager: NewLinkChannelSender, protocol: &str) -> ZResult<LinkManagerUnicast> {
        match protocol {
            // #[cfg(feature = "transport_tcp")]
            // LocatorProtocol::Tcp => Ok(Arc::new(LinkManagerUnicastTcp::new(manager))),
            #[cfg(feature = "transport_udp")]
            UDP_LOCATOR_PREFIX => Ok(std::sync::Arc::new(LinkManagerUnicastUdp::new(manager))),
            // #[cfg(feature = "transport_tls")]
            // LocatorProtocol::Tls => Ok(Arc::new(LinkManagerUnicastTls::new(manager))),
            // #[cfg(feature = "transport_quic")]
            // LocatorProtocol::Quic => Ok(Arc::new(LinkManagerUnicastQuic::new(manager))),
            // #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            // LocatorProtocol::UnixSocketStream => {
            //     Ok(Arc::new(LinkManagerUnicastUnixSocketStream::new(manager)))
            // }
            _ => bail!("Unicast not supported for {} protocol", protocol),
        }
    }
}

/*************************************/
/*            MULTICAST              */
/*************************************/

pub struct LinkManagerBuilderMulticast;

impl LinkManagerBuilderMulticast {
    pub fn make(protocol: &str) -> ZResult<LinkManagerMulticast> {
        match protocol {
            #[cfg(feature = "transport_udp")]
            UDP_LOCATOR_PREFIX => Ok(std::sync::Arc::new(LinkManagerMulticastUdp::default())),
            _ => bail!("Multicast not supported for {} protocol", protocol),
        }
    }
}

pub const WBUF_SIZE: usize = 64;
