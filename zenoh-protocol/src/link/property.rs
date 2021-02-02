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
/* Import of Link modules */
#[cfg(feature = "transport_tcp")]
use super::tcp::LinkPropertyTcp;
#[cfg(feature = "transport_tls")]
use super::tls::LinkPropertyTls;
#[cfg(feature = "transport_udp")]
use super::udp::LinkPropertyUdp;
#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
use super::unixsock_stream::LinkPropertyUnixSocketStream;
use std::fmt;

#[derive(Clone)]
pub enum LinkProperty {
    #[cfg(feature = "transport_tcp")]
    Tcp(LinkPropertyTcp),
    #[cfg(feature = "transport_udp")]
    Udp(LinkPropertyUdp),
    #[cfg(feature = "transport_tls")]
    Tls(LinkPropertyTls),
    #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
    UnixSocketStream(LinkPropertyUnixSocketStream),
}

impl fmt::Debug for LinkProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "n.a.")
    }
}
