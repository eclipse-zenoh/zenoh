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

/// mostly taken from https://github.com/pixsper/socket-pktinfo/blob/main/src/win.rs
use std::io::Error;
use std::{
    io, mem,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    os::windows::io::{AsRawSocket, RawSocket},
    ptr,
};

use socket2::SockAddr;
use tokio::{io::Interest, net::UdpSocket};
use windows_sys::{
    core::{PCSTR, PSTR},
    Win32::{
        Networking::WinSock::{
            self, CMSGHDR, IN6_PKTINFO, IN_PKTINFO, IPPROTO_IP, IPPROTO_IPV6, IPV6_PKTINFO,
            IP_PKTINFO, LPFN_WSARECVMSG, LPWSAOVERLAPPED_COMPLETION_ROUTINE,
            SIO_GET_EXTENSION_FUNCTION_POINTER, SOCKET, WSABUF, WSAID_WSARECVMSG, WSAMSG,
        },
        System::IO::OVERLAPPED,
    },
};
const CMSG_HEADER_SIZE: usize = mem::size_of::<CMSGHDR>();
const PKTINFOV4_DATA_SIZE: usize = mem::size_of::<IN_PKTINFO>();
const PKTINFOV6_DATA_SIZE: usize = mem::size_of::<IN6_PKTINFO>();
const CONTROL_PKTINFOV4_BUFFER_SIZE: usize = CMSG_HEADER_SIZE + PKTINFOV4_DATA_SIZE;
const CONTROL_PKTINFOV6_BUFFER_SIZE: usize = CMSG_HEADER_SIZE + PKTINFOV6_DATA_SIZE + 8;

type WSARecvMsgExtension = unsafe extern "system" fn(
    s: SOCKET,
    lp_msg: *mut WSAMSG,
    lpdw_number_of_bytes_recvd: *mut u32,
    lp_overlapped: *mut OVERLAPPED,
    lp_completion_routine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
) -> i32;

fn locate_wsarecvmsg(socket: RawSocket) -> io::Result<WSARecvMsgExtension> {
    let mut fn_pointer: usize = 0;
    let mut byte_len: u32 = 0;

    let r = unsafe {
        WinSock::WSAIoctl(
            socket as _,
            SIO_GET_EXTENSION_FUNCTION_POINTER,
            &WSAID_WSARECVMSG as *const _ as *mut _,
            mem::size_of_val(&WSAID_WSARECVMSG) as u32,
            &mut fn_pointer as *const _ as *mut _,
            mem::size_of_val(&fn_pointer) as u32,
            &mut byte_len,
            ptr::null_mut(),
            None,
        )
    };
    if r != 0 {
        return Err(Error::last_os_error());
    }

    if mem::size_of::<LPFN_WSARECVMSG>() != byte_len as usize {
        return Err(io::Error::other(
            "Locating fn pointer to WSARecvMsg returned different expected bytes",
        ));
    }
    let cast_to_fn: LPFN_WSARECVMSG = unsafe { mem::transmute(fn_pointer) };

    match cast_to_fn {
        None => Err(io::Error::other("WSARecvMsg extension not found")),
        Some(extension) => Ok(extension),
    }
}

unsafe fn setsockopt<T>(socket: RawSocket, opt: i32, val: i32, payload: T) -> io::Result<()>
where
    T: Copy,
{
    let payload = &payload as *const T as PCSTR;
    if WinSock::setsockopt(socket as _, opt, val, payload, mem::size_of::<T>() as i32) == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(WinSock::WSAGetLastError()))
    }
}

#[derive(Clone)]
pub(crate) struct PktInfoRetrievalData {
    port: u16,
    is_ipv6: bool,
    wsarecvmsg: WSARecvMsgExtension,
}

pub(crate) fn enable_pktinfo(socket: &UdpSocket) -> io::Result<PktInfoRetrievalData> {
    let local_src_addr = socket.local_addr()?;
    match local_src_addr.is_ipv6() {
        false => unsafe {
            setsockopt(socket.as_raw_socket(), IPPROTO_IP, IP_PKTINFO, 1)?;
        },
        true => unsafe {
            setsockopt(socket.as_raw_socket(), IPPROTO_IPV6, IPV6_PKTINFO, 1)?;
        },
    }
    Ok(PktInfoRetrievalData {
        port: local_src_addr.port(),
        is_ipv6: local_src_addr.is_ipv6(),
        wsarecvmsg: locate_wsarecvmsg(socket.as_raw_socket())?,
    })
}

pub fn recv_with_dst_inner(
    socket: RawSocket,
    local_port: u16,
    is_ipv6: bool,
    wsarecvmsg: WSARecvMsgExtension,
    buf: &mut [u8],
) -> io::Result<(usize, SocketAddr, Option<SocketAddr>)> {
    let mut data = WSABUF {
        buf: buf.as_mut_ptr() as PSTR,
        len: buf.len() as u32,
    };

    let mut control_buffer = [0; CONTROL_PKTINFOV6_BUFFER_SIZE]; // Allocate the largest possible buffer
    let control = WSABUF {
        buf: control_buffer.as_mut_ptr(),
        len: match is_ipv6 {
            false => CONTROL_PKTINFOV4_BUFFER_SIZE as u32,
            true => CONTROL_PKTINFOV6_BUFFER_SIZE as u32,
        },
    };

    let mut addr = unsafe { mem::zeroed() };
    let mut wsa_msg = WSAMSG {
        name: &mut addr as *mut _ as *mut _,
        namelen: mem::size_of_val(&addr) as i32,
        lpBuffers: &mut data,
        Control: control,
        dwBufferCount: 1,
        dwFlags: 0,
    };

    let mut read_bytes = 0;
    let error_code = {
        unsafe {
            (wsarecvmsg)(
                socket as _,
                &mut wsa_msg,
                &mut read_bytes,
                ptr::null_mut(),
                None,
            )
        }
    };

    if error_code != 0 {
        return Err(io::Error::last_os_error());
    }

    let addr_src = unsafe { SockAddr::new(addr, mem::size_of_val(&addr) as i32) }
        .as_socket()
        .unwrap();

    let mut addr_dst = None;

    if control.len as usize == CONTROL_PKTINFOV4_BUFFER_SIZE {
        let cmsg_header: CMSGHDR = unsafe { ptr::read_unaligned(control.buf as *const _) };
        if cmsg_header.cmsg_level == IPPROTO_IP && cmsg_header.cmsg_type == IP_PKTINFO {
            let interface_info: IN_PKTINFO =
                unsafe { ptr::read_unaligned(control.buf.add(CMSG_HEADER_SIZE) as *const _) };

            addr_dst = Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::from(unsafe {
                    u32::from_be(interface_info.ipi_addr.S_un.S_addr)
                })),
                local_port,
            ));
        }
    } else if control.len as usize == CONTROL_PKTINFOV6_BUFFER_SIZE {
        let cmsg_header: CMSGHDR = unsafe { ptr::read_unaligned(control.buf as *const _) };
        if cmsg_header.cmsg_level == IPPROTO_IPV6 && cmsg_header.cmsg_type == IPV6_PKTINFO {
            let interface_info: IN6_PKTINFO =
                unsafe { ptr::read_unaligned(control.buf.add(CMSG_HEADER_SIZE) as *const _) };

            addr_dst = Some(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::from(unsafe { interface_info.ipi6_addr.u.Byte })),
                local_port,
            ));
        }
    }
    Ok((read_bytes as usize, addr_src, addr_dst))
}

pub(crate) async fn recv_with_dst(
    socket: &UdpSocket,
    data: &PktInfoRetrievalData,
    buffer: &mut [u8],
) -> io::Result<(usize, SocketAddr, Option<SocketAddr>)> {
    let fd = socket.as_raw_socket();
    let local_port = data.port;
    let wsarecvmsg = data.wsarecvmsg;
    let is_ipv6 = data.is_ipv6;

    socket
        .async_io(Interest::READABLE, || {
            recv_with_dst_inner(fd, local_port, is_ipv6, wsarecvmsg, buffer)
        })
        .await
}
