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
use std::time::Duration;
use std::net::IpAddr;
use async_std::net::TcpStream;
use crate::zerror;
use crate::core::{ZResult, ZError, ZErrorKind};

pub fn set_linger(socket: &TcpStream, dur: Option<Duration>) -> ZResult<()> {
    #[cfg(unix)] {
        use std::os::unix::io::AsRawFd;

        let raw_socket = socket.as_raw_fd();
        let linger = match dur {
            Some(d) => libc::linger {
                l_onoff: 1,
                l_linger: d.as_secs() as libc::c_int,
            },
            None => libc::linger {
                l_onoff: 0,
                l_linger: 0,
            },
        };

        // Set the SO_LINGER option
        unsafe {
            let ret = libc::setsockopt(
                raw_socket,
                libc::SOL_SOCKET,
                libc::SO_LINGER,
                &linger as *const libc::linger as *const libc::c_void,
                std::mem::size_of_val(&linger) as libc::socklen_t,
            );
            match ret {
                0 => Ok(()),
                _ => zerror!(ZErrorKind::IOError { descr: "".to_string() }),
            }
        }
    }

    #[cfg(windows)] {
        use std::os::windows::io::AsRawSocket;
        use winapi::um::winsock2;
        use winapi::um::ws2tcpip;

        let raw_socket = socket.as_raw_socket();
        let linger = match dur {
            Some(d) => winsock2::linger {
                l_onoff: 1,
                l_linger: d.as_secs() as u16,
            },
            None => winsock2::linger {
                l_onoff: 0,
                l_linger: 0,
            },
        };

        unsafe {
            let ret = winsock2::setsockopt(
                raw_socket.try_into().unwrap(),
                winsock2::SOL_SOCKET,
                winsock2::SO_LINGER,
                &linger as *const winsock2::linger as *const i8,
                std::mem::size_of_val(&linger) as ws2tcpip::socklen_t,
            );
            match ret {
                0 => Ok(()),
                _ => zerror!(ZErrorKind::IOError { descr: "".to_string() }),
            }
        }
    }
}

pub fn get_interface(name: &str) -> Option<IpAddr> {
    for iface in pnet::datalink::interfaces() {
        if iface.name == name {
            for ip in &iface.ips {
                if ip.is_ipv4() { return Some(ip.ip()) }
            }
        }
        for ip in &iface.ips {
            if ip.ip().to_string() == name { return Some(ip.ip()) }
        }
    }
    None
}

/// Get the network interface to bind the UDP sending port to when not specified by user
pub fn get_default_multicast_interface() -> Option<IpAddr> {
    #[cfg(unix)] {
        // In unix family, return first active, non-loopback, multicast enabled interface
        for iface in pnet::datalink::interfaces() {
            if iface.is_up() && !iface.is_loopback() && iface.is_multicast() {
                for ipaddr in iface.ips {
                    if ipaddr.is_ipv4() { return Some(ipaddr.ip()) }
                }
            }
        }
        None
    }
    #[cfg(windows)] {
        // On windows, bind to 0.0.0.0, the system will select the default interface
        Some(IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)))
    }
}

pub fn get_local_addresses() -> Vec<IpAddr> {
    pnet::datalink::interfaces().into_iter().map(|iface| iface.ips)
        .flatten().map(|ipnet| ipnet.ip()).collect()
}