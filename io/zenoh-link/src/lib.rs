//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use std::collections::HashMap;

use zenoh_config::Config;
pub use zenoh_link_commons::*;
#[cfg(feature = "transport_quic")]
pub use zenoh_link_quic as quic;
#[cfg(feature = "transport_quic")]
use zenoh_link_quic::{
    LinkManagerUnicastQuic, QuicConfigurator, QuicLocatorInspector, QUIC_LOCATOR_PREFIX,
};
#[cfg(feature = "transport_quic_datagram")]
pub use zenoh_link_quic_datagram as quic_datagram;
#[cfg(all(feature = "transport_quic_datagram", not(feature = "transport_quic")))]
use zenoh_link_quic_datagram::QUIC_DATAGRAM_LOCATOR_PREFIX;
#[cfg(feature = "transport_quic_datagram")]
use zenoh_link_quic_datagram::{
    LinkManagerUnicastQuicDatagram, QuicDatagramConfigurator, QuicDatagramLocatorInspector,
};
#[cfg(feature = "transport_serial")]
pub use zenoh_link_serial as serial;
#[cfg(feature = "transport_serial")]
use zenoh_link_serial::{LinkManagerUnicastSerial, SerialLocatorInspector, SERIAL_LOCATOR_PREFIX};
#[cfg(feature = "transport_tcp")]
pub use zenoh_link_tcp as tcp;
#[cfg(feature = "transport_tcp")]
use zenoh_link_tcp::{
    LinkManagerUnicastTcp, TcpConfigurator, TcpLocatorInspector, TCP_LOCATOR_PREFIX,
};
#[cfg(feature = "transport_tls")]
pub use zenoh_link_tls as tls;
#[cfg(feature = "transport_tls")]
use zenoh_link_tls::{
    LinkManagerUnicastTls, TlsConfigurator, TlsLocatorInspector, TLS_LOCATOR_PREFIX,
};
#[cfg(feature = "transport_udp")]
pub use zenoh_link_udp as udp;
#[cfg(feature = "transport_udp")]
use zenoh_link_udp::{
    LinkManagerMulticastUdp, LinkManagerUnicastUdp, UdpLocatorInspector, UDP_LOCATOR_PREFIX,
};
#[cfg(feature = "transport_unixpipe")]
pub use zenoh_link_unixpipe as unixpipe;
#[cfg(feature = "transport_unixpipe")]
use zenoh_link_unixpipe::{
    LinkManagerUnicastPipe, UnixPipeConfigurator, UnixPipeLocatorInspector, UNIXPIPE_LOCATOR_PREFIX,
};
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
pub use zenoh_link_unixsock_stream as unixsock_stream;
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
use zenoh_link_unixsock_stream::{
    LinkManagerUnicastUnixSocketStream, UnixSockStreamLocatorInspector,
    UNIXSOCKSTREAM_LOCATOR_PREFIX,
};
#[cfg(all(feature = "transport_vsock", target_os = "linux"))]
pub use zenoh_link_vsock as vsock;
#[cfg(all(feature = "transport_vsock", target_os = "linux"))]
use zenoh_link_vsock::{LinkManagerUnicastVsock, VsockLocatorInspector, VSOCK_LOCATOR_PREFIX};
#[cfg(feature = "transport_ws")]
pub use zenoh_link_ws as ws;
#[cfg(feature = "transport_ws")]
use zenoh_link_ws::{LinkManagerUnicastWs, WsLocatorInspector, WS_LOCATOR_PREFIX};
pub use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{bail, ZResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkKind {
    Quic,
    QuicDatagram,
    Serial,
    Tcp,
    Tls,
    Udp,
    Unixpipe,
    UnixsockStream,
    Vscock,
    Ws,
}

impl LinkKind {
    pub fn new_supported_links<'p>(protocols: impl Iterator<Item = &'p str>) -> Vec<LinkKind> {
        #[allow(unused_mut)]
        let mut supported_links = Vec::new();
        for p in protocols {
            match p {
                #[cfg(feature = "transport_tcp")]
                TCP_LOCATOR_PREFIX => supported_links.push(LinkKind::Tcp),
                #[cfg(feature = "transport_udp")]
                UDP_LOCATOR_PREFIX => supported_links.push(LinkKind::Udp),
                #[cfg(feature = "transport_tls")]
                TLS_LOCATOR_PREFIX => supported_links.push(LinkKind::Tls),
                #[cfg(all(feature = "transport_quic_datagram", not(feature = "transport_quic")))]
                QUIC_DATAGRAM_LOCATOR_PREFIX => supported_links.push(LinkKind::QuicDatagram),
                #[cfg(all(feature = "transport_quic", not(feature = "transport_quic_datagram")))]
                QUIC_LOCATOR_PREFIX => supported_links.push(LinkKind::QuicDatagram),
                #[cfg(all(feature = "transport_quic", feature = "transport_quic_datagram"))]
                QUIC_LOCATOR_PREFIX => {
                    supported_links.push(LinkKind::Quic);
                    supported_links.push(LinkKind::QuicDatagram);
                }
                #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
                UNIXSOCKSTREAM_LOCATOR_PREFIX => supported_links.push(LinkKind::UnixsockStream),
                #[cfg(feature = "transport_ws")]
                WS_LOCATOR_PREFIX => supported_links.push(LinkKind::Ws),
                #[cfg(feature = "transport_serial")]
                SERIAL_LOCATOR_PREFIX => supported_links.push(LinkKind::Serial),
                #[cfg(feature = "transport_unixpipe")]
                UNIXPIPE_LOCATOR_PREFIX => supported_links.push(LinkKind::Unixpipe),
                #[cfg(all(feature = "transport_vsock", target_os = "linux"))]
                VSOCK_LOCATOR_PREFIX => supported_links.push(LinkKind::Vscock),
                _ => {}
            }
        }
        supported_links
    }
}

impl TryFrom<&Locator> for LinkKind {
    type Error = zenoh_result::Error;

    fn try_from(locator: &Locator) -> Result<Self, Self::Error> {
        #[allow(unused_imports)]
        use zenoh_link_commons::LocatorInspector;
        match locator.protocol().as_str() {
            #[cfg(feature = "transport_tcp")]
            TCP_LOCATOR_PREFIX => Ok(LinkKind::Tcp),
            #[cfg(feature = "transport_udp")]
            UDP_LOCATOR_PREFIX => Ok(LinkKind::Udp),
            #[cfg(feature = "transport_tls")]
            TLS_LOCATOR_PREFIX => Ok(LinkKind::Tls),
            #[cfg(all(feature = "transport_quic_datagram", not(feature = "transport_quic")))]
            QUIC_DATAGRAM_LOCATOR_PREFIX => {
                if !QuicDatagramLocatorInspector.is_reliable(locator)? {
                    Ok(LinkKind::QuicDatagram)
                } else {
                    Err(zenoh_result::zerror!("Attempted to use a reliable QUIC link without enabling the transport_quic feature").into())
                }
            }
            #[cfg(all(feature = "transport_quic", not(feature = "transport_quic_datagram")))]
            QUIC_LOCATOR_PREFIX => {
                if QuicLocatorInspector.is_reliable(locator)? {
                    Ok(LinkKind::Quic)
                } else {
                    Err(zenoh_result::zerror!("Cannot use unreliable QUIC without enabling the transport_quic_datagram feature").into())
                }
            }
            #[cfg(all(feature = "transport_quic", feature = "transport_quic_datagram"))]
            QUIC_LOCATOR_PREFIX => {
                if QuicLocatorInspector.is_reliable(locator)? {
                    Ok(LinkKind::Quic)
                } else {
                    Ok(LinkKind::QuicDatagram)
                }
            }
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            UNIXSOCKSTREAM_LOCATOR_PREFIX => Ok(LinkKind::UnixsockStream),
            #[cfg(feature = "transport_ws")]
            WS_LOCATOR_PREFIX => Ok(LinkKind::Ws),
            #[cfg(feature = "transport_serial")]
            SERIAL_LOCATOR_PREFIX => Ok(LinkKind::Serial),
            #[cfg(feature = "transport_unixpipe")]
            UNIXPIPE_LOCATOR_PREFIX => Ok(LinkKind::Unixpipe),
            #[cfg(all(feature = "transport_vsock", target_os = "linux"))]
            VSOCK_LOCATOR_PREFIX => Ok(LinkKind::Vscock),
            _ => bail!(
                "Unicast not supported for {} protocol",
                locator.protocol().as_str()
            ),
        }
    }
}

impl TryFrom<&EndPoint> for LinkKind {
    type Error = zenoh_result::Error;

    fn try_from(endpoint: &EndPoint) -> Result<Self, Self::Error> {
        LinkKind::try_from(&endpoint.to_locator())
    }
}

pub const ALL_SUPPORTED_LINKS: &[LinkKind] = &[
    #[cfg(feature = "transport_quic")]
    LinkKind::Quic,
    #[cfg(feature = "transport_quic_datagram")]
    LinkKind::QuicDatagram,
    #[cfg(feature = "transport_tcp")]
    LinkKind::Tcp,
    #[cfg(feature = "transport_tls")]
    LinkKind::Tls,
    #[cfg(feature = "transport_udp")]
    LinkKind::Udp,
    #[cfg(feature = "transport_ws")]
    LinkKind::Ws,
    #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
    LinkKind::UnixsockStream,
    #[cfg(feature = "transport_serial")]
    LinkKind::Serial,
    #[cfg(feature = "transport_unixpipe")]
    LinkKind::Unixpipe,
    #[cfg(all(feature = "transport_vsock", target_os = "linux"))]
    LinkKind::Vscock,
];

#[derive(Default, Clone)]
pub struct LocatorInspector {
    #[cfg(feature = "transport_quic")]
    quic_inspector: QuicLocatorInspector,
    #[cfg(feature = "transport_quic_datagram")]
    quic_datagram_inspector: QuicDatagramLocatorInspector,
    #[cfg(feature = "transport_tcp")]
    tcp_inspector: TcpLocatorInspector,
    #[cfg(feature = "transport_tls")]
    tls_inspector: TlsLocatorInspector,
    #[cfg(feature = "transport_udp")]
    udp_inspector: UdpLocatorInspector,
    #[cfg(feature = "transport_ws")]
    ws_inspector: WsLocatorInspector,
    #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
    unixsock_stream_inspector: UnixSockStreamLocatorInspector,
    #[cfg(feature = "transport_serial")]
    serial_inspector: SerialLocatorInspector,
    #[cfg(feature = "transport_unixpipe")]
    unixpipe_inspector: UnixPipeLocatorInspector,
    #[cfg(all(feature = "transport_vsock", target_os = "linux"))]
    vsock_inspector: VsockLocatorInspector,
}
impl LocatorInspector {
    pub fn is_reliable(&self, locator: &Locator) -> ZResult<bool> {
        #[allow(unused_imports)]
        use zenoh_link_commons::LocatorInspector;
        match LinkKind::try_from(locator)? {
            #[cfg(feature = "transport_tcp")]
            LinkKind::Tcp => self.tcp_inspector.is_reliable(locator),
            #[cfg(feature = "transport_udp")]
            LinkKind::Udp => self.udp_inspector.is_reliable(locator),
            #[cfg(feature = "transport_tls")]
            LinkKind::Tls => self.tls_inspector.is_reliable(locator),
            #[cfg(feature = "transport_quic")]
            LinkKind::Quic => self.quic_inspector.is_reliable(locator),
            #[cfg(feature = "transport_quic_datagram")]
            LinkKind::QuicDatagram => self.quic_datagram_inspector.is_reliable(locator),
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LinkKind::UnixsockStream => self.unixsock_stream_inspector.is_reliable(locator),
            #[cfg(feature = "transport_ws")]
            LinkKind::Ws => self.ws_inspector.is_reliable(locator),
            #[cfg(feature = "transport_serial")]
            LinkKind::Serial => self.serial_inspector.is_reliable(locator),
            #[cfg(feature = "transport_unixpipe")]
            LinkKind::Unixpipe => self.unixpipe_inspector.is_reliable(locator),
            #[cfg(all(feature = "transport_vsock", target_os = "linux"))]
            LinkKind::Vscock => self.vsock_inspector.is_reliable(locator),
            #[allow(unreachable_patterns)]
            _ => unreachable!(),
        }
    }

    pub async fn is_multicast(&self, locator: &Locator) -> ZResult<bool> {
        #[allow(unused_imports)]
        use zenoh_link_commons::LocatorInspector;
        match LinkKind::try_from(locator)? {
            #[cfg(feature = "transport_tcp")]
            LinkKind::Tcp => self.tcp_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_udp")]
            LinkKind::Udp => self.udp_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_tls")]
            LinkKind::Tls => self.tls_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_quic")]
            LinkKind::Quic => self.quic_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_quic_datagram")]
            LinkKind::QuicDatagram => self.quic_datagram_inspector.is_multicast(locator).await,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LinkKind::UnixsockStream => self.unixsock_stream_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_ws")]
            LinkKind::Ws => self.ws_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_serial")]
            LinkKind::Serial => self.serial_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_unixpipe")]
            LinkKind::Unixpipe => self.unixpipe_inspector.is_multicast(locator).await,
            #[cfg(all(feature = "transport_vsock", target_os = "linux"))]
            LinkKind::Vscock => self.vsock_inspector.is_multicast(locator).await,
            #[allow(unreachable_patterns)]
            _ => unreachable!(),
        }
    }
}
#[derive(Default)]
pub struct LinkConfigurator {
    #[cfg(feature = "transport_tcp")]
    tcp_inspector: TcpConfigurator,
    #[cfg(feature = "transport_quic_datagram")]
    quic_datagram_inspector: QuicDatagramConfigurator,
    #[cfg(feature = "transport_quic")]
    quic_inspector: QuicConfigurator,
    #[cfg(feature = "transport_tls")]
    tls_inspector: TlsConfigurator,
    #[cfg(feature = "transport_unixpipe")]
    unixpipe_inspector: UnixPipeConfigurator,
}

impl LinkConfigurator {
    #[allow(unused_variables, unused_mut)]
    pub fn configurations(
        &self,
        config: &Config,
    ) -> (
        HashMap<LinkKind, String>,
        HashMap<LinkKind, zenoh_result::Error>,
    ) {
        let mut configs = HashMap::<LinkKind, String>::new();
        let mut errors = HashMap::<LinkKind, zenoh_result::Error>::new();
        let mut insert_config = |kind: LinkKind, cfg: ZResult<String>| match cfg {
            Ok(v) => {
                configs.insert(kind, v);
            }
            Err(e) => {
                errors.insert(kind, e);
            }
        };
        #[cfg(feature = "transport_tcp")]
        {
            insert_config(LinkKind::Tcp, self.tcp_inspector.inspect_config(config));
        }
        #[cfg(feature = "transport_quic_datagram")]
        {
            insert_config(
                LinkKind::QuicDatagram,
                self.quic_datagram_inspector.inspect_config(config),
            );
        }
        #[cfg(feature = "transport_quic")]
        {
            insert_config(LinkKind::Quic, self.quic_inspector.inspect_config(config));
        }
        #[cfg(feature = "transport_tls")]
        {
            insert_config(LinkKind::Tls, self.tls_inspector.inspect_config(config));
        }
        #[cfg(feature = "transport_unixpipe")]
        {
            insert_config(
                LinkKind::Unixpipe,
                self.unixpipe_inspector.inspect_config(config),
            );
        }
        (configs, errors)
    }
}

/*************************************/
/*             UNICAST               */
/*************************************/

pub struct LinkManagerBuilderUnicast;

impl LinkManagerBuilderUnicast {
    pub fn make(
        _manager: NewLinkChannelSender,
        endpoint: &EndPoint,
    ) -> ZResult<LinkManagerUnicast> {
        #[allow(unused_imports)]
        use zenoh_link_commons::LocatorInspector;
        match LinkKind::try_from(endpoint)? {
            #[cfg(feature = "transport_tcp")]
            LinkKind::Tcp => Ok(std::sync::Arc::new(LinkManagerUnicastTcp::new(_manager))),
            #[cfg(feature = "transport_udp")]
            LinkKind::Udp => Ok(std::sync::Arc::new(LinkManagerUnicastUdp::new(_manager))),
            #[cfg(feature = "transport_tls")]
            LinkKind::Tls => Ok(std::sync::Arc::new(LinkManagerUnicastTls::new(_manager))),
            #[cfg(feature = "transport_quic_datagram")]
            LinkKind::QuicDatagram => Ok(std::sync::Arc::new(LinkManagerUnicastQuicDatagram::new(
                _manager,
            ))),
            #[cfg(feature = "transport_quic")]
            LinkKind::Quic => Ok(std::sync::Arc::new(LinkManagerUnicastQuic::new(_manager))),
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LinkKind::UnixsockStream => Ok(std::sync::Arc::new(
                LinkManagerUnicastUnixSocketStream::new(_manager),
            )),
            #[cfg(feature = "transport_ws")]
            LinkKind::Ws => Ok(std::sync::Arc::new(LinkManagerUnicastWs::new(_manager))),
            #[cfg(feature = "transport_serial")]
            LinkKind::Serial => Ok(std::sync::Arc::new(LinkManagerUnicastSerial::new(_manager))),
            #[cfg(feature = "transport_unixpipe")]
            LinkKind::Unixpipe => Ok(std::sync::Arc::new(LinkManagerUnicastPipe::new(_manager))),
            #[cfg(all(feature = "transport_vsock", target_os = "linux"))]
            LinkKind::Vscock => Ok(std::sync::Arc::new(LinkManagerUnicastVsock::new(_manager))),
            #[allow(unreachable_patterns)]
            _ => unreachable!(),
        }
    }
}

/*************************************/
/*            MULTICAST              */
/*************************************/

pub struct LinkManagerBuilderMulticast;

impl LinkManagerBuilderMulticast {
    pub fn make(link_kind: LinkKind) -> ZResult<LinkManagerMulticast> {
        match link_kind {
            #[cfg(feature = "transport_udp")]
            LinkKind::Udp => Ok(std::sync::Arc::new(LinkManagerMulticastUdp)),
            _ => bail!("Multicast not supported for link {link_kind:?}"),
        }
    }
}
