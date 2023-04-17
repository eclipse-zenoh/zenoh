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
//! [Click here for Zenoh's documentation](../zenoh/index.html)
use std::collections::HashMap;
#[allow(unused_imports)]
use std::sync::Arc;

use zenoh_cfg_properties::Properties;
use zenoh_config::Config;
use zenoh_result::{bail, ZResult};

#[cfg(feature = "transport_tcp")]
pub use zenoh_link_tcp as tcp;
#[cfg(feature = "transport_tcp")]
use zenoh_link_tcp::{LinkManagerUnicastTcp, TcpLocatorInspector, TCP_LOCATOR_PREFIX};

#[cfg(feature = "transport_udp")]
pub use zenoh_link_udp as udp;
#[cfg(feature = "transport_udp")]
use zenoh_link_udp::{
    LinkManagerMulticastUdp, LinkManagerUnicastUdp, UdpLocatorInspector, UDP_LOCATOR_PREFIX,
};

#[cfg(feature = "transport_tls")]
pub use zenoh_link_tls as tls;
#[cfg(feature = "transport_tls")]
use zenoh_link_tls::{
    LinkManagerUnicastTls, TlsConfigurator, TlsLocatorInspector, TLS_LOCATOR_PREFIX,
};

#[cfg(feature = "transport_quic")]
pub use zenoh_link_quic as quic;
#[cfg(feature = "transport_quic")]
use zenoh_link_quic::{
    LinkManagerUnicastQuic, QuicConfigurator, QuicLocatorInspector, QUIC_LOCATOR_PREFIX,
};

#[cfg(feature = "transport_ws")]
pub use zenoh_link_ws as ws;
#[cfg(feature = "transport_ws")]
use zenoh_link_ws::{LinkManagerUnicastWs, WsLocatorInspector, WS_LOCATOR_PREFIX};

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
pub use zenoh_link_unixsock_stream as unixsock_stream;
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
use zenoh_link_unixsock_stream::{
    LinkManagerUnicastUnixSocketStream, UnixSockStreamLocatorInspector,
    UNIXSOCKSTREAM_LOCATOR_PREFIX,
};

#[cfg(feature = "transport_serial")]
pub use zenoh_link_serial as serial;
#[cfg(feature = "transport_serial")]
use zenoh_link_serial::{LinkManagerUnicastSerial, SerialLocatorInspector, SERIAL_LOCATOR_PREFIX};

pub use zenoh_link_commons::*;
pub use zenoh_protocol::core::{EndPoint, Locator};

pub const PROTOCOLS: &[&str] = &[
    #[cfg(feature = "transport_quic")]
    quic::QUIC_LOCATOR_PREFIX,
    #[cfg(feature = "transport_tcp")]
    tcp::TCP_LOCATOR_PREFIX,
    #[cfg(feature = "transport_tls")]
    tls::TLS_LOCATOR_PREFIX,
    #[cfg(feature = "transport_udp")]
    udp::UDP_LOCATOR_PREFIX,
    #[cfg(feature = "transport_ws")]
    ws::WS_LOCATOR_PREFIX,
    #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
    unixsock_stream::UNIXSOCKSTREAM_LOCATOR_PREFIX,
    #[cfg(feature = "transport_serial")]
    serial::SERIAL_LOCATOR_PREFIX,
];

#[derive(Default, Clone)]
pub struct LocatorInspector {
    #[cfg(feature = "transport_quic")]
    quic_inspector: QuicLocatorInspector,
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
}
impl LocatorInspector {
    pub async fn is_multicast(&self, locator: &Locator) -> ZResult<bool> {
        #[allow(unused_imports)]
        use zenoh_link_commons::LocatorInspector;
        let protocol = locator.protocol();
        match protocol.as_str() {
            #[cfg(feature = "transport_tcp")]
            TCP_LOCATOR_PREFIX => self.tcp_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_udp")]
            UDP_LOCATOR_PREFIX => self.udp_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_tls")]
            TLS_LOCATOR_PREFIX => self.tls_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_quic")]
            QUIC_LOCATOR_PREFIX => self.quic_inspector.is_multicast(locator).await,
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            UNIXSOCKSTREAM_LOCATOR_PREFIX => {
                self.unixsock_stream_inspector.is_multicast(locator).await
            }
            #[cfg(feature = "transport_ws")]
            WS_LOCATOR_PREFIX => self.ws_inspector.is_multicast(locator).await,
            #[cfg(feature = "transport_serial")]
            SERIAL_LOCATOR_PREFIX => self.serial_inspector.is_multicast(locator).await,
            _ => bail!("Unsupported protocol: {}.", protocol),
        }
    }
}
#[derive(Default)]
pub struct LinkConfigurator {
    #[cfg(feature = "transport_quic")]
    quic_inspector: QuicConfigurator,
    #[cfg(feature = "transport_tls")]
    tls_inspector: TlsConfigurator,
}
impl LinkConfigurator {
    #[allow(unused_variables, unused_mut)]
    pub async fn configurations(
        &self,
        config: &Config,
    ) -> (
        HashMap<String, Properties>,
        HashMap<String, zenoh_result::Error>,
    ) {
        let mut configs = HashMap::new();
        let mut errors = HashMap::new();
        let mut insert_config = |proto: String, cfg: ZResult<Properties>| match cfg {
            Ok(v) => {
                configs.insert(proto, v);
            }
            Err(e) => {
                errors.insert(proto, e);
            }
        };
        #[cfg(feature = "transport_quic")]
        {
            insert_config(
                QUIC_LOCATOR_PREFIX.into(),
                self.quic_inspector.inspect_config(config).await,
            );
        }
        #[cfg(feature = "transport_tls")]
        {
            insert_config(
                TLS_LOCATOR_PREFIX.into(),
                self.tls_inspector.inspect_config(config).await,
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
    pub fn make(_manager: NewLinkChannelSender, protocol: &str) -> ZResult<LinkManagerUnicast> {
        match protocol {
            #[cfg(feature = "transport_tcp")]
            TCP_LOCATOR_PREFIX => Ok(Arc::new(LinkManagerUnicastTcp::new(_manager))),
            #[cfg(feature = "transport_udp")]
            UDP_LOCATOR_PREFIX => Ok(Arc::new(LinkManagerUnicastUdp::new(_manager))),
            #[cfg(feature = "transport_tls")]
            TLS_LOCATOR_PREFIX => Ok(Arc::new(LinkManagerUnicastTls::new(_manager))),
            #[cfg(feature = "transport_quic")]
            QUIC_LOCATOR_PREFIX => Ok(Arc::new(LinkManagerUnicastQuic::new(_manager))),
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            UNIXSOCKSTREAM_LOCATOR_PREFIX => {
                Ok(Arc::new(LinkManagerUnicastUnixSocketStream::new(_manager)))
            }
            #[cfg(feature = "transport_ws")]
            WS_LOCATOR_PREFIX => Ok(Arc::new(LinkManagerUnicastWs::new(_manager))),
            #[cfg(feature = "transport_serial")]
            SERIAL_LOCATOR_PREFIX => Ok(Arc::new(LinkManagerUnicastSerial::new(_manager))),
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
            UDP_LOCATOR_PREFIX => Ok(Arc::new(LinkManagerMulticastUdp::default())),
            _ => bail!("Multicast not supported for {} protocol", protocol),
        }
    }
}

pub const WBUF_SIZE: usize = 64;
