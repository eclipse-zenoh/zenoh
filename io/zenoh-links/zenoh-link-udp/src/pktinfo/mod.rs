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

#[cfg(target_family = "unix")]
mod pktinfo_unix;
use std::{io, net::SocketAddr, sync::Arc};

#[cfg(target_family = "unix")]
use pktinfo_unix::*;

#[cfg(target_family = "windows")]
mod pktinfo_windows;
#[cfg(target_family = "windows")]
use pktinfo_windows::*;

#[cfg(all(not(target_family = "windows"), not(target_family = "unix")))]
mod pktinfo_generic;
#[cfg(all(not(target_family = "windows"), not(target_family = "unix")))]
use pktinfo_generic::*;
use tokio::net::UdpSocket;

#[derive(Clone)]
pub(crate) struct PktInfoUdpSocket {
    pub(crate) socket: Arc<UdpSocket>,
    pktinfo_retrieval_data: PktInfoRetrievalData,
    local_address: SocketAddr,
}

impl PktInfoUdpSocket {
    pub(crate) fn new(socket: Arc<UdpSocket>) -> io::Result<PktInfoUdpSocket> {
        let pktinfo_retrieval_data = enable_pktinfo(&socket)?;
        let local_address = socket.local_addr()?;
        Ok(PktInfoUdpSocket {
            socket,
            pktinfo_retrieval_data,
            local_address,
        })
    }

    pub(crate) async fn receive(
        &self,
        buffer: &mut [u8],
    ) -> io::Result<(usize, SocketAddr, SocketAddr)> {
        let res = recv_with_dst(&self.socket, &self.pktinfo_retrieval_data, buffer).await?;

        let mut src_addr = self.local_address;
        if src_addr.ip().is_unspecified() {
            if let Some(addr) = res.2 {
                src_addr = addr;
            }
        }
        Ok((res.0, res.1, src_addr))
    }

    pub(crate) async fn send_to(
        &self,
        buffer: &[u8],
        dst_addr: &SocketAddr,
        src_addr: &SocketAddr,
    ) -> io::Result<usize> {
        #[cfg(target_family = "unix")]
        {
            send_with_src(&self.socket, buffer, dst_addr, src_addr).await
        }
        #[cfg(not(target_family = "unix"))]
        {
            // Fallback for non-unix platforms
            let _ = src_addr;
            self.socket.send_to(buffer, dst_addr).await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::{IpAddr, Ipv4Addr, SocketAddr},
        sync::Arc,
    };

    use tokio::net::UdpSocket;

    use super::*;

    #[tokio::test]
    async fn test_pktinfo_source_ip_consistency() {
        let server_socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        let server_addr = server_socket.local_addr().unwrap();
        let server_pktinfo = PktInfoUdpSocket::new(Arc::new(server_socket)).unwrap();

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client_target_addr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), server_addr.port());

        // The connect() call makes the OS kernel strict about the source IP of incoming packets.
        client_socket.connect(client_target_addr).await.unwrap();

        client_socket.send(b"request").await.unwrap();

        let mut buf = [0u8; 1024];
        let (size, client_addr, captured_local_ip) =
            server_pktinfo.receive(&mut buf).await.unwrap();
        assert_eq!(&buf[..size], b"request");
        assert_eq!(
            captured_local_ip.ip(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
        );

        // Server replies using send_to with IP_PKTINFO
        server_pktinfo
            .send_to(b"response", &client_addr, &captured_local_ip)
            .await
            .unwrap();

        let mut resp_buf = [0u8; 1024];
        let n = tokio::time::timeout(std::time::Duration::from_secs(1), client_socket.recv(&mut resp_buf))
            .await
            .expect("Timeout: Client did not receive the response. This usually means the source IP drifted and the packet was dropped by the OS kernel.")
            .unwrap();

        assert_eq!(&resp_buf[..n], b"response");
    }
}
