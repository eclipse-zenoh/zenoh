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

        if let Some(bind_addr) = self.bind_socket {
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

    /// Creates a TcpSocket with the provided config
    fn socket_with_config(&self, addr: &SocketAddr) -> ZResult<TcpSocket> {
        let socket = match addr {
            SocketAddr::V4(_) => TcpSocket::new_v4(),
            SocketAddr::V6(_) => TcpSocket::new_v6(),
        }?;

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
