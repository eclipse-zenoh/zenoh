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
use zenoh_core::{bail, zerror};
use zenoh_protocol::core::{endpoint::Config, Address};
use zenoh_result::ZResult;

use crate::{
    parse_dscp,
    quic::{get_quic_addr, QUIC_SO_RCV_BUF, QUIC_SO_SND_BUF},
    set_dscp, BIND_INTERFACE, BIND_SOCKET,
};

pub struct QuicSocketConfig<'a> {
    iface: Option<&'a str>,
    bind_socket: Option<SocketAddr>,
    dscp: Option<u32>,
    buffer_sizes: QuicSocketBufferSizes,
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
            buffer_sizes: QuicSocketBufferSizes::try_from(epconf)?,
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
        self.buffer_sizes.apply(&socket)?;
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
        self.buffer_sizes.apply(&socket)?;

        Ok(socket)
    }
}

#[derive(Debug, Default, PartialEq)]
struct QuicSocketBufferSizes {
    rx: Option<u32>,
    tx: Option<u32>,
}

impl TryFrom<&Config<'_>> for QuicSocketBufferSizes {
    type Error = zenoh_result::Error;

    fn try_from(config: &Config) -> ZResult<Self> {
        let parse = |key: &str| -> ZResult<Option<u32>> {
            config
                .get(key)
                .map(|size| {
                    size.parse()
                        .map_err(|_| zerror!("Unknown QUIC {key} argument: {size}").into())
                })
                .transpose()
        };
        Ok(Self {
            rx: parse(QUIC_SO_RCV_BUF)?,
            tx: parse(QUIC_SO_SND_BUF)?,
        })
    }
}

impl QuicSocketBufferSizes {
    fn apply<'s>(&self, socket: impl Into<socket2::SockRef<'s>>) -> ZResult<()> {
        let socket = socket.into();
        if let Some(size) = self.rx {
            socket.set_recv_buffer_size(size as usize)?;
        }
        if let Some(size) = self.tx {
            socket.set_send_buffer_size(size as usize)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use zenoh_protocol::core::EndPoint;

    use super::*;

    fn buffer_sizes(endpoint: &str) -> ZResult<QuicSocketBufferSizes> {
        let endpoint: EndPoint = endpoint.parse().unwrap();
        QuicSocketBufferSizes::try_from(&endpoint.config())
    }

    #[test]
    fn buffer_sizes_config() {
        assert_eq!(
            buffer_sizes("quic/localhost:7447").unwrap(),
            QuicSocketBufferSizes::default()
        );
        assert_eq!(
            buffer_sizes("quic/localhost:7447#so_rcvbuf=2097152;so_sndbuf=65536").unwrap(),
            QuicSocketBufferSizes {
                rx: Some(2_097_152),
                tx: Some(65_536)
            }
        );
        assert!(buffer_sizes("quic/localhost:7447#so_rcvbuf=2MB").is_err());
        assert!(buffer_sizes("quic/localhost:7447#so_sndbuf=-1").is_err());
    }

    #[test]
    fn buffer_sizes_apply() {
        // Below the Linux default caps (net.core.rmem_max and wmem_max), so the OS does not lower them.
        const SIZE: u32 = 100_000;
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        QuicSocketBufferSizes {
            rx: Some(SIZE),
            tx: Some(SIZE),
        }
        .apply(&socket)
        .unwrap();
        let socket = socket2::SockRef::from(&socket);
        // Linux reports twice the set value, to account for its bookkeeping overhead.
        assert!(socket.recv_buffer_size().unwrap() >= SIZE as usize);
        assert!(socket.send_buffer_size().unwrap() >= SIZE as usize);
    }
}
