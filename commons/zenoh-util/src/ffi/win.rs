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
    io, mem,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
};

use winapi::shared::{ws2def, ws2ipdef};

#[allow(clippy::many_single_char_names)]
pub fn sockaddr_to_addr(socket_addr: ws2def::SOCKET_ADDRESS) -> io::Result<SocketAddr> {
    unsafe {
        match socket_addr.lpSockaddr.as_ref().unwrap().sa_family as i32 {
            ws2def::AF_INET => {
                assert!(
                    socket_addr.iSockaddrLength as usize >= mem::size_of::<ws2def::SOCKADDR_IN>()
                );
                let socket_addr: &ws2def::SOCKADDR_IN = &*(socket_addr.lpSockaddr.as_ref().unwrap()
                    as *const ws2def::SOCKADDR
                    as *const ws2def::SOCKADDR_IN);
                let ip: u32 = mem::transmute(socket_addr.sin_addr.S_un);
                let ip = ip.to_be();
                let a = (ip >> 24) as u8;
                let b = (ip >> 16) as u8;
                let c = (ip >> 8) as u8;
                let d = ip as u8;
                let sockaddrv4 = SocketAddrV4::new(
                    Ipv4Addr::new(a, b, c, d),
                    u16::from_be(socket_addr.sin_port),
                );
                Ok(SocketAddr::V4(sockaddrv4))
            }
            ws2def::AF_INET6 => {
                assert!(
                    socket_addr.iSockaddrLength as usize
                        >= mem::size_of::<ws2ipdef::SOCKADDR_IN6>()
                );
                let socket_addr: &ws2ipdef::SOCKADDR_IN6 =
                    &*(socket_addr.lpSockaddr.as_ref().unwrap() as *const ws2def::SOCKADDR
                        as *const ws2ipdef::SOCKADDR_IN6_LH);
                let arr: [u16; 8] = mem::transmute(socket_addr.sin6_addr.u);
                let a = u16::from_be(arr[0]);
                let b = u16::from_be(arr[1]);
                let c = u16::from_be(arr[2]);
                let d = u16::from_be(arr[3]);
                let e = u16::from_be(arr[4]);
                let f = u16::from_be(arr[5]);
                let g = u16::from_be(arr[6]);
                let h = u16::from_be(arr[7]);
                let ip = Ipv6Addr::new(a, b, c, d, e, f, g, h);
                Ok(SocketAddr::V6(SocketAddrV6::new(
                    ip,
                    u16::from_be(socket_addr.sin6_port),
                    u32::from_be(socket_addr.sin6_flowinfo),
                    socket_addr.sin6_flowinfo,
                )))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected IPv4 or IPv6 socket",
            )),
        }
    }
}
