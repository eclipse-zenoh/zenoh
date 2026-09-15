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
#[cfg(all(feature = "uring", target_os = "linux"))]
use std::os::fd::{AsRawFd, RawFd};
use std::{cell::UnsafeCell, convert::TryInto, fmt, net::SocketAddr, sync::Arc, time::Duration};

use async_trait::async_trait;
#[cfg(all(feature = "uring", target_os = "linux"))]
use tokio::io::{unix::AsyncFd, Interest};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;
use zenoh_link_commons::{
    get_ip_interface_names, tcp::TcpSocketConfig, LinkAuthId, LinkManagerUnicastTrait, LinkUnicast,
    LinkUnicastTrait, ListenersUnicastIP, NewLinkChannelSender, BIND_INTERFACE, BIND_SOCKET,
};
use zenoh_protocol::{
    core::{EndPoint, Locator, Priority},
    transport::BatchSize,
};
use zenoh_result::{bail, zerror, Error as ZError, ZResult};

use crate::{
    get_tcp_addrs, utils::TcpLinkConfig, TCP_ACCEPT_THROTTLE_TIME, TCP_DEFAULT_MTU,
    TCP_LINGER_TIMEOUT, TCP_LOCATOR_PREFIX,
};

/// The socket behind a TCP link.
///
/// `Tokio` is the regular tokio stream (reads and writes through the tokio
/// reactor). `WriteOnly` is used once io_uring has taken over the RX side: the
/// same fd is re-registered with the reactor for WRITABLE interest only, so
/// incoming data no longer wakes the runtime that accepted/connected the socket.
enum TcpSocket {
    Tokio(TcpStream),
    #[cfg(all(feature = "uring", target_os = "linux"))]
    WriteOnly(AsyncFd<std::net::TcpStream>),
    /// Transient state while converting `Tokio` -> `WriteOnly`; also what is
    /// left behind if that conversion fails midway (the link then errors out).
    #[cfg(all(feature = "uring", target_os = "linux"))]
    Detached,
}

#[cfg(all(feature = "uring", target_os = "linux"))]
fn detached_error() -> std::io::Error {
    std::io::Error::other("socket RX is owned by io_uring")
}

#[cfg(all(feature = "uring", target_os = "linux"))]
async fn write_nonblocking(
    fd: &AsyncFd<std::net::TcpStream>,
    buffer: &[u8],
) -> std::io::Result<usize> {
    use std::io::Write;
    fd.async_io(Interest::WRITABLE, |mut stream| stream.write(buffer))
        .await
}

#[cfg(all(feature = "uring", target_os = "linux"))]
async fn write_all_nonblocking(
    fd: &AsyncFd<std::net::TcpStream>,
    mut buffer: &[u8],
) -> std::io::Result<()> {
    while !buffer.is_empty() {
        let n = write_nonblocking(fd, buffer).await?;
        if n == 0 {
            return Err(std::io::ErrorKind::WriteZero.into());
        }
        buffer = &buffer[n..];
    }
    Ok(())
}

pub struct LinkUnicastTcp {
    // The underlying socket as returned from the tokio library
    socket: UnsafeCell<TcpSocket>,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    src_locator: Locator,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
    dst_locator: Locator,
    // The computed mtu
    mtu: BatchSize,
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
        #[allow(deprecated)]
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

        // Compute the MTU
        // See IETF RFC6691: https://datatracker.ietf.org/doc/rfc6691/
        let header = match src_addr.ip() {
            std::net::IpAddr::V4(_) => 40,
            std::net::IpAddr::V6(_) => 60,
        };
        #[allow(unused_mut)] // mut is not needed when target_family != unix
        let mut mtu = *TCP_DEFAULT_MTU - header;

        // target limitation of socket2: https://docs.rs/socket2/latest/src/socket2/sys/unix.rs.html#1544
        #[cfg(target_family = "unix")]
        {
            let socket = socket2::SockRef::from(&socket);
            // Get the MSS and divide it by 2 to ensure we can at least fill half the MSS
            let mss = socket.mss().unwrap_or(mtu as u32) / 2;
            // Compute largest multiple of TCP MSS that is smaller of default MTU
            let mut tgt = mss;
            while (tgt + mss) < mtu as u32 {
                tgt += mss;
            }
            mtu = (mtu as u32).min(tgt) as BatchSize;
        }

        // Build the Tcp object
        LinkUnicastTcp {
            socket: UnsafeCell::new(TcpSocket::Tokio(socket)),
            src_addr,
            src_locator: Locator::new(TCP_LOCATOR_PREFIX, src_addr.to_string(), "").unwrap(),
            dst_addr,
            dst_locator: Locator::new(TCP_LOCATOR_PREFIX, dst_addr.to_string(), "").unwrap(),
            mtu,
        }
    }

    #[allow(clippy::mut_from_ref)]
    fn get_mut_socket(&self) -> &mut TcpSocket {
        unsafe { &mut *self.socket.get() }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastTcp {
    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing TCP link: {}", self);
        // Close the underlying TCP socket
        let res = match self.get_mut_socket() {
            TcpSocket::Tokio(socket) => socket.shutdown().await,
            #[cfg(all(feature = "uring", target_os = "linux"))]
            TcpSocket::WriteOnly(fd) => fd.get_ref().shutdown(std::net::Shutdown::Write),
            #[cfg(all(feature = "uring", target_os = "linux"))]
            TcpSocket::Detached => Ok(()),
        };
        res.map_err(|e| {
            let e = zerror!("TCP link shutdown {}: {:?}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn write(&self, buffer: &[u8], _priority: Option<Priority>) -> ZResult<usize> {
        let res = match self.get_mut_socket() {
            TcpSocket::Tokio(socket) => socket.write(buffer).await,
            #[cfg(all(feature = "uring", target_os = "linux"))]
            TcpSocket::WriteOnly(fd) => write_nonblocking(fd, buffer).await,
            #[cfg(all(feature = "uring", target_os = "linux"))]
            TcpSocket::Detached => Err(detached_error()),
        };
        res.map_err(|e| {
            let e = zerror!("Write error on TCP link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn write_all(&self, buffer: &[u8], _priority: Option<Priority>) -> ZResult<()> {
        let res = match self.get_mut_socket() {
            TcpSocket::Tokio(socket) => socket.write_all(buffer).await,
            #[cfg(all(feature = "uring", target_os = "linux"))]
            TcpSocket::WriteOnly(fd) => write_all_nonblocking(fd, buffer).await,
            #[cfg(all(feature = "uring", target_os = "linux"))]
            TcpSocket::Detached => Err(detached_error()),
        };
        res.map_err(|e| {
            let e = zerror!("Write error on TCP link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn read(&self, buffer: &mut [u8], _priority: Option<Priority>) -> ZResult<usize> {
        let res = match self.get_mut_socket() {
            TcpSocket::Tokio(socket) => socket.read(buffer).await,
            #[cfg(all(feature = "uring", target_os = "linux"))]
            TcpSocket::WriteOnly(_) | TcpSocket::Detached => Err(detached_error()),
        };
        res.map_err(|e| {
            let e = zerror!("Read error on TCP link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn read_exact(&self, buffer: &mut [u8], _priority: Option<Priority>) -> ZResult<()> {
        let res = match self.get_mut_socket() {
            TcpSocket::Tokio(socket) => socket.read_exact(buffer).await.map(|_| ()),
            #[cfg(all(feature = "uring", target_os = "linux"))]
            TcpSocket::WriteOnly(_) | TcpSocket::Detached => Err(detached_error()),
        };
        res.map_err(|e| {
            let e = zerror!("Read error on TCP link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
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
    fn get_mtu(&self) -> BatchSize {
        self.mtu
    }

    #[inline(always)]
    fn get_interface_names(&self) -> Vec<String> {
        get_ip_interface_names(&self.src_addr)
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        super::IS_RELIABLE
    }

    #[inline(always)]
    fn is_streamed(&self) -> bool {
        true
    }

    #[inline(always)]
    fn get_auth_id(&self) -> &LinkAuthId {
        &LinkAuthId::Tcp
    }

    #[cfg(all(feature = "uring", target_os = "linux"))]
    fn get_fd(&self) -> ZResult<RawFd> {
        let fd = match unsafe { &*self.socket.get() } {
            TcpSocket::Tokio(socket) => socket.as_raw_fd(),
            TcpSocket::WriteOnly(fd) => fd.as_raw_fd(),
            TcpSocket::Detached => -1,
        };
        if fd < 0 {
            bail!("FD unavailable")
        }
        Ok(fd)
    }

    #[cfg(all(feature = "uring", target_os = "linux"))]
    unsafe fn detach_rx_from_reactor(&self) -> ZResult<()> {
        let slot = self.get_mut_socket();
        if !matches!(slot, TcpSocket::Tokio(_)) {
            return Ok(());
        }
        let TcpSocket::Tokio(stream) = std::mem::replace(slot, TcpSocket::Detached) else {
            unreachable!()
        };
        // `into_std` deregisters the fd from the tokio reactor that accepted or
        // connected it; re-register it for WRITABLE interest only, so incoming
        // data (now read by io_uring) no longer wakes that reactor.
        let stream = stream.into_std()?;
        let fd = AsyncFd::with_interest(stream, Interest::WRITABLE)?;
        *slot = TcpSocket::WriteOnly(fd);
        tracing::debug!("TCP link {}: RX detached from the tokio reactor", self);
        Ok(())
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
            .field("mtu", &self.get_mtu())
            .finish()
    }
}

pub struct LinkManagerUnicastTcp {
    manager: NewLinkChannelSender,
    listeners: ListenersUnicastIP,
}

impl fmt::Debug for LinkManagerUnicastTcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkManagerUnicastTcp")
            .field("manager", &self.manager)
            .field("listeners", &self.listeners)
            .finish()
    }
}

impl LinkManagerUnicastTcp {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: ListenersUnicastIP::new(),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastTcp {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let dst_addrs = get_tcp_addrs(endpoint.address()).await?;

        let config = endpoint.config();

        // if both `iface`, and `bind` are present, return error
        if let (Some(_), Some(_)) = (config.get(BIND_INTERFACE), config.get(BIND_SOCKET)) {
            bail!(
                "Using Config options `iface` and `bind` in conjunction is unsupported at this time {} {:?}",
                BIND_INTERFACE,
                BIND_SOCKET
            )
        }

        let socket_config = TcpSocketConfig::from(TcpLinkConfig::new(&config).await?);

        let mut errs: Vec<ZError> = vec![];
        for da in dst_addrs {
            match socket_config.new_link(&da).await {
                Ok((stream, src_addr, dst_addr)) => {
                    let link = Arc::new(LinkUnicastTcp::new(stream, src_addr, dst_addr));
                    return Ok(LinkUnicast::from(link as Arc<dyn LinkUnicastTrait>));
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

        let socket_config = TcpSocketConfig::from(TcpLinkConfig::new(&config).await?);

        let mut errs: Vec<ZError> = vec![];
        for da in addrs {
            match socket_config.new_listener(&da) {
                Ok((socket, local_addr)) => {
                    // Update the endpoint locator address
                    endpoint = EndPoint::new(
                        endpoint.protocol(),
                        format!("{local_addr}"),
                        endpoint.metadata(),
                        endpoint.config(),
                    )?;

                    let token = self.listeners.token.child_token();

                    let task = {
                        let token = token.clone();
                        let manager = self.manager.clone();

                        async move { accept_task(socket, token, manager).await }
                    };

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

    /// See [`zenoh_link_commons::ListenersUnicastIP::get_locators`].
    async fn get_locators(&self) -> Vec<Locator> {
        self.listeners.get_locators()
    }

    /// See [`zenoh_link_commons::ListenersUnicastIP::get_locators_noloopback`].
    async fn get_locators_noloopback(&self) -> Vec<Locator> {
        self.listeners.get_locators_noloopback()
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
                        // Get the right source address in case an unsepecified IP (i.e. 0.0.0.0 or [::]) is used
                        let src_addr =  match stream.local_addr()  {
                            Ok(sa) => sa,
                            Err(e) => {
                                tracing::debug!("Can not accept TCP connection: {}", e);
                                continue;
                            }
                        };

                        tracing::debug!("Accepted TCP connection on {:?}: {:?}", src_addr, dst_addr);
                        // Create the new link object
                        let link: Arc<dyn LinkUnicastTrait> = Arc::new(LinkUnicastTcp::new(stream, src_addr, dst_addr));

                        // Communicate the new link to the initial transport manager
                        if let Err(e) = manager.send_async(LinkUnicast::from(link)).await {
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

#[cfg(all(test, feature = "uring", target_os = "linux"))]
mod tests {
    use std::{
        future::Future,
        io::{Read, Write},
        task::Poll,
    };

    use super::*;

    #[test]
    fn detached_socket_preserves_writes_and_half_close() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let stream = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
                let (mut peer, _) = listener.accept().unwrap();
                peer.set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                stream.set_nonblocking(true).unwrap();
                socket2::SockRef::from(&stream)
                    .set_send_buffer_size(1024)
                    .unwrap();
                let stream = TcpStream::from_std(stream).unwrap();
                let link = LinkUnicastTcp::new(
                    stream,
                    peer.peer_addr().unwrap(),
                    peer.local_addr().unwrap(),
                );
                // No I/O futures or other users exist during the conversion.
                unsafe { link.detach_rx_from_reactor().unwrap() };

                let raw_fd = link.get_fd().unwrap();
                let mut registrations = 0;
                for entry in std::fs::read_dir("/proc/self/fdinfo").unwrap() {
                    let Ok(info) = std::fs::read_to_string(entry.unwrap().path()) else {
                        continue;
                    };
                    for line in info.lines() {
                        let fields: Vec<_> = line.split_whitespace().collect();
                        if fields.first() == Some(&"tfd:") && fields[1] == raw_fd.to_string() {
                            let events = u32::from_str_radix(fields[3], 16).unwrap();
                            assert_eq!(events & 1, 0, "socket still registered for EPOLLIN");
                            assert_ne!(events & 4, 0, "socket missing EPOLLOUT");
                            registrations += 1;
                        }
                    }
                }
                assert!(registrations > 0);

                // Drive initial readiness so Pending below comes from a blocked write.
                {
                    let TcpSocket::WriteOnly(fd) = (unsafe { &*link.socket.get() }) else {
                        panic!("socket was not converted")
                    };
                    let _ = tokio::time::timeout(Duration::from_secs(10), fd.writable())
                        .await
                        .unwrap()
                        .unwrap();
                }
                let data = vec![0x5a; 1024 * 1024];
                let mut write = Box::pin(link.write_all(&data, None));
                // A small send buffer and a peer that is not reading force backpressure.
                std::future::poll_fn(|cx| match write.as_mut().poll(cx) {
                    Poll::Pending => Poll::Ready(()),
                    Poll::Ready(result) => {
                        panic!("write did not encounter backpressure: {result:?}")
                    }
                })
                .await;
                let receiver = std::thread::spawn(move || {
                    let mut received = Vec::new();
                    peer.read_to_end(&mut received).unwrap();
                    assert_eq!(received, vec![0x5a; 1024 * 1024]);
                    peer.write_all(b"reply").unwrap();
                });
                tokio::time::timeout(Duration::from_secs(10), write)
                    .await
                    .unwrap()
                    .unwrap();
                link.close().await.unwrap();

                let TcpSocket::WriteOnly(fd) = (unsafe { &*link.socket.get() }) else {
                    panic!("socket was not converted")
                };
                let mut read_half = fd.get_ref().try_clone().unwrap();
                read_half.set_nonblocking(false).unwrap();
                read_half
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut reply = [0; 5];
                let result = read_half.read_exact(&mut reply);
                receiver.join().unwrap();
                result.unwrap();
                assert_eq!(&reply, b"reply");
            });
    }
}
