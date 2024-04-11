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
use async_trait::async_trait;
use std::cell::UnsafeCell;
use std::convert::TryInto;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use zenoh_link_commons::{
    get_ip_interface_names, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait,
    ListenersUnicastIP, NewLinkChannelSender, BIND_INTERFACE,
};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{bail, zerror, Error as ZError, ZResult};

use super::{
    get_tcp_addrs, TCP_ACCEPT_THROTTLE_TIME, TCP_DEFAULT_MTU, TCP_LINGER_TIMEOUT,
    TCP_LOCATOR_PREFIX,
};
use tokio::net::{TcpListener, TcpSocket, TcpStream};

pub struct LinkUnicastTcp {
    // The underlying socket as returned from the tokio library
    socket: UnsafeCell<TcpStream>,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    src_locator: Locator,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
    dst_locator: Locator,
}

unsafe impl Sync for LinkUnicastTcp {}

impl LinkUnicastTcp {
    fn new(socket: TcpStream, src_addr: SocketAddr, dst_addr: SocketAddr) -> LinkUnicastTcp {
        // Set the TCP nodelay option
        if let Err(err) = socket.set_nodelay(true) {
            tracing::warn!(
                "Unable to set NODEALY option on TCP link {} => {}: {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Set the TCP linger option
        if let Err(err) = socket.set_linger(Some(Duration::from_secs(
            (*TCP_LINGER_TIMEOUT).try_into().unwrap(),
        ))) {
            tracing::warn!(
                "Unable to set LINGER option on TCP link {} => {}: {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Build the Tcp object
        LinkUnicastTcp {
            socket: UnsafeCell::new(socket),
            src_addr,
            src_locator: Locator::new(TCP_LOCATOR_PREFIX, src_addr.to_string(), "").unwrap(),
            dst_addr,
            dst_locator: Locator::new(TCP_LOCATOR_PREFIX, dst_addr.to_string(), "").unwrap(),
        }
    }
    #[allow(clippy::mut_from_ref)]
    fn get_mut_socket(&self) -> &mut TcpStream {
        unsafe { &mut *self.socket.get() }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastTcp {
    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing TCP link: {}", self);
        // Close the underlying TCP socket
        self.get_mut_socket().shutdown().await.map_err(|e| {
            let e = zerror!("TCP link shutdown {}: {:?}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        self.get_mut_socket().write(buffer).await.map_err(|e| {
            let e = zerror!("Write error on TCP link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        self.get_mut_socket().write_all(buffer).await.map_err(|e| {
            let e = zerror!("Write error on TCP link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        self.get_mut_socket().read(buffer).await.map_err(|e| {
            let e = zerror!("Read error on TCP link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let _ = self
            .get_mut_socket()
            .read_exact(buffer)
            .await
            .map_err(|e| {
                let e = zerror!("Read error on TCP link {}: {}", self, e);
                tracing::trace!("{}", e);
                e
            })?;
        Ok(())
    }

    #[inline(always)]
    fn get_src(&self) -> &Locator {
        &self.src_locator
    }

    #[inline(always)]
    fn get_dst(&self) -> &Locator {
        &self.dst_locator
    }

    #[inline(always)]
    fn get_mtu(&self) -> u16 {
        *TCP_DEFAULT_MTU
    }

    #[inline(always)]
    fn get_interface_names(&self) -> Vec<String> {
        get_ip_interface_names(&self.src_addr)
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        true
    }

    #[inline(always)]
    fn is_streamed(&self) -> bool {
        true
    }
}

// // WARN: This sometimes causes timeout in routing test
// // WARN assume the drop of TcpStream would clean itself
// // https://docs.rs/tokio/latest/tokio/net/struct.TcpStream.html#method.into_split
// impl Drop for LinkUnicastTcp {
//     fn drop(&mut self) {
//         // Close the underlying TCP socket
//         zenoh_runtime::ZRuntime::Acceptor.block_in_place(async {
//             let _ = self.get_mut_socket().shutdown().await;
//         });
//     }
// }

impl fmt::Display for LinkUnicastTcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastTcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tcp")
            .field("src", &self.src_addr)
            .field("dst", &self.dst_addr)
            .finish()
    }
}

pub struct LinkManagerUnicastTcp {
    manager: NewLinkChannelSender,
    listeners: ListenersUnicastIP,
}

impl LinkManagerUnicastTcp {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: ListenersUnicastIP::new(),
        }
    }
}

impl LinkManagerUnicastTcp {
    async fn new_link_inner(
        &self,
        dst_addr: &SocketAddr,
        iface: Option<&str>,
    ) -> ZResult<(TcpStream, SocketAddr, SocketAddr)> {
        let socket = match dst_addr {
            SocketAddr::V4(_) => TcpSocket::new_v4(),
            SocketAddr::V6(_) => TcpSocket::new_v6(),
        }?;

        if let Some(iface) = iface {
            zenoh_util::net::set_bind_to_device_tcp_socket(&socket, iface)?;
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

    async fn new_listener_inner(
        &self,
        addr: &SocketAddr,
        iface: Option<&str>,
    ) -> ZResult<(TcpListener, SocketAddr)> {
        let socket = match addr {
            SocketAddr::V4(_) => TcpSocket::new_v4(),
            SocketAddr::V6(_) => TcpSocket::new_v6(),
        }?;

        if let Some(iface) = iface {
            zenoh_util::net::set_bind_to_device_tcp_socket(&socket, iface)?;
        }

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
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastTcp {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let dst_addrs = get_tcp_addrs(endpoint.address()).await?;
        let config = endpoint.config();
        let iface = config.get(BIND_INTERFACE);

        let mut errs: Vec<ZError> = vec![];
        for da in dst_addrs {
            match self.new_link_inner(&da, iface).await {
                Ok((stream, src_addr, dst_addr)) => {
                    let link = Arc::new(LinkUnicastTcp::new(stream, src_addr, dst_addr));
                    return Ok(LinkUnicast(link));
                }
                Err(e) => {
                    errs.push(e);
                }
            }
        }

        if errs.is_empty() {
            errs.push(zerror!("No TCP unicast addresses available").into());
        }

        bail!(
            "Can not create a new TCP link bound to {}: {:?}",
            endpoint,
            errs
        )
    }

    async fn new_listener(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let addrs = get_tcp_addrs(endpoint.address()).await?;
        let config = endpoint.config();
        let iface = config.get(BIND_INTERFACE);

        let mut errs: Vec<ZError> = vec![];
        for da in addrs {
            match self.new_listener_inner(&da, iface).await {
                Ok((socket, local_addr)) => {
                    // Update the endpoint locator address
                    endpoint = EndPoint::new(
                        endpoint.protocol(),
                        &format!("{local_addr}"),
                        endpoint.metadata(),
                        endpoint.config(),
                    )?;

                    let token = self.listeners.token.child_token();
                    let c_token = token.clone();

                    let c_manager = self.manager.clone();
                    let task = async move { accept_task(socket, c_token, c_manager).await };

                    let locator = endpoint.to_locator();
                    self.listeners
                        .add_listener(endpoint, local_addr, task, token)
                        .await?;

                    return Ok(locator);
                }
                Err(e) => {
                    errs.push(e);
                }
            }
        }

        if errs.is_empty() {
            errs.push(zerror!("No TCP unicast addresses available").into());
        }

        bail!(
            "Can not create a new TCP listener bound to {}: {:?}",
            endpoint,
            errs
        )
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let addrs = get_tcp_addrs(endpoint.address()).await?;

        // Stop the listener
        let mut errs: Vec<ZError> = vec![];
        let mut failed = true;
        for a in addrs {
            match self.listeners.del_listener(a).await {
                Ok(_) => {
                    failed = false;
                    break;
                }
                Err(err) => {
                    errs.push(zerror!("{}", err).into());
                }
            }
        }

        if failed {
            bail!(
                "Can not delete the TCP listener bound to {}: {:?}",
                endpoint,
                errs
            )
        }
        Ok(())
    }

    async fn get_listeners(&self) -> Vec<EndPoint> {
        self.listeners.get_endpoints()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        self.listeners.get_locators()
    }
}

async fn accept_task(
    socket: TcpListener,
    token: CancellationToken,
    manager: NewLinkChannelSender,
) -> ZResult<()> {
    async fn accept(socket: &TcpListener) -> ZResult<(TcpStream, SocketAddr)> {
        let res = socket.accept().await.map_err(|e| zerror!(e))?;
        Ok(res)
    }

    let src_addr = socket.local_addr().map_err(|e| {
        let e = zerror!("Can not accept TCP connections: {}", e);
        tracing::warn!("{}", e);
        e
    })?;

    tracing::trace!("Ready to accept TCP connections on: {:?}", src_addr);
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            res = accept(&socket) => {
                match res {
                    Ok((stream, dst_addr)) => {
                        tracing::debug!("Accepted TCP connection on {:?}: {:?}", src_addr, dst_addr);
                        // Create the new link object
                        let link = Arc::new(LinkUnicastTcp::new(stream, src_addr, dst_addr));

                        // Communicate the new link to the initial transport manager
                        if let Err(e) = manager.send_async(LinkUnicast(link)).await {
                            tracing::error!("{}-{}: {}", file!(), line!(), e)
                        }
                    },
                    Err(e) => {
                        tracing::warn!("{}. Hint: increase the system open file limit.", e);
                        // Throttle the accept loop upon an error
                        // NOTE: This might be due to various factors. However, the most common case is that
                        //       the process has reached the maximum number of open files in the system. On
                        //       Linux systems this limit can be changed by using the "ulimit" command line
                        //       tool. In case of systemd-based systems, this can be changed by using the
                        //       "sysctl" command line tool.
                        tokio::time::sleep(Duration::from_micros(*TCP_ACCEPT_THROTTLE_TIME)).await;
                    }

                }
            }
        };
    }

    Ok(())
}
