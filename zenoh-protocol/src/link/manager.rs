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
use crate::link::tcp::LinkManagerTcp;
#[cfg(feature = "transport_udp")]
use crate::link::udp::LinkManagerUdp;
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
use crate::link::unixsock_stream::LinkManagerUnixSockStream;
use crate::link::{LinkManager, LocatorProtocol};
use crate::session::SessionManager;
use async_std::sync::Arc;

pub struct LinkManagerBuilder;

impl LinkManagerBuilder {
    pub(crate) fn make(manager: SessionManager, protocol: &LocatorProtocol) -> LinkManager {
        match protocol {
            #[cfg(feature = "transport_tcp")]
            LocatorProtocol::Tcp => Arc::new(LinkManagerTcp::new(manager)),
            #[cfg(feature = "transport_udp")]
            LocatorProtocol::Udp => Arc::new(LinkManagerUdp::new(manager)),
            #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
            LocatorProtocol::UnixSockStream => Arc::new(LinkManagerUnixSockStream::new(manager)),
        }
    }
}
