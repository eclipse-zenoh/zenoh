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
use crate::core::{ZError, ZErrorKind, ZResult};
use crate::{zconfigurable, zerror};
use async_std::net::TcpStream;
use std::net::IpAddr;
use std::time::Duration;

zconfigurable! {
    static ref WINDOWS_GET_ADAPTERS_ADDRESSES_BUF_SIZE: u32 = 8192;
    static ref WINDOWS_GET_ADAPTERS_ADDRESSES_MAX_RETRIES: u32 = 3;
}

pub fn set_linger(socket: &TcpStream, dur: Option<Duration>) -> ZResult<()> {
    #[cfg(unix)]
    {
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
                err_code => zerror!(ZErrorKind::IoError {
                    descr: format!("setsockopt returned {}", err_code)
                }),
            }
        }
    }

    #[cfg(windows)]
    {
        use std::convert::TryInto;
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
                err_code => zerror!(ZErrorKind::IoError {
                    descr: format!("setsockopt returned {}", err_code)
                }),
            }
        }
    }
}

pub fn get_interface(name: &str) -> ZResult<Option<IpAddr>> {
    #[cfg(unix)]
    {
        for iface in pnet::datalink::interfaces() {
            if iface.name == name {
                for ifaddr in &iface.ips {
                    if ifaddr.is_ipv4() {
                        return Ok(Some(ifaddr.ip()));
                    }
                }
            }
            for ifaddr in &iface.ips {
                if ifaddr.ip().to_string() == name {
                    return Ok(Some(ifaddr.ip()));
                }
            }
        }
        Ok(None)
    }

    #[cfg(windows)]
    {
        unsafe {
            use crate::ffi;
            use std::convert::TryInto;
            use winapi::um::iptypes::IP_ADAPTER_ADDRESSES_LH;

            let mut ret;
            let mut retries = 0;
            let mut size: u32 = *WINDOWS_GET_ADAPTERS_ADDRESSES_BUF_SIZE;
            let mut buffer: Vec<u8>;
            loop {
                buffer = Vec::with_capacity(size as usize);
                ret = winapi::um::iphlpapi::GetAdaptersAddresses(
                    winapi::shared::ws2def::AF_INET.try_into().unwrap(),
                    0,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                    &mut size,
                );
                if ret != winapi::shared::winerror::ERROR_BUFFER_OVERFLOW {
                    break;
                }
                if retries >= *WINDOWS_GET_ADAPTERS_ADDRESSES_MAX_RETRIES {
                    break;
                }
                retries += 1;
            }

            if ret != 0 {
                return zerror!(ZErrorKind::IoError {
                    descr: format!("GetAdaptersAddresses returned {}", ret)
                });
            }

            let mut next_iface = (buffer.as_ptr() as *mut IP_ADAPTER_ADDRESSES_LH).as_ref();
            while let Some(iface) = next_iface {
                if name == ffi::pstr_to_string(iface.AdapterName)
                    || name == ffi::pwstr_to_string(iface.FriendlyName)
                    || name == ffi::pwstr_to_string(iface.Description)
                {
                    let mut next_ucast_addr = iface.FirstUnicastAddress.as_ref();
                    while let Some(ucast_addr) = next_ucast_addr {
                        if let Ok(ifaddr) = ffi::win::sockaddr_to_addr(ucast_addr.Address) {
                            if ifaddr.is_ipv4() {
                                return Ok(Some(ifaddr.ip()));
                            }
                        }
                        next_ucast_addr = ucast_addr.Next.as_ref();
                    }
                }

                let mut next_ucast_addr = iface.FirstUnicastAddress.as_ref();
                while let Some(ucast_addr) = next_ucast_addr {
                    if let Ok(ifaddr) = ffi::win::sockaddr_to_addr(ucast_addr.Address) {
                        if ifaddr.ip().to_string() == name {
                            return Ok(Some(ifaddr.ip()));
                        }
                    }
                    next_ucast_addr = ucast_addr.Next.as_ref();
                }
                next_iface = iface.Next.as_ref();
            }
            Ok(None)
        }
    }
}

/// Get the network interface to bind the UDP sending port to when not specified by user
pub fn get_multicast_interfaces() -> Vec<IpAddr> {
    #[cfg(unix)]
    {
        pnet::datalink::interfaces()
            .iter()
            .filter_map(|iface| {
                if iface.is_up() && iface.is_multicast() {
                    for ipaddr in &iface.ips {
                        if ipaddr.is_ipv4() {
                            return Some(ipaddr.ip());
                        }
                    }
                }
                None
            })
            .collect()
    }
    #[cfg(windows)]
    {
        // On windows, bind to 0.0.0.0, the system will select the default interface
        vec![IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0))]
    }
}

pub fn get_local_addresses() -> ZResult<Vec<IpAddr>> {
    #[cfg(unix)]
    {
        Ok(pnet::datalink::interfaces()
            .into_iter()
            .map(|iface| iface.ips)
            .flatten()
            .map(|ipnet| ipnet.ip())
            .collect())
    }

    #[cfg(windows)]
    {
        unsafe {
            use crate::ffi;
            use std::convert::TryInto;
            use winapi::um::iptypes::IP_ADAPTER_ADDRESSES_LH;

            let mut result = vec![];
            let mut ret;
            let mut retries = 0;
            let mut size: u32 = *WINDOWS_GET_ADAPTERS_ADDRESSES_BUF_SIZE;
            let mut buffer: Vec<u8>;
            loop {
                buffer = Vec::with_capacity(size as usize);
                ret = winapi::um::iphlpapi::GetAdaptersAddresses(
                    winapi::shared::ws2def::AF_INET.try_into().unwrap(),
                    0,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                    &mut size,
                );
                if ret != winapi::shared::winerror::ERROR_BUFFER_OVERFLOW {
                    break;
                }
                if retries >= *WINDOWS_GET_ADAPTERS_ADDRESSES_MAX_RETRIES {
                    break;
                }
                retries += 1;
            }

            if ret != 0 {
                return zerror!(ZErrorKind::IoError {
                    descr: format!("GetAdaptersAddresses returned {}", ret)
                });
            }

            let mut next_iface = (buffer.as_ptr() as *mut IP_ADAPTER_ADDRESSES_LH).as_ref();
            while let Some(iface) = next_iface {
                let mut next_ucast_addr = iface.FirstUnicastAddress.as_ref();
                while let Some(ucast_addr) = next_ucast_addr {
                    if let Ok(ifaddr) = ffi::win::sockaddr_to_addr(ucast_addr.Address) {
                        result.push(ifaddr.ip());
                    }
                    next_ucast_addr = ucast_addr.Next.as_ref();
                }
                next_iface = iface.Next.as_ref();
            }
            Ok(result)
        }
    }
}
