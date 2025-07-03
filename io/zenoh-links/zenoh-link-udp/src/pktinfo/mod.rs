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
}
