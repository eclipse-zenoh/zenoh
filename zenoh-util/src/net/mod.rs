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
use async_std::net::{UdpSocket, ToSocketAddrs};

/// # Safety
/// This function is unsafe as it uses unsafe functions from the libc crate.
pub async unsafe fn bind_udp<A: ToSocketAddrs>(addrs: A, opts: Vec<(libc::c_int, *const libc::c_void)>) -> async_std::io::Result<UdpSocket> {
    let fd: async_std::os::unix::io::RawFd = 
        libc::socket(libc::AF_INET, libc::SOCK_DGRAM, libc::IPPROTO_UDP);
    if fd == -1 {return Err(async_std::io::Error::last_os_error());}

    for (opt, optval) in opts {
        let res = libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            opt,
            &optval as *const _ as *const libc::c_void,
            std::mem::size_of_val(&optval) as libc::socklen_t,
        );
        if res == -1 {return Err(async_std::io::Error::last_os_error());}
    }

    let addrs = addrs.to_socket_addrs().await?;
    for addr in addrs {
        let socketaddr: os_socketaddr::OsSocketAddr = addr.into();
    
        let res = libc::bind(fd, socketaddr.as_ptr(), socketaddr.len());
        if res != -1 {
            return Ok(async_std::os::unix::io::FromRawFd::from_raw_fd(fd))
        }
    }
    Err(async_std::io::Error::last_os_error())
}