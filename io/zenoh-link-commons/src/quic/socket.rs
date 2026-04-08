//
// Copyright (c) 2026 ZettaScale Technology
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
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use tokio::net::UdpSocket;
use zenoh_core::bail;
use zenoh_protocol::core::{endpoint::Config, Address};
use zenoh_result::ZResult;

use crate::{parse_dscp, quic::get_quic_addr, set_dscp, BIND_INTERFACE, BIND_SOCKET};

pub struct QuicSocketConfig<'a> {
    iface: Option<&'a str>,
    bind_socket: Option<SocketAddr>,
    dscp: Option<u32>,
}

impl<'a> QuicSocketConfig<'a> {
    pub async fn new(epconf: &Config<'a>) -> ZResult<Self> {
        if let (Some(_), Some(_)) = (epconf.get(BIND_INTERFACE), epconf.get(BIND_SOCKET)) {
            bail!(
                "Using Config options `iface` and `bind` in conjunction is unsupported at this time {} {:?}",
                BIND_INTERFACE,
                BIND_SOCKET
            )
        }

        let bind_socket = if let Some(bind_socket_str) = epconf.get(BIND_SOCKET) {
            Some(get_quic_addr(&Address::from(bind_socket_str)).await?)
        } else {
            None
        };

        Ok(Self {
            iface: epconf.get(BIND_INTERFACE),
            bind_socket,
            dscp: parse_dscp(epconf)?,
        })
    }

    /// Build a new UDP socket bound to `addr` with the given configuration parameters
    pub async fn new_listener(&self, addr: &SocketAddr) -> ZResult<UdpSocket> {
        let socket = tokio::net::UdpSocket::bind(addr).await?;
        if let Some(iface) = self.iface {
            zenoh_util::net::set_bind_to_device_udp_socket(&socket, iface)?;
        }
        if let Some(dscp) = self.dscp {
            set_dscp(&socket, *addr, dscp)?;
        }
        Ok(socket)
    }

    /// Connect to a UDP socket address at `dst_addr` with the given configuration parameters
    pub async fn new_link(&self, dst_addr: &SocketAddr) -> ZResult<UdpSocket> {
        let src_addr = if let Some(bind_socket) = self.bind_socket {
            bind_socket
        } else if dst_addr.is_ipv4() {
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)
        } else {
            SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 0)
        };
        let socket = tokio::net::UdpSocket::bind(src_addr).await?;

        if let Some(dscp) = self.dscp {
            set_dscp(&socket, src_addr, dscp)?;
        }
        if let Some(iface) = self.iface {
            zenoh_util::net::set_bind_to_device_udp_socket(&socket, iface)?;
        };

        Ok(socket)
    }
}
