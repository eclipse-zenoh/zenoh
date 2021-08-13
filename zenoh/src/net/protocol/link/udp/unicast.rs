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
use super::session::SessionManager;
use super::*;
use async_std::net::{SocketAddr, UdpSocket};
use async_std::prelude::*;
use async_std::sync::Mutex as AsyncMutex;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;
use zenoh_util::collections::{RecyclingObject, RecyclingObjectPool};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::sync::{Mvar, Signal};
use zenoh_util::{zasynclock, zerror};

type LinkHashMap = Arc<Mutex<HashMap<(SocketAddr, SocketAddr), Weak<LinkUnicastUdpUnconnected>>>>;
type LinkInput = (RecyclingObject<Box<[u8]>>, usize);
type LinkLeftOver = (RecyclingObject<Box<[u8]>>, usize, usize);

struct LinkUnicastUdpConnected {
    socket: Arc<UdpSocket>,
}

impl LinkUnicastUdpConnected {
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        (&self.socket).recv(buffer).await.map_err(|e| {
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        (&self.socket).send(buffer).await.map_err(|e| {
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })
    }

    async fn close(&self) -> ZResult<()> {
        Ok(())
    }
}

struct LinkUnicastUdpUnconnected {
    socket: Weak<UdpSocket>,
    links: LinkHashMap,
    input: Mvar<LinkInput>,
    leftover: AsyncMutex<Option<LinkLeftOver>>,
}

impl LinkUnicastUdpUnconnected {
    async fn received(&self, buffer: RecyclingObject<Box<[u8]>>, len: usize) {
        self.input.put((buffer, len)).await;
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let mut guard = zasynclock!(self.leftover);
        let (slice, start, len) = match guard.take() {
            Some(tuple) => tuple,
            None => {
                let (slice, len) = self.input.take().await;
                (slice, 0, len)
            }
        };
        // Copy the read bytes into the target buffer
        let len_min = (len - start).min(buffer.len());
        let end = start + len_min;
        buffer[0..len_min].copy_from_slice(&slice[start..end]);
        if end < len {
            // Store the leftover
            *guard = Some((slice, end, len));
        } else {
            // Recycle the buffer
            slice.recycle().await;
        }
        // Return the amount read
        Ok(len_min)
    }

    async fn write(&self, buffer: &[u8], dst_addr: SocketAddr) -> ZResult<usize> {
        match self.socket.upgrade() {
            Some(socket) => socket.send_to(buffer, &dst_addr).await.map_err(|e| {
                zerror2!(ZErrorKind::IoError {
                    descr: e.to_string()
                })
            }),
            None => zerror!(ZErrorKind::IoError {
                descr: "UDP listener has been dropped".to_string()
            }),
        }
    }

    async fn close(&self, src_addr: SocketAddr, dst_addr: SocketAddr) -> ZResult<()> {
        // Delete the link from the list of links
        zlock!(self.links).remove(&(src_addr, dst_addr));
        Ok(())
    }
}

enum LinkUnicastUdpVariant {
    Connected(LinkUnicastUdpConnected),
    Unconnected(Arc<LinkUnicastUdpUnconnected>),
}

pub struct LinkUnicastUdp {
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
    // The UDP socket is connected to the peer
    variant: LinkUnicastUdpVariant,
}

impl LinkUnicastUdp {
    fn new(
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
        variant: LinkUnicastUdpVariant,
    ) -> LinkUnicastUdp {
        LinkUnicastUdp {
            src_addr,
            dst_addr,
            variant,
        }
    }
}

#[async_trait]
impl LinkTrait for LinkUnicastUdp {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing UDP link: {}", self);
        match &self.variant {
            LinkUnicastUdpVariant::Connected(link) => link.close().await,
            LinkUnicastUdpVariant::Unconnected(link) => {
                link.close(self.src_addr, self.dst_addr).await
            }
        }
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        match &self.variant {
            LinkUnicastUdpVariant::Connected(link) => link.write(buffer).await,
            LinkUnicastUdpVariant::Unconnected(link) => link.write(buffer, self.dst_addr).await,
        }
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut written: usize = 0;
        while written < buffer.len() {
            written += self.write(&buffer[written..]).await?;
        }
        Ok(())
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        match &self.variant {
            LinkUnicastUdpVariant::Connected(link) => link.read(buffer).await,
            LinkUnicastUdpVariant::Unconnected(link) => link.read(buffer).await,
        }
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let mut read: usize = 0;
        loop {
            let n = self.read(&mut buffer[read..]).await?;
            read += n;
            if read == buffer.len() {
                return Ok(());
            }
        }
    }

    #[inline(always)]
    fn get_src(&self) -> Locator {
        Locator::Udp(LocatorUdp::SocketAddr(self.src_addr))
    }

    #[inline(always)]
    fn get_dst(&self) -> Locator {
        Locator::Udp(LocatorUdp::SocketAddr(self.dst_addr))
    }

    #[inline(always)]
    fn get_mtu(&self) -> usize {
        *UDP_DEFAULT_MTU
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        false
    }

    #[inline(always)]
    fn is_streamed(&self) -> bool {
        false
    }
}

impl fmt::Display for LinkUnicastUdp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastUdp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Udp")
            .field("src", &self.src_addr)
            .field("dst", &self.dst_addr)
            .finish()
    }
}

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerUnicastUdp {
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastUdp {
    fn new(
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> ListenerUnicastUdp {
        ListenerUnicastUdp {
            active,
            signal,
            handle,
        }
    }
}

pub struct LinkManagerUnicastUdp {
    manager: SessionManager,
    listeners: Arc<RwLock<HashMap<SocketAddr, ListenerUnicastUdp>>>,
}

impl LinkManagerUnicastUdp {
    pub(crate) fn new(manager: SessionManager) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastUdp {
    async fn new_link(&self, dst: &Locator, _ps: Option<&LocatorProperty>) -> ZResult<Link> {
        let dst_addr = get_udp_addr(dst).await?;
        // Establish a UDP socket
        let socket = if dst_addr.is_ipv4() {
            // IPv4 format
            UdpSocket::bind("0.0.0.0:0").await
        } else {
            // IPv6 format
            UdpSocket::bind(":::0").await
        }
        .map_err(|e| {
            let e = format!("Can not create a new UDP link bound to {}: {}", dst_addr, e);
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Connect the socket to the remote address
        socket.connect(dst_addr).await.map_err(|e| {
            let e = format!("Can not create a new UDP link bound to {}: {}", dst_addr, e);
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Get source and destination UDP addresses
        let src_addr = socket.local_addr().map_err(|e| {
            let e = format!("Can not create a new UDP link bound to {}: {}", dst_addr, e);
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let dst_addr = socket.peer_addr().map_err(|e| {
            let e = format!("Can not create a new UDP link bound to {}: {}", dst_addr, e);
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Create UDP link
        let link = Arc::new(LinkUnicastUdp::new(
            src_addr,
            dst_addr,
            LinkUnicastUdpVariant::Connected(LinkUnicastUdpConnected {
                socket: Arc::new(socket),
            }),
        ));

        Ok(Link(link))
    }

    async fn new_listener(
        &self,
        locator: &Locator,
        _ps: Option<&LocatorProperty>,
    ) -> ZResult<Locator> {
        let addr = get_udp_addr(locator).await?;

        // Bind the UDP socket
        let socket = UdpSocket::bind(addr).await.map_err(|e| {
            let e = format!("Can not create a new UDP listener on {}: {}", addr, e);
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let local_addr = socket.local_addr().map_err(|e| {
            let e = format!("Can not create a new UDP listener on {}: {}", addr, e);
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Spawn the accept loop for the listener
        let active = Arc::new(AtomicBool::new(true));
        let signal = Signal::new();

        let c_active = active.clone();
        let c_signal = signal.clone();
        let c_manager = self.manager.clone();
        let c_listeners = self.listeners.clone();
        let c_addr = local_addr;
        let handle = task::spawn(async move {
            // Wait for the accept loop to terminate
            let res = accept_read_task(socket, c_active, c_signal, c_manager).await;
            zwrite!(c_listeners).remove(&c_addr);
            res
        });

        let listener = ListenerUnicastUdp::new(active, signal, handle);
        // Update the list of active listeners on the manager
        zwrite!(self.listeners).insert(local_addr, listener);

        Ok(Locator::Udp(LocatorUdp::SocketAddr(local_addr)))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_udp_addr(locator).await?;

        // Stop the listener
        let listener = zwrite!(self.listeners).remove(&addr).ok_or_else(|| {
            let e = format!(
                "Can not delete the UDP listener because it has not been found: {}",
                addr
            );
            log::trace!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Send the stop signal
        listener.active.store(false, Ordering::Release);
        listener.signal.trigger();
        listener.handle.await
    }

    fn get_listeners(&self) -> Vec<Locator> {
        zread!(self.listeners)
            .keys()
            .map(|x| Locator::Udp(LocatorUdp::SocketAddr(*x)))
            .collect()
    }

    fn get_locators(&self) -> Vec<Locator> {
        self.get_listeners()
    }
}

async fn accept_read_task(
    socket: UdpSocket,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: SessionManager,
) -> ZResult<()> {
    let socket = Arc::new(socket);
    let links: LinkHashMap = Arc::new(Mutex::new(HashMap::new()));

    macro_rules! zaddlink {
        ($src:expr, $dst:expr, $link:expr) => {
            zlock!(links).insert(($src, $dst), $link);
        };
    }

    macro_rules! zdellink {
        ($src:expr, $dst:expr) => {
            zlock!(links).remove(&($src, $dst));
        };
    }

    macro_rules! zgetlink {
        ($src:expr, $dst:expr) => {
            zlock!(links).get(&($src, $dst)).map(|link| link.clone())
        };
    }

    enum Action {
        Receive((usize, SocketAddr)),
        Stop,
    }

    async fn receive(socket: Arc<UdpSocket>, buffer: &mut [u8]) -> ZResult<Action> {
        let res = socket.recv_from(buffer).await.map_err(|e| {
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })?;
        Ok(Action::Receive(res))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let src_addr = socket.local_addr().map_err(|e| {
        let e = format!("Can not accept UDP connections: {}", e);
        log::warn!("{}", e);
        zerror2!(ZErrorKind::IoError { descr: e })
    })?;

    log::trace!("Ready to accept UDP connections on: {:?}", src_addr);
    // Buffers for deserialization
    let pool = RecyclingObjectPool::new(1, || vec![0u8; UDP_MAX_MTU].into_boxed_slice());
    while active.load(Ordering::Acquire) {
        let mut buff = pool.take().await;
        // Wait for incoming connections
        let (n, dst_addr) = match receive(socket.clone(), &mut buff)
            .race(stop(signal.clone()))
            .await
        {
            Ok(action) => match action {
                Action::Receive((n, addr)) => (n, addr),
                Action::Stop => break,
            },
            Err(e) => {
                log::warn!("{}. Hint: increase the system open file limit.", e);
                // Throttle the accept loop upon an error
                // NOTE: This might be due to various factors. However, the most common case is that
                //       the process has reached the maximum number of open files in the system. On
                //       Linux systems this limit can be changed by using the "ulimit" command line
                //       tool. In case of systemd-based systems, this can be changed by using the
                //       "sysctl" command line tool.
                task::sleep(Duration::from_micros(*UDP_ACCEPT_THROTTLE_TIME)).await;
                continue;
            }
        };

        let link = loop {
            let res = zgetlink!(src_addr, dst_addr);
            match res {
                Some(link) => break link.upgrade(),
                None => {
                    // A new peers has sent data to this socket
                    log::debug!("Accepted UDP connection on {}: {}", src_addr, dst_addr);
                    let unconnected = Arc::new(LinkUnicastUdpUnconnected {
                        socket: Arc::downgrade(&socket),
                        links: links.clone(),
                        input: Mvar::new(),
                        leftover: AsyncMutex::new(None),
                    });
                    zaddlink!(src_addr, dst_addr, Arc::downgrade(&unconnected));
                    // Create the new link object
                    let link = Arc::new(LinkUnicastUdp::new(
                        src_addr,
                        dst_addr,
                        LinkUnicastUdpVariant::Unconnected(unconnected),
                    ));
                    // Add the new link to the set of connected peers
                    manager.handle_new_link_unicast(Link(link), None).await;
                }
            }
        };

        match link {
            Some(link) => {
                link.received(buff, n).await;
            }
            None => {
                zdellink!(src_addr, dst_addr);
            }
        }
    }

    Ok(())
}
