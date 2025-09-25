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
use std::net::{IpAddr, Ipv6Addr};

#[cfg(unix)]
use lazy_static::lazy_static;
#[cfg(unix)]
use pnet_datalink::NetworkInterface;
use tokio::net::{TcpSocket, UdpSocket};
use zenoh_core::zconfigurable;
#[cfg(unix)]
use zenoh_result::zerror;
use zenoh_result::{bail, ZResult};

zconfigurable! {
    static ref WINDOWS_GET_ADAPTERS_ADDRESSES_BUF_SIZE: u32 = 8192;
    static ref WINDOWS_GET_ADAPTERS_ADDRESSES_MAX_RETRIES: u32 = 3;
}

#[cfg(unix)]
lazy_static! {
    static ref IFACES: Vec<NetworkInterface> = pnet_datalink::interfaces();
}

#[cfg(windows)]
unsafe fn get_adapters_addresses(af_spec: i32) -> ZResult<Vec<u8>> {
    use winapi::um::iptypes::IP_ADAPTER_ADDRESSES_LH;

    let mut ret;
    let mut retries = 0;
    let mut size: u32 = *WINDOWS_GET_ADAPTERS_ADDRESSES_BUF_SIZE;
    let mut buffer: Vec<u8>;
    loop {
        buffer = Vec::with_capacity(size as usize);
        ret = winapi::um::iphlpapi::GetAdaptersAddresses(
            af_spec.try_into().unwrap(),
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
        bail!("GetAdaptersAddresses returned {}", ret)
    }

    Ok(buffer)
}
pub fn get_interface(name: &str) -> ZResult<Option<IpAddr>> {
    #[cfg(unix)]
    {
        for iface in IFACES.iter() {
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
            use winapi::um::iptypes::IP_ADAPTER_ADDRESSES_LH;

            use crate::ffi;

            let buffer = get_adapters_addresses(winapi::shared::ws2def::AF_INET)?;

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
        IFACES
            .iter()
            .filter_map(|iface| {
                if iface.is_up() && iface.is_running() && iface.is_multicast() {
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
        // On windows, bind to [::], the system will select the default interface
        vec![IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)]
    }
}

pub fn get_local_addresses(interface: Option<&str>) -> ZResult<Vec<IpAddr>> {
    #[cfg(unix)]
    {
        Ok(IFACES
            .iter()
            .filter(|iface| {
                if let Some(interface) = interface.as_ref() {
                    if iface.name != *interface {
                        return false;
                    }
                }
                iface.is_up() && iface.is_running()
            })
            .flat_map(|iface| iface.ips.clone())
            .map(|ipnet| ipnet.ip())
            .collect())
    }

    #[cfg(windows)]
    {
        unsafe {
            use winapi::um::iptypes::IP_ADAPTER_ADDRESSES_LH;

            use crate::ffi;

            let buffer = get_adapters_addresses(winapi::shared::ws2def::AF_UNSPEC)?;

            let mut result = vec![];
            let mut next_iface = (buffer.as_ptr() as *mut IP_ADAPTER_ADDRESSES_LH).as_ref();
            while let Some(iface) = next_iface {
                if let Some(interface) = interface.as_ref() {
                    if ffi::pstr_to_string(iface.AdapterName) != *interface {
                        continue;
                    }
                }
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

/// Get the network interface to bind the UDP sending port to when not specified by user
pub fn get_unicast_addresses_of_multicast_interfaces() -> Vec<IpAddr> {
    #[cfg(unix)]
    {
        IFACES
            .iter()
            .filter(|iface| iface.is_up() && iface.is_running() && iface.is_multicast())
            .flat_map(|iface| {
                iface
                    .ips
                    .iter()
                    .filter(|ip| !ip.ip().is_multicast())
                    .map(|x| x.ip())
                    .collect::<Vec<IpAddr>>()
            })
            .collect()
    }
    #[cfg(windows)]
    {
        // On windows, bind to [::] or [::], the system will select the default interface
        vec![]
    }
}

pub fn get_unicast_addresses_of_interface(name: &str) -> ZResult<Vec<IpAddr>> {
    #[cfg(unix)]
    {
        match IFACES.iter().find(|iface| iface.name == name) {
            Some(iface) => {
                if !iface.is_up() {
                    bail!("Interface {name} is not up");
                }
                if !iface.is_running() {
                    bail!("Interface {name} is not running");
                }
                let addrs = iface
                    .ips
                    .iter()
                    .filter(|ip| !ip.ip().is_multicast())
                    .map(|x| x.ip())
                    .collect::<Vec<IpAddr>>();
                Ok(addrs)
            }
            None => bail!("Interface {name} not found"),
        }
    }

    #[cfg(windows)]
    {
        unsafe {
            use winapi::um::iptypes::IP_ADAPTER_ADDRESSES_LH;

            use crate::ffi;

            let buffer = get_adapters_addresses(winapi::shared::ws2def::AF_INET)?;

            let mut addrs = vec![];
            let mut next_iface = (buffer.as_ptr() as *mut IP_ADAPTER_ADDRESSES_LH).as_ref();
            while let Some(iface) = next_iface {
                if name == ffi::pstr_to_string(iface.AdapterName)
                    || name == ffi::pwstr_to_string(iface.FriendlyName)
                    || name == ffi::pwstr_to_string(iface.Description)
                {
                    let mut next_ucast_addr = iface.FirstUnicastAddress.as_ref();
                    while let Some(ucast_addr) = next_ucast_addr {
                        if let Ok(ifaddr) = ffi::win::sockaddr_to_addr(ucast_addr.Address) {
                            addrs.push(ifaddr.ip());
                        }
                        next_ucast_addr = ucast_addr.Next.as_ref();
                    }
                }
                next_iface = iface.Next.as_ref();
            }
            Ok(addrs)
        }
    }
}

pub fn get_index_of_interface(addr: IpAddr) -> ZResult<u32> {
    #[cfg(unix)]
    {
        IFACES
            .iter()
            .find(|iface| iface.ips.iter().any(|ipnet| ipnet.ip() == addr))
            .map(|iface| iface.index)
            .ok_or_else(|| zerror!("No interface found with address {addr}").into())
    }
    #[cfg(windows)]
    {
        unsafe {
            use winapi::um::iptypes::IP_ADAPTER_ADDRESSES_LH;

            use crate::ffi;

            let buffer = get_adapters_addresses(winapi::shared::ws2def::AF_INET)?;

            let mut next_iface = (buffer.as_ptr() as *mut IP_ADAPTER_ADDRESSES_LH).as_ref();
            while let Some(iface) = next_iface {
                let mut next_ucast_addr = iface.FirstUnicastAddress.as_ref();
                while let Some(ucast_addr) = next_ucast_addr {
                    if let Ok(ifaddr) = ffi::win::sockaddr_to_addr(ucast_addr.Address) {
                        if ifaddr.ip() == addr {
                            return Ok(iface.Ipv6IfIndex);
                        }
                    }
                    next_ucast_addr = ucast_addr.Next.as_ref();
                }
                next_iface = iface.Next.as_ref();
            }
            bail!("No interface found with address {addr}")
        }
    }
}

pub fn get_interface_names_by_addr(addr: IpAddr) -> ZResult<Vec<String>> {
    #[cfg(unix)]
    {
        if addr.is_unspecified() {
            Ok(IFACES
                .iter()
                .map(|iface| iface.name.clone())
                .collect::<Vec<String>>())
        } else {
            let addr = addr.to_canonical();
            Ok(IFACES
                .iter()
                .filter(|iface| iface.ips.iter().any(|ipnet| ipnet.ip() == addr))
                .map(|iface| iface.name.clone())
                .collect::<Vec<String>>())
        }
    }
    #[cfg(windows)]
    {
        let mut result = vec![];
        unsafe {
            use winapi::um::iptypes::IP_ADAPTER_ADDRESSES_LH;

            use crate::ffi;

            let buffer = get_adapters_addresses(winapi::shared::ws2def::AF_UNSPEC)?;

            if addr.is_unspecified() {
                let mut next_iface = (buffer.as_ptr() as *mut IP_ADAPTER_ADDRESSES_LH).as_ref();
                while let Some(iface) = next_iface {
                    result.push(ffi::pstr_to_string(iface.AdapterName));
                    next_iface = iface.Next.as_ref();
                }
            } else {
                let addr = addr.to_canonical();
                let mut next_iface = (buffer.as_ptr() as *mut IP_ADAPTER_ADDRESSES_LH).as_ref();
                while let Some(iface) = next_iface {
                    let mut next_ucast_addr = iface.FirstUnicastAddress.as_ref();
                    while let Some(ucast_addr) = next_ucast_addr {
                        if let Ok(ifaddr) = ffi::win::sockaddr_to_addr(ucast_addr.Address) {
                            if ifaddr.ip() == addr {
                                result.push(ffi::pstr_to_string(iface.AdapterName));
                            }
                        }
                        next_ucast_addr = ucast_addr.Next.as_ref();
                    }
                    next_iface = iface.Next.as_ref();
                }
            }
        }
        Ok(result)
    }
}

pub fn get_ipv4_ipaddrs(interface: Option<&str>) -> Vec<IpAddr> {
    get_local_addresses(interface)
        .unwrap_or_else(|_| vec![])
        .drain(..)
        .filter_map(|x| match x {
            IpAddr::V4(a) => Some(a),
            IpAddr::V6(_) => None,
        })
        .filter(|x| !x.is_loopback() && !x.is_multicast())
        .map(IpAddr::V4)
        .collect()
}

pub fn get_ipv6_ipaddrs(interface: Option<&str>) -> Vec<IpAddr> {
    const fn is_unicast_link_local(addr: &Ipv6Addr) -> bool {
        (addr.segments()[0] & 0xffc0) == 0xfe80
    }

    let ipaddrs = get_local_addresses(interface).unwrap_or_else(|_| vec![]);

    // Get first all IPv4 addresses
    let ipv4_iter = ipaddrs
        .iter()
        .filter_map(|x| match x {
            IpAddr::V4(a) => Some(a),
            IpAddr::V6(_) => None,
        })
        .filter(|x| {
            !x.is_loopback() && !x.is_link_local() && !x.is_multicast() && !x.is_broadcast()
        });

    // Get next all IPv6 addresses
    let ipv6_iter = ipaddrs.iter().filter_map(|x| match x {
        IpAddr::V4(_) => None,
        IpAddr::V6(a) => Some(a),
    });

    // First match non-linklocal IPv6 addresses
    let nll_ipv6_addrs = ipv6_iter
        .clone()
        .filter(|x| !x.is_loopback() && !x.is_multicast() && !is_unicast_link_local(x))
        .map(|x| IpAddr::V6(*x));

    // Second match public IPv4 addresses
    let pub_ipv4_addrs = ipv4_iter
        .clone()
        .filter(|x| !x.is_private())
        .map(|x| IpAddr::V4(*x));

    // Third match linklocal IPv6 addresses
    let yll_ipv6_addrs = ipv6_iter
        .filter(|x| !x.is_loopback() && !x.is_multicast() && is_unicast_link_local(x))
        .map(|x| IpAddr::V6(*x));

    // Fourth match private IPv4 addresses
    let priv_ipv4_addrs = ipv4_iter
        .clone()
        .filter(|x| x.is_private())
        .map(|x| IpAddr::V4(*x));

    // Extend
    nll_ipv6_addrs
        .chain(pub_ipv4_addrs)
        .chain(yll_ipv6_addrs)
        .chain(priv_ipv4_addrs)
        .collect()
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn set_bind_to_device_tcp_socket(socket: &TcpSocket, iface: &str) -> ZResult<()> {
    socket.bind_device(Some(iface.as_bytes()))?;
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn set_bind_to_device_udp_socket(socket: &UdpSocket, iface: &str) -> ZResult<()> {
    socket.bind_device(Some(iface.as_bytes()))?;
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn set_bind_to_device_tcp_socket(socket: &TcpSocket, iface: &str) -> ZResult<()> {
    tracing::warn!("Binding the socket {socket:?} to the interface {iface} is not supported on macOS and Windows");
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn set_bind_to_device_udp_socket(socket: &UdpSocket, iface: &str) -> ZResult<()> {
    tracing::warn!("Binding the socket {socket:?} to the interface {iface} is not supported on macOS and Windows");
    Ok(())
}
