//
// Copyright (c) 2025 ZettaScale Technology
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

/// mostly taken from https://github.com/pixsper/socket-pktinfo/blob/main/src/unix.rs
use std::io::{Error, IoSlice, IoSliceMut};
use std::{
    io, mem,
    mem::MaybeUninit,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    os::unix::io::{AsRawFd, RawFd},
    ptr,
};

use socket2::SockAddr;
use tokio::{io::Interest, net::UdpSocket};

const CMSG_BUF_SIZE: usize =
    unsafe { libc::CMSG_SPACE(mem::size_of::<libc::in6_pktinfo>() as libc::c_uint) as usize };

/// # Safety
/// The caller must ensure that:
/// - `socket` is a valid file descriptor for a socket.
/// - The combination of `level`, `name`, and the type `T` of `value` is a valid
///   socket option. The underlying C function `setsockopt` will read
///   `mem::size_of::<T>()` bytes from `value`.
unsafe fn setsockopt<T>(
    socket: libc::c_int,
    level: libc::c_int,
    name: libc::c_int,
    value: T,
) -> io::Result<()>
where
    T: Copy,
{
    let value = &value as *const T as *const libc::c_void;
    // SAFETY: Call the underlying C function `setsockopt`.
    if unsafe {
        libc::setsockopt(
            socket,
            level,
            name,
            value,
            mem::size_of::<T>() as libc::socklen_t,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(Error::last_os_error())
    }
}

#[derive(Clone)]
pub(crate) struct PktInfoRetrievalData {
    port: u16,
}

pub(crate) fn enable_pktinfo(socket: &UdpSocket) -> io::Result<PktInfoRetrievalData> {
    let local_src_addr = socket.local_addr()?;
    match local_src_addr.is_ipv6() {
        false => unsafe {
            setsockopt(socket.as_raw_fd(), libc::IPPROTO_IP, libc::IP_PKTINFO, 1)?;
        },
        true => unsafe {
            setsockopt(
                socket.as_raw_fd(),
                libc::IPPROTO_IPV6,
                libc::IPV6_RECVPKTINFO,
                1,
            )?;
        },
    }
    Ok(PktInfoRetrievalData {
        port: local_src_addr.port(),
    })
}

fn recv_with_dst_inner(
    fd: RawFd,
    local_port: u16,
    buf: &mut [u8],
) -> io::Result<(usize, SocketAddr, Option<SocketAddr>)> {
    let mut addr_src: MaybeUninit<libc::sockaddr_storage> = MaybeUninit::uninit();
    let mut msg_iov = IoSliceMut::new(buf);
    let mut cmsg = [0u8; CMSG_BUF_SIZE];

    let mut mhdr = unsafe {
        let mut mhdr = MaybeUninit::<libc::msghdr>::zeroed();
        let p = mhdr.as_mut_ptr();
        (*p).msg_name = addr_src.as_mut_ptr() as *mut libc::c_void;
        (*p).msg_namelen = mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        (*p).msg_iov = &mut msg_iov as *mut IoSliceMut as *mut libc::iovec;
        (*p).msg_iovlen = 1;
        (*p).msg_control = cmsg.as_mut_ptr() as *mut libc::c_void;
        (*p).msg_controllen = cmsg.len() as _;
        (*p).msg_flags = 0;
        mhdr.assume_init()
    };

    let bytes_recv = unsafe { libc::recvmsg(fd, &mut mhdr as *mut libc::msghdr, 0) };
    if bytes_recv <= 0 {
        return Err(Error::last_os_error());
    }

    let addr_src = unsafe {
        SockAddr::new(
            addr_src.assume_init(),
            mem::size_of::<libc::sockaddr_storage>() as _,
        )
    }
    .as_socket()
    .unwrap();

    let mut header = if mhdr.msg_controllen > 0 {
        debug_assert!(!mhdr.msg_control.is_null());
        debug_assert!(cmsg.len() >= mhdr.msg_controllen as _);

        Some(unsafe {
            libc::CMSG_FIRSTHDR(&mhdr as *const libc::msghdr)
                .as_ref()
                .unwrap()
        })
    } else {
        None
    };

    let mut addr_dst = None;

    while addr_dst.is_none() && header.is_some() {
        let h = header.unwrap();
        let p = unsafe { libc::CMSG_DATA(h) };

        match (h.cmsg_level, h.cmsg_type) {
            (libc::IPPROTO_IP, libc::IP_PKTINFO) => {
                let pktinfo = unsafe { ptr::read_unaligned(p as *const libc::in_pktinfo) };
                addr_dst = Some(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::from(u32::from_be(pktinfo.ipi_addr.s_addr))),
                    local_port,
                ));
            }
            (libc::IPPROTO_IPV6, libc::IPV6_PKTINFO) => {
                let pktinfo = unsafe { ptr::read_unaligned(p as *const libc::in6_pktinfo) };
                addr_dst = Some(SocketAddr::new(
                    IpAddr::V6(Ipv6Addr::from(pktinfo.ipi6_addr.s6_addr)),
                    local_port,
                ));
            }
            _ => {
                header = unsafe {
                    let p = libc::CMSG_NXTHDR(&mhdr as *const _, h as *const _);
                    p.as_ref()
                };
            }
        }
    }
    Ok((bytes_recv as _, addr_src, addr_dst))
}

pub(crate) async fn recv_with_dst(
    socket: &UdpSocket,
    data: &PktInfoRetrievalData,
    buffer: &mut [u8],
) -> io::Result<(usize, SocketAddr, Option<SocketAddr>)> {
    let fd = socket.as_raw_fd();
    let local_port = data.port;

    socket
        .async_io(Interest::READABLE, || {
            recv_with_dst_inner(fd, local_port, buffer)
        })
        .await
}

pub(crate) async fn send_with_src(
    socket: &UdpSocket,
    buffer: &[u8],
    dst_addr: &SocketAddr,
    src_addr: &SocketAddr,
) -> io::Result<usize> {
    let fd = socket.as_raw_fd();
    socket
        .async_io(Interest::WRITABLE, || {
            send_with_src_inner(fd, buffer, dst_addr, src_addr)
        })
        .await
}

fn send_with_src_inner(
    fd: RawFd,
    buffer: &[u8],
    dst_addr: &SocketAddr,
    src_addr: &SocketAddr,
) -> io::Result<usize> {
    let mut msg_iov = IoSlice::new(buffer);

    let mut addr_dst: MaybeUninit<libc::sockaddr_storage> = MaybeUninit::zeroed();
    let addr_dst_len = match dst_addr {
        SocketAddr::V4(a) => {
            let sockaddr: *mut libc::sockaddr_in = addr_dst.as_mut_ptr() as *mut _;
            unsafe {
                (*sockaddr).sin_family = libc::AF_INET as libc::sa_family_t;
                (*sockaddr).sin_port = a.port().to_be();
                (*sockaddr).sin_addr.s_addr = u32::from_ne_bytes(a.ip().octets());
            }
            mem::size_of::<libc::sockaddr_in>() as libc::socklen_t
        }
        SocketAddr::V6(a) => {
            let sockaddr: *mut libc::sockaddr_in6 = addr_dst.as_mut_ptr() as *mut _;
            unsafe {
                (*sockaddr).sin6_family = libc::AF_INET6 as libc::sa_family_t;
                (*sockaddr).sin6_port = a.port().to_be();
                (*sockaddr).sin6_addr.s6_addr = a.ip().octets();
                (*sockaddr).sin6_flowinfo = a.flowinfo();
                (*sockaddr).sin6_scope_id = a.scope_id();
            }
            mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t
        }
    };

    let mut cmsg = [0u8; CMSG_BUF_SIZE];

    let mhdr = unsafe {
        let mut mhdr = MaybeUninit::<libc::msghdr>::zeroed();
        let p = mhdr.as_mut_ptr();
        (*p).msg_name = addr_dst.as_mut_ptr() as *mut libc::c_void;
        (*p).msg_namelen = addr_dst_len;
        (*p).msg_iov = &mut msg_iov as *mut IoSlice as *mut libc::iovec;
        (*p).msg_iovlen = 1;

        if src_addr.ip().is_unspecified() {
            (*p).msg_control = ptr::null_mut();
            (*p).msg_controllen = 0;
        } else {
            (*p).msg_control = cmsg.as_mut_ptr() as *mut libc::c_void;
            match src_addr {
                SocketAddr::V4(a) => {
                    let cmsg_space =
                        libc::CMSG_SPACE(mem::size_of::<libc::in_pktinfo>() as libc::c_uint)
                            as usize;
                    (*p).msg_controllen = cmsg_space as _;

                    let cmsg_hdr = libc::CMSG_FIRSTHDR(p);
                    (*cmsg_hdr).cmsg_level = libc::IPPROTO_IP;
                    (*cmsg_hdr).cmsg_type = libc::IP_PKTINFO;
                    (*cmsg_hdr).cmsg_len =
                        libc::CMSG_LEN(mem::size_of::<libc::in_pktinfo>() as libc::c_uint) as _;

                    let pktinfo = libc::CMSG_DATA(cmsg_hdr) as *mut libc::in_pktinfo;
                    ptr::write_bytes(pktinfo, 0, 1);
                    // set ipi_ifindex to 0 for routing by source address
                    (*pktinfo).ipi_ifindex = 0;
                    // ipi_spec_dst is used as the local source address for the routing table lookup and for setting up IP source route options
                    (*pktinfo).ipi_spec_dst.s_addr = u32::from_ne_bytes(a.ip().octets());
                }
                SocketAddr::V6(a) => {
                    let cmsg_space =
                        libc::CMSG_SPACE(mem::size_of::<libc::in6_pktinfo>() as libc::c_uint)
                            as usize;
                    (*p).msg_controllen = cmsg_space as _;

                    let cmsg_hdr = libc::CMSG_FIRSTHDR(p);
                    (*cmsg_hdr).cmsg_level = libc::IPPROTO_IPV6;
                    (*cmsg_hdr).cmsg_type = libc::IPV6_PKTINFO;
                    (*cmsg_hdr).cmsg_len =
                        libc::CMSG_LEN(mem::size_of::<libc::in6_pktinfo>() as libc::c_uint) as _;

                    let pktinfo = libc::CMSG_DATA(cmsg_hdr) as *mut libc::in6_pktinfo;
                    ptr::write_bytes(pktinfo, 0, 1);
                    (*pktinfo).ipi6_ifindex = 0;
                    (*pktinfo).ipi6_addr.s6_addr = a.ip().octets();
                }
            }
        }
        (*p).msg_flags = 0;
        mhdr.assume_init()
    };

    let res = unsafe { libc::sendmsg(fd, &mhdr, 0) };
    if res < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(res as usize)
    }
}
