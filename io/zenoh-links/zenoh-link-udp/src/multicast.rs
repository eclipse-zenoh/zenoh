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
use std::{
    borrow::Cow,
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use async_trait::async_trait;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use zenoh_link_commons::{
    parse_dscp, set_dscp, LinkAuthId, LinkManagerMulticastTrait, LinkMulticast, LinkMulticastTrait,
    BIND_SOCKET,
};
use zenoh_protocol::{
    core::{Config, EndPoint, Locator},
    transport::BatchSize,
};
use zenoh_result::{bail, zerror, Error as ZError, ZResult};

use super::{config::*, UDP_DEFAULT_MTU};
use crate::{get_udp_addrs, socket_addr_to_udp_locator};

pub struct LinkMulticastUdp {
    // The unicast socket address of this link
    unicast_addr: SocketAddr,
    unicast_locator: Locator,
    // The unicast UDP socket used for write (and porentiablly read) operations
    unicast_socket: UdpSocket,
    // The multicast socket address of this link
    multicast_addr: SocketAddr,
    multicast_locator: Locator,
    // The multicast UDP socket used for read operations
    mcast_sock: UdpSocket,
}

impl LinkMulticastUdp {
    fn new(
        unicast_addr: SocketAddr,
        unicast_socket: UdpSocket,
        multicast_addr: SocketAddr,
        mcast_sock: UdpSocket,
    ) -> LinkMulticastUdp {
        LinkMulticastUdp {
            unicast_locator: socket_addr_to_udp_locator(&unicast_addr),
            multicast_locator: socket_addr_to_udp_locator(&multicast_addr),
            unicast_addr,
            unicast_socket,
            multicast_addr,
            mcast_sock,
        }
    }
}

#[async_trait]
impl LinkMulticastTrait for LinkMulticastUdp {
    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing UDP link: {}", self);
        match self.multicast_addr.ip() {
            IpAddr::V4(dst_ip4) => match self.multicast_addr.ip() {
                IpAddr::V4(src_ip4) => self.mcast_sock.leave_multicast_v4(dst_ip4, src_ip4),
                IpAddr::V6(_) => unreachable!(),
            },
            IpAddr::V6(dst_ip6) => self.mcast_sock.leave_multicast_v6(&dst_ip6, 0),
        }
        .map_err(|e| {
            let e = zerror!("Close error on UDP link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        match self
            .unicast_socket
            .send_to(buffer, self.multicast_addr)
            .await
        {
            Ok(size) => Ok(size),
            std::io::Result::Err(e) => {
                #[cfg(not(target_os = "macos"))]
                {
                    let e = zerror!("Write error on UDP link {}: {}", self, e);
                    tracing::trace!("{}", e);
                    Err(e.into())
                }
                #[cfg(target_os = "macos")]
                if let Some(55) = e.raw_os_error() {
                    // No buffer space available
                    tracing::trace!("Write error on UDP link {}: {}", self, e);
                    Ok(0)
                } else {
                    let e = zerror!("Write error on UDP link {}: {}", self, e);
                    tracing::trace!("{}", e);
                    Err(e.into())
                }
            }
        }
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut written: usize = 0;
        while written < buffer.len() {
            written += self.write(&buffer[written..]).await?;
        }
        Ok(())
    }

    async fn read<'a>(&'a self, buffer: &mut [u8]) -> ZResult<(usize, Cow<'a, Locator>)> {
        loop {
            let (n, addr) = self.mcast_sock.recv_from(buffer).await.map_err(|e| {
                let e = zerror!("Read error on UDP link {}: {}", self, e);
                tracing::trace!("{}", e);
                e
            })?;

            if self.unicast_addr == addr {
                continue; // We are reading our own messages, skip it
            } else {
                let locator = socket_addr_to_udp_locator(&addr);
                break Ok((n, Cow::Owned(locator)));
            }
        }
    }

    fn get_auth_id(&self) -> &LinkAuthId {
        &LinkAuthId::Udp
    }

    #[inline(always)]
    fn get_src(&self) -> &Locator {
        &self.unicast_locator
    }

    #[inline(always)]
    fn get_dst(&self) -> &Locator {
        &self.multicast_locator
    }

    #[inline(always)]
    fn get_mtu(&self) -> BatchSize {
        *UDP_DEFAULT_MTU
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        false
    }
}

impl fmt::Display for LinkMulticastUdp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.unicast_addr, self.multicast_addr)?;
        Ok(())
    }
}

impl fmt::Debug for LinkMulticastUdp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Udp")
            .field("src", &self.unicast_addr)
            .field("dst", &self.multicast_addr)
            .finish()
    }
}

/*************************************/
/*             MANAGER               */
/*************************************/
#[derive(Default)]
pub struct LinkManagerMulticastUdp;

impl LinkManagerMulticastUdp {
    async fn new_link_inner(
        &self,
        mcast_addr: &SocketAddr,
        config: Config<'_>,
        bind_socket: Option<&str>,
        dscp: Option<u32>,
    ) -> ZResult<(UdpSocket, SocketAddr, UdpSocket, SocketAddr)> {
        let domain = match mcast_addr.ip() {
            IpAddr::V4(_) => Domain::IPV4,
            IpAddr::V6(_) => Domain::IPV6,
        };

        // Get local unicast address to bind the socket on
        let mut local_addr = if let Some(bind_socket) = bind_socket {
            let bind_addr = SocketAddr::from_str(bind_socket)?;
            match (bind_addr, mcast_addr) {
                (SocketAddr::V6(local), SocketAddr::V4(dest)) => {
                    return Err(Box::from(format!(
                        "Protocols must match: Cannot bind to IPv6 {local} and join IPv4 {dest}"
                    )));
                }
                (SocketAddr::V4(local), SocketAddr::V6(dest)) => {
                    return Err(Box::from(format!(
                        "Protocols must match: Cannot bind to IPv4 {local} and join IPv6 {dest}"
                    )));
                }
                _ => bind_addr, // No issue here
            }
        } else if mcast_addr.is_ipv4() {
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)
        } else {
            SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 0)
        };
        if local_addr.ip() == IpAddr::V4(Ipv4Addr::UNSPECIFIED)
            || local_addr.ip() == IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        {
            // Get default iface address to bind the socket on if provided
            let mut iface_addr: Option<IpAddr> = None;
            if let Some(iface) = config.get(UDP_MULTICAST_IFACE) {
                iface_addr = match iface.parse() {
                    Ok(addr) => Some(addr),
                    Err(_) => zenoh_util::net::get_unicast_addresses_of_interface(iface)?
                        .into_iter()
                        .filter(|x| match mcast_addr.ip() {
                            IpAddr::V4(_) => x.is_ipv4(),
                            IpAddr::V6(_) => x.is_ipv6(),
                        })
                        .take(1)
                        .collect::<Vec<IpAddr>>()
                        .first()
                        .copied(),
                };
            }
            if let Some(iface_addr) = iface_addr {
                local_addr.set_ip(iface_addr);
            } else if let Some(iface_addr) =
                zenoh_util::net::get_unicast_addresses_of_multicast_interfaces()
                    .into_iter()
                    .filter(|x| {
                        !x.is_loopback()
                            && match mcast_addr.ip() {
                                IpAddr::V4(_) => x.is_ipv4(),
                                IpAddr::V6(_) => x.is_ipv6(),
                            }
                    })
                    .take(1)
                    .collect::<Vec<IpAddr>>()
                    .first()
                    .copied()
            {
                local_addr.set_ip(iface_addr);
            }
        }

        // Establish a unicast UDP socket
        let ucast_sock = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
        match &local_addr.ip() {
            IpAddr::V4(addr) => {
                ucast_sock
                    .set_multicast_if_v4(addr)
                    .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
            }
            IpAddr::V6(_) => match zenoh_util::net::get_index_of_interface(local_addr.ip()) {
                Ok(idx) => ucast_sock
                    .set_multicast_if_v6(idx)
                    .map_err(|e| zerror!("{}: {}", mcast_addr, e))?,
                Err(e) => bail!("{}: {}", mcast_addr, e),
            },
        }

        ucast_sock
            .bind(&local_addr.into())
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;

        if let Some(dscp) = dscp {
            set_dscp(&ucast_sock, *mcast_addr, dscp)?;
        }

        // Must set to nonblocking according to the doc of tokio
        // https://docs.rs/tokio/latest/tokio/net/struct.UdpSocket.html#notes
        ucast_sock.set_nonblocking(true)?;
        let ucast_sock = UdpSocket::from_std(ucast_sock.into())?;

        // Establish a multicast UDP socket
        let mcast_sock = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
        mcast_sock
            .set_reuse_address(true)
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
        #[cfg(target_family = "unix")]
        {
            mcast_sock
                .set_reuse_port(true)
                .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
        }

        // Bind the socket: let's bind to the unspecified address so we can join and read
        // from multiple multicast groups.
        let bind_mcast_addr = match mcast_addr.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        };
        mcast_sock
            .bind(&SocketAddr::new(bind_mcast_addr, mcast_addr.port()).into())
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;

        // Join the multicast group
        let join = config.values(UDP_MULTICAST_JOIN);
        match mcast_addr.ip() {
            IpAddr::V4(dst_ip4) => match local_addr.ip() {
                IpAddr::V4(src_ip4) => {
                    // Join default multicast group
                    mcast_sock
                        .join_multicast_v4(&dst_ip4, &src_ip4)
                        .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
                    // Join any additional multicast group
                    for g in join {
                        let g: Ipv4Addr =
                            g.parse().map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
                        mcast_sock
                            .join_multicast_v4(&g, &src_ip4)
                            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
                    }
                }
                IpAddr::V6(src_ip6) => bail!("{}: unexpected IPv6 source address", src_ip6),
            },
            IpAddr::V6(dst_ip6) => {
                // Join default multicast group
                mcast_sock
                    .join_multicast_v6(&dst_ip6, 0)
                    .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
                // Join any additional multicast group
                for g in join {
                    let g: Ipv6Addr = g.parse().map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
                    mcast_sock
                        .join_multicast_v6(&g, 0)
                        .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
                }
            }
        };

        // Must set to nonblocking according to the doc of tokio
        // https://docs.rs/tokio/latest/tokio/net/struct.UdpSocket.html#notes
        mcast_sock.set_nonblocking(true)?;

        // If TTL is specified, add set the socket's TTL
        if let Some(ttl_str) = config.get(UDP_MULTICAST_TTL) {
            match &local_addr.ip() {
                IpAddr::V4(_) => {
                    let ttl = match ttl_str.parse::<u32>() {
                        Ok(ttl) => ttl,
                        Err(e) => bail!("Can not parse TTL '{}' to a u32: {}", ttl_str, e),
                    };

                    ucast_sock.set_multicast_ttl_v4(ttl).map_err(|e| {
                        zerror!("Can not set multicast TTL {} on {}: {}", ttl, mcast_addr, e)
                    })?;
                }
                IpAddr::V6(_) => {
                    tracing::warn!(
                            "UDP multicast hop limit not supported for v6 socket: {}. See https://github.com/rust-lang/rust/pull/138744.",
                            mcast_addr
                        );
                }
            }
        }

        // Build the tokio multicast UdpSocket
        let mcast_sock = UdpSocket::from_std(mcast_sock.into())?;

        let ucast_addr = ucast_sock
            .local_addr()
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
        assert_eq!(ucast_addr.ip(), local_addr.ip());
        // We may have bind to port 0, so we need to retrieve the actual port
        let mcast_port = mcast_sock
            .local_addr()
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?
            .port();
        let mcast_addr = SocketAddr::new(mcast_addr.ip(), mcast_port);

        Ok((mcast_sock, mcast_addr, ucast_sock, ucast_addr))
    }
}

#[async_trait]
impl LinkManagerMulticastTrait for LinkManagerMulticastUdp {
    async fn new_link(&self, endpoint: &EndPoint) -> ZResult<LinkMulticast> {
        let mcast_addrs = get_udp_addrs(endpoint.address())
            .await?
            .filter(|a| a.ip().is_multicast())
            .collect::<Vec<SocketAddr>>();
        let config = endpoint.config();
        let bind_socket = config.get(BIND_SOCKET);
        let dscp = parse_dscp(&config)?;

        let mut errs: Vec<ZError> = vec![];
        for maddr in mcast_addrs {
            match self
                .new_link_inner(&maddr, endpoint.config(), bind_socket, dscp)
                .await
            {
                Ok((mcast_sock, mcast_addr, ucast_sock, ucast_addr)) => {
                    let link = Arc::new(LinkMulticastUdp::new(
                        ucast_addr, ucast_sock, mcast_addr, mcast_sock,
                    ));

                    return Ok(LinkMulticast(link));
                }
                Err(e) => {
                    errs.push(e);
                }
            }
        }

        if errs.is_empty() {
            errs.push(zerror!("No UDP multicast addresses available").into());
        }

        bail!(
            "Can not create a new UDP link bound to {}: {:?}",
            endpoint,
            errs
        )
    }
}
