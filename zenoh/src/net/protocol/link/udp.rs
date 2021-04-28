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
use super::{Link, LinkManagerTrait, Locator, LocatorProperty};
use async_std::channel::{bounded, Receiver, Sender};
use async_std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use async_std::prelude::*;
use async_std::sync::{Arc, Barrier, Mutex, Weak};
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;
use zenoh_util::collections::{RecyclingObject, RecyclingObjectPool};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::sync::Mvar;
use zenoh_util::{zasynclock, zerror};

// NOTE: In case of using UDP in high-throughput scenarios, it is recommended to set the
//       UDP buffer size on the host to a reasonable size. Usually, default values for UDP buffers
//       size are undersized. Setting UDP buffers on the host to a size of 4M can be considered
//       as a safe choice.
//       Usually, on Linux systems this could be achieved by executing:
//           $ sysctl -w net.core.rmem_max=4194304
//           $ sysctl -w net.core.rmem_default=4194304

// Maximum MTU (UDP PDU) in bytes.
// NOTE: The UDP field size sets a theoretical limit of 65,535 bytes (8 byte header + 65,527 bytes of
//       data) for a UDP datagram. However the actual limit for the data length, which is imposed by
//       the underlying IPv4 protocol, is 65,507 bytes (65,535 − 8 byte UDP header − 20 byte IP header).
//       Although in IPv6 it is possible to have UDP datagrams of size greater than 65,535 bytes via
//       IPv6 Jumbograms, its usage in Zenoh is discouraged unless the consequences are very well
//       understood.
const UDP_MAX_MTU: usize = 65_507;

#[cfg(any(target_os = "linux", target_os = "windows"))]
// Linux default value of a maximum datagram size is set to UDP MAX MTU.
const UDP_MTU_LIMIT: usize = UDP_MAX_MTU;

#[cfg(target_os = "macos")]
// Mac OS X default value of a maximum datagram size is set to 9216 bytes.
const UDP_MTU_LIMIT: usize = 9_216;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const UDP_MTU_LIMIT: usize = 8_192;

zconfigurable! {
    // Default MTU (UDP PDU) in bytes.
    static ref UDP_DEFAULT_MTU: usize = UDP_MTU_LIMIT;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref UDP_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[allow(unreachable_patterns)]
async fn get_udp_addr(locator: &Locator) -> ZResult<SocketAddr> {
    match locator {
        Locator::Udp(addr) => match addr {
            LocatorUdp::SocketAddr(addr) => Ok(*addr),
            LocatorUdp::DnsName(addr) => match addr.to_socket_addrs().await {
                Ok(mut addr_iter) => {
                    if let Some(addr) = addr_iter.next() {
                        Ok(addr)
                    } else {
                        let e = format!("Couldn't resolve TCP locator: {}", addr);
                        zerror!(ZErrorKind::InvalidLocator { descr: e })
                    }
                }
                Err(e) => {
                    let e = format!("{}: {}", e, addr);
                    zerror!(ZErrorKind::InvalidLocator { descr: e })
                }
            },
        },
        _ => {
            let e = format!("Not a TCP locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

/*************************************/
/*             LOCATOR               */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocatorUdp {
    SocketAddr(SocketAddr),
    DnsName(String),
}

impl FromStr for LocatorUdp {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(addr) => Ok(LocatorUdp::SocketAddr(addr)),
            Err(_) => Ok(LocatorUdp::DnsName(s.to_string())),
        }
    }
}

impl fmt::Display for LocatorUdp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocatorUdp::SocketAddr(addr) => write!(f, "{}", addr)?,
            LocatorUdp::DnsName(addr) => write!(f, "{}", addr)?,
        }
        Ok(())
    }
}

/*************************************/
/*            PROPERTY               */
/*************************************/
pub type LocatorPropertyUdp = ();

/*************************************/
/*              LINK                 */
/*************************************/
type LinkHashMap = Arc<Mutex<HashMap<(SocketAddr, SocketAddr), Weak<LinkUdpUnconnected>>>>;
type LinkInput = (RecyclingObject<Box<[u8]>>, usize);
type LinkLeftOver = (RecyclingObject<Box<[u8]>>, usize, usize);

struct LinkUdpConnected {
    socket: Arc<UdpSocket>,
}

impl LinkUdpConnected {
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

struct LinkUdpUnconnected {
    socket: Weak<UdpSocket>,
    links: LinkHashMap,
    input: Mvar<LinkInput>,
    leftover: Mutex<Option<LinkLeftOver>>,
}

impl LinkUdpUnconnected {
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
        let mut guard = zasynclock!(self.links);
        guard.remove(&(src_addr, dst_addr));
        Ok(())
    }
}

enum LinkUdpVariant {
    Connected(LinkUdpConnected),
    Unconnected(Arc<LinkUdpUnconnected>),
}

pub struct LinkUdp {
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
    // The UDP socket is connected to the peer
    variant: LinkUdpVariant,
}

impl LinkUdp {
    fn new(src_addr: SocketAddr, dst_addr: SocketAddr, variant: LinkUdpVariant) -> LinkUdp {
        LinkUdp {
            src_addr,
            dst_addr,
            variant,
        }
    }

    pub(crate) async fn close(&self) -> ZResult<()> {
        log::trace!("Closing UDP link: {}", self);
        match &self.variant {
            LinkUdpVariant::Connected(link) => link.close().await,
            LinkUdpVariant::Unconnected(link) => link.close(self.src_addr, self.dst_addr).await,
        }
    }

    pub(crate) async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        match &self.variant {
            LinkUdpVariant::Connected(link) => link.write(buffer).await,
            LinkUdpVariant::Unconnected(link) => link.write(buffer, self.dst_addr).await,
        }
    }

    pub(crate) async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut written: usize = 0;
        while written < buffer.len() {
            written += self.write(&buffer[written..]).await?;
        }
        Ok(())
    }

    pub(crate) async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        match &self.variant {
            LinkUdpVariant::Connected(link) => link.read(buffer).await,
            LinkUdpVariant::Unconnected(link) => link.read(buffer).await,
        }
    }

    pub(crate) async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
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
    pub(crate) fn get_src(&self) -> Locator {
        Locator::Udp(LocatorUdp::SocketAddr(self.src_addr))
    }

    #[inline(always)]
    pub(crate) fn get_dst(&self) -> Locator {
        Locator::Udp(LocatorUdp::SocketAddr(self.dst_addr))
    }

    #[inline(always)]
    pub(crate) fn get_mtu(&self) -> usize {
        *UDP_DEFAULT_MTU
    }

    #[inline(always)]
    pub(crate) fn is_reliable(&self) -> bool {
        false
    }

    #[inline(always)]
    pub(crate) fn is_streamed(&self) -> bool {
        false
    }
}

impl fmt::Display for LinkUdp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUdp {
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
pub struct LinkManagerUdp {
    manager: SessionManager,
    listeners: Arc<Mutex<HashMap<SocketAddr, Arc<ListenerUdp>>>>,
}

impl LinkManagerUdp {
    pub(crate) fn new(manager: SessionManager) -> Self {
        Self {
            manager,
            listeners: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerTrait for LinkManagerUdp {
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
        let link = Arc::new(LinkUdp::new(
            src_addr,
            dst_addr,
            LinkUdpVariant::Connected(LinkUdpConnected {
                socket: Arc::new(socket),
            }),
        ));
        let lobj = Link::Udp(link);

        Ok(lobj)
    }

    async fn new_listener(
        &self,
        locator: &Locator,
        _ps: Option<&LocatorProperty>,
    ) -> ZResult<Locator> {
        let addr = get_udp_addr(locator).await?;

        if let Some(listener) = zasynclock!(self.listeners).get(&addr) {
            match listener.socket.local_addr() {
                Ok(addr) => return Ok(Locator::Udp(LocatorUdp::SocketAddr(addr))),
                Err(e) => {
                    let e = format!("Can not create a new UDP listener on {}: {}", addr, e);
                    log::warn!("{}", e);
                    return zerror!(ZErrorKind::InvalidLink { descr: e });
                }
            };
        }

        // Bind the UDP socket
        let socket = match UdpSocket::bind(addr).await {
            Ok(socket) => Arc::new(socket),
            Err(e) => {
                let e = format!("Can not create a new UDP listener on {}: {}", addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        let local_addr = socket.local_addr().map_err(|e| {
            let e = format!("Can not create a new UDP listener on {}: {}", addr, e);
            log::warn!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;
        let listener = Arc::new(ListenerUdp::new(socket));
        // Update the list of active listeners on the manager
        zasynclock!(self.listeners).insert(local_addr, listener.clone());

        // Spawn the accept loop for the listener
        let c_listeners = self.listeners.clone();
        let c_addr = local_addr;
        let c_manager = self.manager.clone();
        task::spawn(async move {
            // Wait for the accept loop to terminate
            accept_read_task(listener, c_manager).await;
            // Delete the listener from the manager
            zasynclock!(c_listeners).remove(&c_addr);
        });
        Ok(Locator::Udp(LocatorUdp::SocketAddr(local_addr)))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_udp_addr(locator).await?;
        // Stop the listener
        let mut guard = zasynclock!(self.listeners);
        match guard.remove(&addr) {
            Some(listener) => {
                let res = listener.sender.send(()).await;
                zasynclock!(listener.links).clear();
                if res.is_ok() {
                    // Wait for the accept loop to be stopped
                    listener.barrier.wait().await;
                }
                Ok(())
            }
            None => {
                let e = format!(
                    "Can not delete the UDP listener because it has not been found: {}",
                    addr
                );
                log::trace!("{}", e);
                zerror!(ZErrorKind::InvalidLink { descr: e })
            }
        }
    }

    async fn get_listeners(&self) -> Vec<Locator> {
        zasynclock!(self.listeners)
            .keys()
            .map(|x| Locator::Udp(LocatorUdp::SocketAddr(*x)))
            .collect()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        self.get_listeners().await
    }
}

struct ListenerUdp {
    socket: Arc<UdpSocket>,
    links: LinkHashMap,
    sender: Sender<()>,
    receiver: Receiver<()>,
    barrier: Arc<Barrier>,
}

impl ListenerUdp {
    fn new(socket: Arc<UdpSocket>) -> Self {
        // Create the channel necessary to break the accept loop
        let (sender, receiver) = bounded::<()>(1);
        // Update the list of active listeners on the manager
        Self {
            socket,
            links: Arc::new(Mutex::new(HashMap::new())),
            sender,
            receiver,
            barrier: Arc::new(Barrier::new(2)),
        }
    }
}

async fn accept_read_task(listener: Arc<ListenerUdp>, manager: SessionManager) {
    // The accept/read future
    let accept_read_loop = async {
        macro_rules! zaddlink {
            ($src:expr, $dst:expr, $link:expr) => {
                zasynclock!(listener.links).insert(($src, $dst), $link);
            };
        }

        macro_rules! zdellink {
            ($src:expr, $dst:expr) => {
                zasynclock!(listener.links).remove(&($src, $dst));
            };
        }

        macro_rules! zgetlink {
            ($src:expr, $dst:expr) => {
                zasynclock!(listener.links)
                    .get(&($src, $dst))
                    .map(|link| link.clone())
            };
        }

        let src_addr = match listener.socket.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not accept UDP connections: {}", e);
                log::warn!("{}", e);
                return Ok(());
            }
        };

        log::trace!("Ready to accept UDP connections on: {:?}", src_addr);
        // Buffers for deserialization
        let pool = RecyclingObjectPool::new(1, || vec![0u8; UDP_MAX_MTU].into_boxed_slice());
        loop {
            let mut buff = pool.take().await;
            // Wait for incoming connections
            let (n, dst_addr) = match listener.socket.recv_from(&mut buff).await {
                Ok((n, dst_addr)) => (n, dst_addr),
                Err(e) => {
                    log::warn!(
                        "{}. Hint: you might want to increase the system open file limit",
                        e
                    );
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
                        let unconnected = Arc::new(LinkUdpUnconnected {
                            socket: Arc::downgrade(&listener.socket),
                            links: listener.links.clone(),
                            input: Mvar::new(),
                            leftover: Mutex::new(None),
                        });
                        zaddlink!(src_addr, dst_addr, Arc::downgrade(&unconnected));
                        // Create the new link object
                        let link = Arc::new(LinkUdp::new(
                            src_addr,
                            dst_addr,
                            LinkUdpVariant::Unconnected(unconnected),
                        ));
                        // Add the new link to the set of connected peers
                        manager.handle_new_link(Link::Udp(link), None).await;
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
    };

    let stop = listener.receiver.recv();
    let _ = accept_read_loop.race(stop).await;
    listener.barrier.wait().await;
}
