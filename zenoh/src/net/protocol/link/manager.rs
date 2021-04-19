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
use super::quic::LinkManagerQuic;
use super::session::SessionManager;
#[cfg(feature = "transport_tcp")]
use super::tcp::LinkManagerTcp;
#[cfg(feature = "transport_tls")]
use super::tls::LinkManagerTls;
#[cfg(feature = "transport_udp")]
use super::udp::LinkManagerUdp;
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
use super::unixsock_stream::LinkManagerUnixSocketStream;
use super::{LinkManager, LocatorProtocol};
use async_std::sync::Arc;

pub struct LinkManagerBuilder;

impl LinkManagerBuilder {
    pub(crate) fn make(manager: SessionManager, protocol: &LocatorProtocol) -> LinkManager {
        match protocol {
            #[cfg(feature = "transport_tcp")]
            LocatorProtocol::Tcp => Arc::new(LinkManagerTcp::new(manager)),
            #[cfg(feature = "transport_udp")]
            LocatorProtocol::Udp => Arc::new(LinkManagerUdp::new(manager)),
            #[cfg(feature = "transport_tls")]
            LocatorProtocol::Tls => Arc::new(LinkManagerTls::new(manager)),
            #[cfg(feature = "transport_quic")]
            LocatorProtocol::Quic => Arc::new(LinkManagerQuic::new(manager)),
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorProtocol::UnixSocketStream => {
                Arc::new(LinkManagerUnixSocketStream::new(manager))
            }
        }
    }
}
