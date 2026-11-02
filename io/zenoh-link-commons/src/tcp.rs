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
use zenoh_protocol::core::endpoint::Address;
use zenoh_result::{bail, zerror, Error as ZError, ZResult};

use crate::{get_ip_addrs, set_dscp};

/// Resolves `address` to its socket addresses, in resolver order, skipping multicast ones.
pub async fn get_tcp_addrs(address: Address<'_>) -> ZResult<impl Iterator<Item = SocketAddr>> {
    Ok(get_ip_addrs(address)
        .await?
        .filter(|x| !x.ip().is_multicast()))
}

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

    /// Try TCP connections in iterator order, returning the first success or all connection errors.
    pub async fn new_link_first_reachable(
        &self,
        dst_addrs: impl Iterator<Item = SocketAddr>,
    ) -> ZResult<(TcpStream, SocketAddr, SocketAddr)> {
        let mut errs: Vec<ZError> = vec![];
        for da in dst_addrs {
            match self.new_link(&da).await {
                Ok(res) => return Ok(res),
                Err(e) => errs.push(e),
            }
        }
        if errs.is_empty() {
            errs.push(zerror!("No unicast addresses available").into());
        }
        bail!("{:?}", errs)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tcp_config() -> TcpSocketConfig<'static> {
        TcpSocketConfig::new(None, None, None, None, None)
    }

    // Keep listeners alive until all addresses are collected to prevent port reuse between binds.
    async fn unreachable_addrs<const N: usize>() -> [SocketAddr; N] {
        let mut listeners = vec![];
        for _ in 0..N {
            listeners.push(TcpListener::bind("127.0.0.1:0").await.unwrap());
        }
        core::array::from_fn(|i| listeners[i].local_addr().unwrap())
    }

    #[tokio::test]
    async fn connect_falls_through_to_reachable_addr() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let live = listener.local_addr().unwrap();
        let [dead] = unreachable_addrs().await;

        let (_stream, _src, dst) = tcp_config()
            .new_link_first_reachable([dead, live].into_iter())
            .await
            .unwrap();
        assert_eq!(dst, live);
    }

    #[tokio::test]
    async fn connect_failure_reports_every_addr() {
        let [dead1, dead2] = unreachable_addrs().await;

        let err = tcp_config()
            .new_link_first_reachable([dead1, dead2].into_iter())
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(&dead1.to_string()), "{msg}");
        assert!(msg.contains(&dead2.to_string()), "{msg}");
    }

    #[tokio::test]
    async fn connect_no_addrs_is_error() {
        let err = tcp_config()
            .new_link_first_reachable(std::iter::empty())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("No unicast addresses"));
    }

    #[tokio::test]
    async fn only_tcp_addrs_skip_multicast() {
        let mcast = "224.0.0.1:7447";
        let all: Vec<_> = get_ip_addrs(Address::from(mcast)).await.unwrap().collect();
        assert_eq!(all, [mcast.parse::<SocketAddr>().unwrap()]);
        assert_eq!(
            get_tcp_addrs(Address::from(mcast)).await.unwrap().count(),
            0
        );
    }
}
