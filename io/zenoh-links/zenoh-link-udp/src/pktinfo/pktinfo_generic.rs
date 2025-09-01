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
use std::net::SocketAddr;

use tokio::net::UdpSocket;

#[derive(Clone)]
pub(crate) struct PktInfoRetrievalData;
pub(crate) fn enable_pktinfo(socket: &UdpSocket) -> io::Result<PktInfoRetrievalData> {
    if socket.local_addr()?.ip().is_unspecified() {
        tracing::warn!("PKTINFO can be only retrieved on windows and unix. Interceptors (e.g. Access Control, Downsampling) are not guaranteed to work on UDP when listening on 0.0.0.0 or [::]. Their usage is discouraged.");
    }
    Ok(PktInfoRetrievalData)
}

pub(crate) async fn recv_with_dst(
    socket: &UdpSocket,
    _data: &PktInfoRetrievalData,
    buffer: &mut [u8],
) -> io::Result<(usize, SocketAddr, Option<SocketAddr>)> {
    let res = socket.recv_from(buffer).await?;
    Ok((res.0, res.1, None))
}
