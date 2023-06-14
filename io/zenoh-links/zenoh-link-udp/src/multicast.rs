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
use super::{config::*, UDP_DEFAULT_MTU};
use crate::{get_udp_addrs, socket_addr_to_udp_locator};
use async_std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use async_trait::async_trait;
use socket2::{Domain, Protocol, Socket, Type};
use std::sync::Arc;
use std::{borrow::Cow, fmt};
use zenoh_link_commons::{LinkManagerMulticastTrait, LinkMulticast, LinkMulticastTrait};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{bail, zerror, Error as ZError, ZResult};

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
        log::trace!("Closing UDP link: {}", self);
        match self.multicast_addr.ip() {
            IpAddr::V4(dst_ip4) => match self.multicast_addr.ip() {
                IpAddr::V4(src_ip4) => self.mcast_sock.leave_multicast_v4(dst_ip4, src_ip4),
                IpAddr::V6(_) => unreachable!(),
            },
            IpAddr::V6(dst_ip6) => self.mcast_sock.leave_multicast_v6(&dst_ip6, 0),
        }
        .map_err(|e| {
            let e = zerror!("Close error on UDP link {}: {}", self, e);
            log::trace!("{}", e);
            e.into()
        })
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        self.unicast_socket
            .send_to(buffer, self.multicast_addr)
            .await
            .map_err(|e| {
                let e = zerror!("Write error on UDP link {}: {}", self, e);
                log::trace!("{}", e);
                e.into()
            })
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
                log::trace!("{}", e);
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

    #[inline(always)]
    fn get_src(&self) -> &Locator {
        &self.unicast_locator
    }

    #[inline(always)]
    fn get_dst(&self) -> &Locator {
        &self.multicast_locator
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

impl LinkManagerMulticastUdp {
    async fn new_link_inner(
        &self,
        mcast_addr: &SocketAddr,
        iface: Option<&str>,
    ) -> ZResult<(UdpSocket, UdpSocket, SocketAddr)> {
        let domain = match mcast_addr.ip() {
            IpAddr::V4(_) => Domain::IPV4,
            IpAddr::V6(_) => Domain::IPV6,
        };

        // Defaults
        let _default_ipv4_iface = Ipv4Addr::UNSPECIFIED;
        let default_ipv6_iface = 0;
        let default_ipv4_addr = Ipv4Addr::UNSPECIFIED;
        let default_ipv6_addr = Ipv6Addr::UNSPECIFIED;

        // Get default iface address to bind the socket on if provided
        let mut iface_addr: Option<IpAddr> = None;
        if let Some(iface) = iface {
            iface_addr = match iface.parse() {
                Ok(addr) => Some(addr),
                Err(_) => zenoh_util::net::get_unicast_addresses_of_interface(iface)?
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
                    .get(0)
                    .copied(),
            };
        }

        // Get local unicast address to bind the socket on
        let local_addr = match iface_addr {
            Some(iface_addr) => iface_addr,
            None => {
                let iface = zenoh_util::net::get_unicast_addresses_of_multicast_interfaces()
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
                    .get(0)
                    .copied();

                match iface {
                    Some(iface) => iface,
                    None => match mcast_addr.ip() {
                        IpAddr::V4(_) => IpAddr::V4(default_ipv4_addr),
                        IpAddr::V6(_) => IpAddr::V6(default_ipv6_addr),
                    },
                }
            }
        };

        // Establish a unicast UDP socket
        let ucast_sock = UdpSocket::bind(SocketAddr::new(local_addr, 0))
            .await
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;

        // Establish a multicast UDP socket
        let mcast_sock = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
        mcast_sock
            .set_reuse_address(true)
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;

        // Bind the socket
        let default_mcast_addr = {
            #[cfg(unix)]
            {
                match mcast_addr.ip() {
                    IpAddr::V4(ip4) => IpAddr::V4(ip4),
                    IpAddr::V6(_) => local_addr,
                }
            } // See UNIX Network Programmping p.212
            #[cfg(windows)]
            {
                match mcast_addr.ip() {
                    IpAddr::V4(_) => IpAddr::V4(default_ipv4_addr),
                    IpAddr::V6(_) => IpAddr::V6(default_ipv6_addr),
                }
            }
        };
        mcast_sock
            .bind(&SocketAddr::new(default_mcast_addr, mcast_addr.port()).into())
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;

        // Join the multicast group
        match mcast_addr.ip() {
            IpAddr::V4(dst_ip4) => match local_addr {
                IpAddr::V4(src_ip4) => mcast_sock.join_multicast_v4(&dst_ip4, &src_ip4),
                IpAddr::V6(_) => panic!(),
            },
            IpAddr::V6(dst_ip6) => mcast_sock.join_multicast_v6(&dst_ip6, default_ipv6_iface),
        }
        .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;

        // Build the async_std multicast UdpSocket
        let mcast_sock: UdpSocket = std::net::UdpSocket::from(mcast_sock).into();

        let ucast_addr = ucast_sock
            .local_addr()
            .map_err(|e| zerror!("{}: {}", mcast_addr, e))?;
        assert_eq!(ucast_addr.ip(), local_addr);

        Ok((mcast_sock, ucast_sock, ucast_addr))
    }
}

#[async_trait]
impl LinkManagerMulticastTrait for LinkManagerMulticastUdp {
    async fn new_link(&self, endpoint: &EndPoint) -> ZResult<LinkMulticast> {
        let mcast_addrs = get_udp_addrs(endpoint.address())
            .await?
            .filter(|a| a.ip().is_multicast())
            .collect::<Vec<SocketAddr>>();

        let mut errs: Vec<ZError> = vec![];
        for maddr in mcast_addrs {
            match self
                .new_link_inner(&maddr, endpoint.config().get(UDP_MULTICAST_SRC_IFACE))
                .await
            {
                Ok((mcast_sock, ucast_sock, ucast_addr)) => {
                    let link = Arc::new(LinkMulticastUdp::new(
                        ucast_addr, ucast_sock, maddr, mcast_sock,
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
