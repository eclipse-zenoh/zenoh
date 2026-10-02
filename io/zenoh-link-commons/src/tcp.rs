//
// Copyright (c) 2024 ZettaScale Technology
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
use std::net::SocketAddr;

use tokio::net::{TcpListener, TcpSocket, TcpStream};
use zenoh_result::{zerror, ZResult};

use crate::set_dscp;

#[derive(Debug)]
pub struct TcpSocketConfig<'a> {
    tx_buffer_size: Option<u32>,
    rx_buffer_size: Option<u32>,
    iface: Option<&'a str>,
    bind_socket: Option<SocketAddr>,
    dscp: Option<u32>,
}

impl<'a> TcpSocketConfig<'a> {
    pub fn new(
        tx_buffer_size: Option<u32>,
        rx_buffer_size: Option<u32>,
        iface: Option<&'a str>,
        bind_socket: Option<SocketAddr>,
        dscp: Option<u32>,
    ) -> Self {
        Self {
            tx_buffer_size,
            rx_buffer_size,
            iface,
            bind_socket,
            dscp,
        }
    }

    /// Build a new TCPListener bound to `addr` with the given configuration parameters
    pub fn new_listener(&self, addr: &SocketAddr) -> ZResult<(TcpListener, SocketAddr)> {
        let socket = self.socket_with_config(addr)?;
        // Build a TcpListener from TcpSocket
        // https://docs.rs/tokio/latest/tokio/net/struct.TcpSocket.html
        socket.set_reuseaddr(true)?;
        let addr = &self.resolve_listen_addr(addr)?;
        socket.bind(*addr).map_err(|e| zerror!("{}: {}", addr, e))?;
        // backlog (the maximum number of pending connections are queued): 1024
        let listener = socket
            .listen(1024)
            .map_err(|e| zerror!("{}: {}", addr, e))?;

        let local_addr = listener
            .local_addr()
            .map_err(|e| zerror!("{}: {}", addr, e))?;

        Ok((listener, local_addr))
    }

    /// Connect to a TCP socket address at `dst_addr` with the given configuration parameters
    pub async fn new_link(
        &self,
        dst_addr: &SocketAddr,
    ) -> ZResult<(TcpStream, SocketAddr, SocketAddr)> {
        let socket = self.socket_with_config(dst_addr)?;

        if let Some(bind_addr) = self.resolve_bind_socket(dst_addr)? {
            match (bind_addr, dst_addr) {
                (SocketAddr::V6(local), SocketAddr::V4(dest)) => {
                    return Err(Box::from(format!(
                        "Protocols must match: Cannot bind to IPv6 {local} and connect to IPv4 {dest}",
                    )));
                }
                (SocketAddr::V4(local), SocketAddr::V6(dest)) => {
                    return Err(Box::from(format!(
                        "Protocols must match: Cannot bind to IPv4 {local} and connect to IPv6 {dest}",
                    )));
                }
                _ => (), // No issue here
            }
            socket
                .bind(bind_addr)
                .map_err(|e| zerror!("{}: {}", bind_addr, e))?;
        }

        // Build a TcpStream from TcpSocket
        // https://docs.rs/tokio/latest/tokio/net/struct.TcpSocket.html
        let stream = socket
            .connect(*dst_addr)
            .await
            .map_err(|e| zerror!("{}: {}", dst_addr, e))?;

        let src_addr = stream
            .local_addr()
            .map_err(|e| zerror!("{}: {}", dst_addr, e))?;

        let dst_addr = stream
            .peer_addr()
            .map_err(|e| zerror!("{}: {}", dst_addr, e))?;

        Ok((stream, src_addr, dst_addr))
    }

    /// Resolve the address to listen on, restricting egress/ingress to `self.iface` when set.
    /// FreeBSD has no SO_BINDTODEVICE equivalent, so the interface is selected by binding to
    /// its own address instead of a device-level sockopt (see `resolve_bind_addr_for_interface`).
    /// On other platforms the interface restriction is applied via `set_bind_to_device_tcp_socket`
    /// in `socket_with_config`, so the listen address is returned unchanged here.
    #[cfg(target_os = "freebsd")]
    fn resolve_listen_addr(&self, addr: &SocketAddr) -> ZResult<SocketAddr> {
        match self.iface {
            Some(iface) => zenoh_util::net::resolve_bind_addr_for_interface(iface, *addr),
            None => Ok(*addr),
        }
    }
    #[cfg(not(target_os = "freebsd"))]
    fn resolve_listen_addr(&self, addr: &SocketAddr) -> ZResult<SocketAddr> {
        Ok(*addr)
    }

    /// Resolve the address to bind before connect(), merging `self.iface`'s own address into
    /// `self.bind_socket` (or synthesizing one) on FreeBSD for the same reason as above.
    #[cfg(target_os = "freebsd")]
    fn resolve_bind_socket(&self, dst_addr: &SocketAddr) -> ZResult<Option<SocketAddr>> {
        match (self.iface, self.bind_socket) {
            (Some(iface), Some(bind_addr)) => {
                Ok(Some(zenoh_util::net::resolve_bind_addr_for_interface(
                    iface, bind_addr,
                )?))
            }
            (Some(iface), None) => {
                let unspec = if dst_addr.is_ipv6() {
                    SocketAddr::new(std::net::Ipv6Addr::UNSPECIFIED.into(), 0)
                } else {
                    SocketAddr::new(std::net::Ipv4Addr::UNSPECIFIED.into(), 0)
                };
                Ok(Some(zenoh_util::net::resolve_bind_addr_for_interface(
                    iface, unspec,
                )?))
            }
            (None, bind_socket) => Ok(bind_socket),
        }
    }
    #[cfg(not(target_os = "freebsd"))]
    fn resolve_bind_socket(&self, _dst_addr: &SocketAddr) -> ZResult<Option<SocketAddr>> {
        Ok(self.bind_socket)
    }

    /// Creates a TcpSocket with the provided config
    fn socket_with_config(&self, addr: &SocketAddr) -> ZResult<TcpSocket> {
        let socket = match addr {
            SocketAddr::V4(_) => TcpSocket::new_v4(),
            SocketAddr::V6(_) => TcpSocket::new_v6(),
        }?;

        #[cfg(not(target_os = "freebsd"))]
        if let Some(iface) = self.iface {
            zenoh_util::net::set_bind_to_device_tcp_socket(&socket, iface)?;
        }
        if let Some(size) = self.tx_buffer_size {
            socket.set_send_buffer_size(size)?;
        }
        if let Some(size) = self.rx_buffer_size {
            socket.set_recv_buffer_size(size)?;
        }
        if let Some(dscp) = self.dscp {
            set_dscp(&socket, *addr, dscp)?;
        }

        Ok(socket)
    }
}
