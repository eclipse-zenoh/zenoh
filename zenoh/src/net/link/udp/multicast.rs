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
use super::*;
use async_std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use async_trait::async_trait;
use socket2::{Domain, Protocol, Socket, Type};
use std::fmt;
use std::sync::Arc;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::net::get_local_addresses;

pub struct LinkMulticastUdp {
    // The unicast socket address of this link
    unicast_addr: SocketAddr,
    // The unicast UDP socket used for write (and porentiablly read) operations
    unicast_socket: UdpSocket,
    // The multicast socket address of this link
    multicast_addr: SocketAddr,
    // The multicast UDP socket used for read operations
    mcast_sock: UdpSocket,
    // The list of local addresses
    local_addrs: Vec<IpAddr>,
}

impl LinkMulticastUdp {
    fn new(
        unicast_addr: SocketAddr,
        unicast_socket: UdpSocket,
        multicast_addr: SocketAddr,
        mcast_sock: UdpSocket,
        local_addrs: Vec<IpAddr>,
    ) -> LinkMulticastUdp {
        LinkMulticastUdp {
            unicast_addr,
            unicast_socket,
            multicast_addr,
            mcast_sock,
            local_addrs,
        }
    }
}

#[async_trait]
impl LinkMulticastTrait for LinkMulticastUdp {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing UDP link: {}", self);
        match self.multicast_addr.ip() {
            IpAddr::V4(dst_ip4) => match self.multicast_addr.ip() {
                IpAddr::V4(src_ip4) => self.mcast_sock.leave_multicast_v4(dst_ip4, src_ip4),
                IpAddr::V6(_) => unreachable!(),
            },
            IpAddr::V6(dst_ip6) => self.mcast_sock.leave_multicast_v6(&dst_ip6, 0),
        }
        .map_err(|e| {
            let e = format!("Close error on UDP link {}: {}", self, e);
            log::trace!("{}", e);
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        (&self.unicast_socket)
            .send_to(buffer, self.multicast_addr)
            .await
            .map_err(|e| {
                let e = format!("Write error on UDP link {}: {}", self, e);
                log::trace!("{}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut written: usize = 0;
        while written < buffer.len() {
            written += self.write(&buffer[written..]).await?;
        }
        Ok(())
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<(usize, Locator)> {
        loop {
            let (n, addr) = (&self.mcast_sock).recv_from(buffer).await.map_err(|e| {
                let e = format!("Read error on UDP link {}: {}", self, e);
                log::trace!("{}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })?;
            if self
                .local_addrs
                .iter()
                .any(|la| *la == addr.ip() && self.unicast_addr.port() == addr.port())
            {
                continue; // We are reading our own messages, skip it
            } else {
                break Ok((n, Locator::Udp(LocatorUdp::SocketAddr(addr))));
            }
        }
    }

    #[inline(always)]
    fn get_src(&self) -> Locator {
        Locator::Udp(LocatorUdp::SocketAddr(self.unicast_addr))
    }

    #[inline(always)]
    fn get_dst(&self) -> Locator {
        Locator::Udp(LocatorUdp::SocketAddr(self.multicast_addr))
    }

    #[inline(always)]
    fn get_mtu(&self) -> u16 {
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

#[async_trait]
impl LinkManagerMulticastTrait for LinkManagerMulticastUdp {
    async fn new_link(
        &self,
        mutlticast: &Locator,
        _ps: Option<&LocatorProperty>,
    ) -> ZResult<LinkMulticast> {
        let mcast_addr = get_udp_addr(mutlticast).await?;

        // Defaults
        let default_ipv4_iface = Ipv4Addr::new(0, 0, 0, 0);
        let default_ipv6_iface = 0;
        let default_ipv4_addr = Ipv4Addr::new(0, 0, 0, 0);
        let default_ipv6_addr = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0);

        macro_rules! zerrmsg {
            ($err:expr) => {{
                let e = format!("Can not create a new UDP link bound to {}: {}", mcast_addr, $err);
                log::warn!("{}", e);
                zerror2!(ZErrorKind::InvalidLink { descr: e })
            }};
        }

        // Establish a multicast UDP socket
        let domain = match mcast_addr.ip() {
            IpAddr::V4(_) => Domain::IPV4,
            IpAddr::V6(_) => Domain::IPV6,
        };

        // Create the multicast socket
        let mcast_sock =
            Socket::new(domain, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| zerrmsg!(e))?;
        mcast_sock
            .set_reuse_address(true)
            .map_err(|e| zerrmsg!(e))?;

        // Bind the socket
        let default_mcast_addr = {
            #[cfg(unix)]
            {
                mcast_addr.ip()
            } // See UNIX Network Programmping p.212
            #[cfg(windows)]
            {
                match mcast_addr.ip() {
                    IpAddr::V4(_) => IpAddr::V4(default_ipv4_addr),
                    IpAddr::V6(_) => IpAddr::V6(default_ipv6_addr),
                }
            }
        };
        let _ = mcast_sock
            .bind(&SocketAddr::new(default_mcast_addr, mcast_addr.port()).into())
            .map_err(|e| zerrmsg!(e))?;

        // Join the multicast group
        match mcast_addr.ip() {
            IpAddr::V4(dst_ip4) => mcast_sock.join_multicast_v4(&dst_ip4, &default_ipv4_iface),
            IpAddr::V6(dst_ip6) => mcast_sock.join_multicast_v6(&dst_ip6, default_ipv6_iface),
        }
        .map_err(|e| zerrmsg!(e))?;

        // Build the async_std multicast UdpSocket
        let mcast_sock: UdpSocket = std::net::UdpSocket::from(mcast_sock).into();

        // Establish a unicast UDP socket
        let ucast_sock = match mcast_addr.ip() {
            IpAddr::V4(_) => {
                UdpSocket::bind(&SocketAddr::new(IpAddr::V4(default_ipv4_addr), 0)).await
            }
            IpAddr::V6(_) => {
                UdpSocket::bind(&SocketAddr::new(IpAddr::V6(default_ipv6_addr), 0)).await
            }
        }
        .map_err(|e| zerrmsg!(e))?;
        let ucast_addr = ucast_sock.local_addr().map_err(|e| zerrmsg!(e))?;
        let local_addrs = get_local_addresses()
            .map_err(|e| zerrmsg!(e))?
            .iter()
            .filter(|a| match mcast_addr.ip() {
                IpAddr::V4(_) => a.is_ipv4(),
                IpAddr::V6(_) => a.is_ipv6(),
            })
            .copied()
            .collect();

        let link = Arc::new(LinkMulticastUdp::new(
            ucast_addr,
            ucast_sock,
            mcast_addr,
            mcast_sock,
            local_addrs,
        ));

        Ok(LinkMulticast(link))
    }
}
