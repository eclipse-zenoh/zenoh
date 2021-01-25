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
use super::{Link, LinkManagerTrait, LinkTrait, Locator};
use crate::session::SessionManager;
use async_std::channel::{bounded, Receiver, Sender};
use async_std::net::{SocketAddr, UdpSocket};
use async_std::prelude::*;
use async_std::sync::{Arc, Barrier, Mutex, Weak};
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
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
fn get_udp_addr(locator: &Locator) -> ZResult<&SocketAddr> {
    match locator {
        Locator::Udp(addr) => Ok(addr),
        _ => {
            let e = format!("Not a UDP locator: {}", locator);
            log::debug!("{}", e);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

/*************************************/
/*              LINK                 */
/*************************************/
type LinkHashMap = Arc<Mutex<HashMap<(SocketAddr, SocketAddr), Weak<Udp>>>>;

struct UnconnectedUdp {
    links: LinkHashMap,
    status: ListenerUdpStatus,
    signal: Sender<()>,
    input: Mvar<(Vec<u8>, usize, usize)>,
    output: Mvar<(Vec<u8>, usize)>,
}

pub struct Udp {
    // The underlying socket as returned from the async-std library
    socket: Arc<UdpSocket>,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
    // The UDP socket is connected to the peer
    unconnected: Option<UnconnectedUdp>,
}

impl Udp {
    fn new(
        socket: Arc<UdpSocket>,
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
        unconnected: Option<UnconnectedUdp>,
    ) -> Udp {
        Udp {
            socket,
            src_addr,
            dst_addr,
            unconnected,
        }
    }

    async fn received(&self, mut buffer: Vec<u8>, len: usize) -> Vec<u8> {
        let unconnected = self.unconnected.as_ref().unwrap();
        // Make sure that all bytes are read
        let mut start: usize = 0;
        while start < len {
            unconnected.input.put((buffer, start, len)).await;
            let tuple = unconnected.output.take().await;
            buffer = tuple.0;
            start += tuple.1;
        }
        buffer
    }

    async fn read_unconnected(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let unconnected = match self.unconnected.as_ref() {
            Some(unconnected) => unconnected,
            None => {
                return zerror!(ZErrorKind::IOError {
                    descr: "Send error".to_string()
                });
            }
        };
        // Get the buffer used in the main loop
        let (slice, start, len) = unconnected.input.take().await;
        // Copy the read bytes into the target buffer
        let len = len.min(buffer.len());
        buffer[0..len].copy_from_slice(&slice[start..start + len]);
        // Return back the ownership of the buffer used to read from the socket
        unconnected.output.put((slice, len)).await;
        // Return the amount read
        Ok(len)
    }

    async fn read_connected(&self, buffer: &mut [u8]) -> ZResult<usize> {
        match (&self.socket).recv(buffer).await {
            Ok(n) => Ok(n),
            Err(e) => {
                log::trace!("Reception error on UDP link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }
}

#[async_trait]
impl LinkTrait for Udp {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing UDP link: {}", self);
        if let Some(unconnected) = self.unconnected.as_ref() {
            // Bring back the buffer if present in the input
            if let Some(tuple) = unconnected.input.try_take().await {
                let (slice, _start, len) = tuple;
                unconnected.output.put((slice, len)).await;
            } else if unconnected.output.has_take_waiting() {
                let slice = vec![0u8; UDP_MAX_MTU];
                unconnected.output.put((slice, UDP_MAX_MTU)).await;
            }
            // Delete the link from the list of links
            let mut guard = zasynclock!(unconnected.links);
            guard.remove(&(self.src_addr, self.dst_addr));
            if !unconnected.status.is_active() && guard.is_empty() {
                let _ = unconnected.signal.send(()).await;
            }
        }
        Ok(())
    }

    #[inline]
    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        let res = if self.unconnected.is_some() {
            (&self.socket).send_to(buffer, &self.dst_addr).await
        } else {
            (&self.socket).send(buffer).await
        };
        match res {
            Ok(n) => Ok(n),
            Err(e) => {
                log::trace!("Transmission error on UDP link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut written: usize = 0;
        while written < buffer.len() {
            written += self.write(&buffer[written..]).await?;
        }
        Ok(())
    }

    #[inline]
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        if self.unconnected.is_some() {
            self.read_unconnected(buffer).await
        } else {
            self.read_connected(buffer).await
        }
    }

    #[inline]
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

    #[inline]
    fn get_src(&self) -> Locator {
        Locator::Udp(self.src_addr)
    }

    #[inline]
    fn get_dst(&self) -> Locator {
        Locator::Udp(self.dst_addr)
    }

    #[inline]
    fn get_mtu(&self) -> usize {
        *UDP_DEFAULT_MTU
    }

    #[inline]
    fn is_reliable(&self) -> bool {
        false
    }

    #[inline]
    fn is_streamed(&self) -> bool {
        false
    }
}

impl fmt::Display for Udp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for Udp {
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
    async fn new_link(&self, dst: &Locator) -> ZResult<Link> {
        let dst_addr = get_udp_addr(dst)?;
        // Establish a UDP socket
        let res = if dst_addr.is_ipv4() {
            // IPv4 format
            UdpSocket::bind("0.0.0.0:0").await
        } else {
            // IPv6 format
            UdpSocket::bind(":::0").await
        };

        // Get the socket
        let socket = match res {
            Ok(socket) => socket,
            Err(e) => {
                let e = format!("Can not create a new UDP link bound to {}: {}", dst_addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        // Connect the socket to the remote address
        let res = socket.connect(dst_addr).await;
        if let Err(e) = res {
            let e = format!("Can not create a new UDP link bound to {}: {}", dst_addr, e);
            log::warn!("{}", e);
            return zerror!(ZErrorKind::InvalidLink { descr: e });
        };

        // Get source and destination UDP addresses
        let src_addr = match socket.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not create a new UDP link bound to {}: {}", dst_addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        let dst_addr = match socket.peer_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not create a new UDP link bound to {}: {}", dst_addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        // Create UDP link
        let link = Arc::new(Udp::new(Arc::new(socket), src_addr, dst_addr, None));
        let lobj = Link::new(link);

        Ok(lobj)
    }

    async fn new_listener(&self, locator: &Locator) -> ZResult<Locator> {
        let addr = get_udp_addr(locator)?;

        if let Some(listener) = zasynclock!(self.listeners).get(addr) {
            listener.status.activate();
            let local_addr = match listener.socket.local_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    let e = format!("Can not create a new UDP listener on {}: {}", addr, e);
                    log::warn!("{}", e);
                    return zerror!(ZErrorKind::InvalidLink { descr: e });
                }
            };
            return Ok(Locator::Udp(local_addr));
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

        let local_addr = match socket.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not create a new UDP listener on {}: {}", addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };
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
        Ok(Locator::Udp(local_addr))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_udp_addr(locator)?;
        // Stop the listener
        let mut guard = zasynclock!(self.listeners);
        match guard.remove(&addr) {
            Some(listener) => {
                // Deactivate the listener to not accept any new incoming connections
                listener.status.deactivate();

                let guard = zasynclock!(listener.links);
                if guard.is_empty() {
                    let res = listener.sender.send(()).await;
                    if res.is_ok() {
                        // Wait for the accept loop to be stopped
                        listener.barrier.wait().await;
                    }
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
            .map(|x| Locator::Udp(*x))
            .collect()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        self.get_listeners().await
    }
}

#[derive(Clone)]
struct ListenerUdpStatus(Arc<AtomicBool>);

impl ListenerUdpStatus {
    fn new(value: bool) -> ListenerUdpStatus {
        Self(Arc::new(AtomicBool::new(value)))
    }

    fn is_active(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn activate(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    fn deactivate(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct ListenerUdp {
    socket: Arc<UdpSocket>,
    links: LinkHashMap,
    status: ListenerUdpStatus,
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
            status: ListenerUdpStatus::new(true),
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
                match zasynclock!(listener.links).get(&($src, $dst)) {
                    Some(link) => Some(link.clone()),
                    None => None,
                }
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
        let mut buff = vec![0u8; UDP_MAX_MTU];
        loop {
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
                        // Create a new link if the listener is active
                        if listener.status.is_active() {
                            // A new peers has sent data to this socket
                            log::debug!("Accepted UDP connection on {}: {}", src_addr, dst_addr);
                            let unconnected = UnconnectedUdp {
                                links: listener.links.clone(),
                                status: listener.status.clone(),
                                signal: listener.sender.clone(),
                                input: Mvar::new(),
                                output: Mvar::new(),
                            };
                            // Create the new link object
                            let link = Arc::new(Udp::new(
                                listener.socket.clone(),
                                src_addr,
                                dst_addr,
                                Some(unconnected),
                            ));
                            // Add the new link to the set of connected peers
                            zaddlink!(src_addr, dst_addr, Arc::downgrade(&link));

                            manager.handle_new_link(Link::new(link)).await;
                        } else {
                            log::debug!(
                                "Rejected UDP connection from {}: listerner {} is not active",
                                src_addr,
                                dst_addr
                            );
                            break None;
                        }
                    }
                }
            };

            if let Some(link) = link {
                buff = link.received(buff, n).await;
            } else {
                zdellink!(src_addr, dst_addr);
            }
        }
    };

    let stop = listener.receiver.recv();
    let _ = accept_read_loop.race(stop).await;
    listener.barrier.wait().await;
}
