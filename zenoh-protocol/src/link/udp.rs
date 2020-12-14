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
use async_std::channel::{bounded, Receiver, Sender};
use async_std::net::{SocketAddr, UdpSocket};
use async_std::prelude::*;
use async_std::sync::{Arc, Barrier, Mutex, RwLock, Weak};
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::{Link, LinkTrait, Locator, ManagerTrait};
use crate::io::{ArcSlice, RBuf};
use crate::session::{Action, SessionManagerInner, Transport};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasynclock, zasyncread, zasyncwrite, zerror};

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
//       IPv6 Jumbograms, it's usage in Zenoh is discouraged unless the consequences are very well
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
    // Size of buffer used to read from socket.
    static ref UDP_READ_BUFFER_SIZE: usize = UDP_MAX_MTU;
    // Size of the vector used to deserialize the messages.
    static ref UDP_READ_MESSAGES_VEC_SIZE: usize = 32;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref UDP_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[macro_export]
macro_rules! get_udp_addr {
    ($locator:expr) => {
        match $locator {
            Locator::Udp(addr) => addr,
            _ => {
                let e = format!("Not a UDP locator: {}", $locator);
                log::debug!("{}", e);
                return zerror!(ZErrorKind::InvalidLocator { descr: e });
            }
        }
    };
}

/*************************************/
/*              LINK                 */
/*************************************/
pub struct Udp {
    // The underlying socket as returned from the async-std library
    socket: Arc<UdpSocket>,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
    // The source Zenoh locator of this link (locator used on the local host)
    src_locator: Locator,
    // The destination Zenoh locator of this link (locator used on the local host)
    dst_locator: Locator,
    // The reference to the associated transport
    transport: Mutex<Transport>,
    // The reference to the associated link manager
    manager: Arc<ManagerUdpInner>,
    // Channel for stopping the read task
    signal: Mutex<Option<Sender<()>>>,
    // Weak reference to self
    w_self: RwLock<Option<Weak<Self>>>,
    // The UDP socket is connected to the peer
    is_connected: bool,
}

impl Udp {
    fn new(
        socket: Arc<UdpSocket>,
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
        transport: Transport,
        manager: Arc<ManagerUdpInner>,
        is_connected: bool,
    ) -> Udp {
        // Build the Udp object
        Udp {
            socket,
            src_addr,
            dst_addr,
            src_locator: Locator::Udp(src_addr),
            dst_locator: Locator::Udp(dst_addr),
            transport: Mutex::new(transport),
            manager,
            signal: Mutex::new(None),
            w_self: RwLock::new(None),
            is_connected,
        }
    }

    fn initizalize(&self, w_self: Weak<Self>) {
        *self.w_self.try_write().unwrap() = Some(w_self);
    }
}

#[async_trait]
impl LinkTrait for Udp {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing UDP link: {}", self);
        // Stop the read loop
        self.stop().await?;
        // Delete the link from the manager
        let _ = self.manager.del_link(&self.src_addr, &self.dst_addr).await;
        Ok(())
    }

    async fn send(&self, buffer: &[u8]) -> ZResult<()> {
        log::trace!("Sending {} bytes on UDP link: {}", buffer.len(), self);

        let res = if self.is_connected {
            (&self.socket).send(buffer).await
        } else {
            (&self.socket).send_to(buffer, self.dst_addr).await
        };
        if let Err(e) = res {
            log::trace!("Transmission error on UDP link {}: {}", self, e);
            return zerror!(ZErrorKind::IOError {
                descr: format!("{}", e)
            });
        }

        Ok(())
    }

    async fn start(&self) -> ZResult<()> {
        log::trace!("Starting read loop on UDP link: {}", self);
        if self.is_connected {
            let mut guard = zasynclock!(self.signal);
            if guard.is_none() {
                let link = if let Some(link) = zasyncread!(self.w_self).as_ref() {
                    if let Some(link) = link.upgrade() {
                        link
                    } else {
                        let e = format!("UDP link does not longer exist: {}", self);
                        log::error!("{}", e);
                        return zerror!(ZErrorKind::Other { descr: e });
                    }
                } else {
                    let e = format!("UDP link is unitialized: {}", self);
                    log::error!("{}", e);
                    return zerror!(ZErrorKind::Other { descr: e });
                };

                // The channel for stopping the read task
                let (sender, receiver) = bounded::<()>(1);
                // Store the sender
                *guard = Some(sender);

                // Spawn the read task
                task::spawn(read_task(link, receiver));
            }
        }

        Ok(())
    }

    async fn stop(&self) -> ZResult<()> {
        log::trace!("Stopping read loop on UDP link: {}", self);
        if self.is_connected {
            let mut guard = zasynclock!(self.signal);
            if let Some(signal) = guard.take() {
                let _ = signal.send(()).await;
            }
        }

        Ok(())
    }

    fn get_src(&self) -> Locator {
        self.src_locator.clone()
    }

    fn get_dst(&self) -> Locator {
        self.dst_locator.clone()
    }

    fn get_mtu(&self) -> usize {
        *UDP_DEFAULT_MTU
    }

    fn is_reliable(&self) -> bool {
        false
    }

    fn is_streamed(&self) -> bool {
        false
    }
}

async fn read_task(link: Arc<Udp>, stop: Receiver<()>) {
    // The accept future
    let read_loop = async {
        // The link object to be passed to the transport
        let lobj = Link::new(link.clone());
        // Acquire the lock on the transport
        let mut guard = zasynclock!(link.transport);

        // Buffers for deserialization
        let mut buff = vec![0; *UDP_READ_BUFFER_SIZE];
        let mut rbuf = RBuf::new();
        let mut msgs = Vec::with_capacity(*UDP_READ_MESSAGES_VEC_SIZE);

        // Macro to handle a link error
        macro_rules! zlinkerror {
            ($notify:expr) => {
                // Delete the link from the manager
                let _ = link.manager.del_link(&link.src_addr, &link.dst_addr).await;
                if $notify {
                    // Notify the transport
                    let _ = guard.link_err(&lobj).await;
                }
                // Exit
                return Ok(());
            };
        }

        loop {
            // Clear the rbuf
            rbuf.clear();
            // Wait for incoming connections
            match link.socket.recv(&mut buff).await {
                Ok(n) => {
                    if n == 0 {
                        // Reading 0 bytes means error
                        log::debug!("Zero bytes reading on UDP link: {}", link);
                        zlinkerror!(true);
                    }
                    // Add the received bytes to the RBuf for deserialization
                    let mut slice = Vec::with_capacity(n);
                    slice.extend_from_slice(&buff[..n]);
                    rbuf.add_slice(ArcSlice::new(Arc::new(slice), 0, n));

                    // Deserialize all the messages from the current RBuf
                    msgs.clear();
                    while rbuf.can_read() {
                        match rbuf.read_session_message() {
                            Some(msg) => msgs.push(msg),
                            None => {
                                zlinkerror!(true);
                            }
                        }
                    }

                    // Process all the messages
                    for msg in msgs.drain(..) {
                        let res = guard.receive_message(&lobj, msg).await;
                        // Enforce the action as instructed by the upper logic
                        match res {
                            Ok(action) => match action {
                                Action::Read => {}
                                Action::ChangeTransport(transport) => {
                                    log::trace!("Change transport on UDP link: {}", link);
                                    *guard = transport
                                }
                                Action::Close => {
                                    log::trace!("Closing UDP link: {}", link);
                                    zlinkerror!(false);
                                }
                            },
                            Err(e) => {
                                log::trace!("Closing UDP link {}: {}", link, e);
                                zlinkerror!(false);
                            }
                        }
                    }
                }
                Err(e) => {
                    log::debug!("Reading error on UDP link {}: {}", link, e);
                    zlinkerror!(true);
                }
            };
        }
    };

    let _ = read_loop.race(stop.recv()).await;
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
pub struct ManagerUdp(Arc<ManagerUdpInner>);

impl ManagerUdp {
    pub(crate) fn new(manager: Arc<SessionManagerInner>) -> Self {
        Self(Arc::new(ManagerUdpInner::new(manager)))
    }
}

#[async_trait]
impl ManagerTrait for ManagerUdp {
    async fn new_link(&self, dst: &Locator, transport: &Transport) -> ZResult<Link> {
        let dst = get_udp_addr!(dst);
        let link = self.0.new_link(&self.0, dst, transport).await?;
        let link = Link::new(link);
        Ok(link)
    }

    async fn get_link(&self, src: &Locator, dst: &Locator) -> ZResult<Link> {
        let src = get_udp_addr!(src);
        let dst = get_udp_addr!(dst);
        let link = self.0.get_link(src, dst).await?;
        let link = Link::new(link);
        Ok(link)
    }

    async fn del_link(&self, src: &Locator, dst: &Locator) -> ZResult<()> {
        let src = get_udp_addr!(src);
        let dst = get_udp_addr!(dst);
        let _ = self.0.del_link(src, dst).await?;
        Ok(())
    }

    async fn new_listener(&self, locator: &Locator) -> ZResult<Locator> {
        let addr = get_udp_addr!(locator);
        self.0.new_listener(&self.0, addr).await
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_udp_addr!(locator);
        self.0.del_listener(&self.0, addr).await
    }

    async fn get_listeners(&self) -> Vec<Locator> {
        self.0.get_listeners().await
    }

    async fn get_locators(&self) -> Vec<Locator> {
        self.0.get_locators().await
    }
}

struct ListenerUdpInner {
    socket: Arc<UdpSocket>,
    active: AtomicBool,
    sender: Sender<()>,
    receiver: Receiver<()>,
    barrier: Arc<Barrier>,
}

impl ListenerUdpInner {
    fn new(socket: Arc<UdpSocket>) -> Self {
        // Create the channel necessary to break the accept loop
        let (sender, receiver) = bounded::<()>(1);
        // Update the list of active listeners on the manager
        Self {
            socket,
            active: AtomicBool::new(true),
            sender,
            receiver,
            barrier: Arc::new(Barrier::new(2)),
        }
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    fn activate(&self) {
        self.active.store(true, Ordering::Release)
    }

    fn deactivate(&self) {
        self.active.store(false, Ordering::Release)
    }
}

type LinkKey = (SocketAddr, SocketAddr);
struct ManagerUdpInner {
    inner: Arc<SessionManagerInner>,
    listeners: RwLock<HashMap<SocketAddr, Arc<ListenerUdpInner>>>,
    links: RwLock<HashMap<LinkKey, (Arc<Udp>, Link)>>,
}

impl ManagerUdpInner {
    pub fn new(inner: Arc<SessionManagerInner>) -> Self {
        Self {
            inner,
            listeners: RwLock::new(HashMap::new()),
            links: RwLock::new(HashMap::new()),
        }
    }

    async fn new_link(
        &self,
        a_self: &Arc<Self>,
        dst_addr: &SocketAddr,
        transport: &Transport,
    ) -> ZResult<Arc<Udp>> {
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
        let link = Arc::new(Udp::new(
            Arc::new(socket),
            src_addr,
            dst_addr,
            transport.clone(),
            a_self.clone(),
            true,
        ));
        link.initizalize(Arc::downgrade(&link));
        let lobj = Link::new(link.clone());

        // Store the ink object
        let key = (src_addr, dst_addr);
        zasyncwrite!(self.links).insert(key, (link.clone(), lobj));

        // Spawn the receive loop for the new link
        let _ = link.start().await;

        log::trace!("Created UDP link: {}", link);

        Ok(link)
    }

    async fn del_link(&self, src_addr: &SocketAddr, dst_addr: &SocketAddr) -> ZResult<()> {
        // Remove the link from the manager list
        let mut guard_link = zasyncwrite!(self.links);
        match guard_link.remove(&(*src_addr, *dst_addr)) {
            Some(_) => {
                // Stop the accepter loop in case there are no links left
                // for a given listener and the listerner is no longer active
                let has_links = guard_link.keys().any(|&(s, _)| &s == src_addr);
                drop(guard_link);
                if !has_links {
                    let mut guard_list = zasyncwrite!(self.listeners);
                    if let Some(listener) = guard_list.remove(src_addr) {
                        if listener.is_active() {
                            // Re-insert the listener
                            guard_list.insert(*src_addr, listener);
                        } else {
                            // Send the stop signal
                            if listener.sender.send(()).await.is_ok() {
                                // Wait for the accept loop to be stopped
                                listener.barrier.wait().await;
                            }
                        }
                    };
                }
                Ok(())
            }
            None => {
                let e = format!(
                    "Can not delete UDP link because it has not been found: {} => {}",
                    src_addr, dst_addr
                );
                log::trace!("{}", e);
                zerror!(ZErrorKind::InvalidLink { descr: e })
            }
        }
    }

    async fn get_link(&self, src_addr: &SocketAddr, dst_addr: &SocketAddr) -> ZResult<Arc<Udp>> {
        // Remove the link from the manager list
        match zasyncread!(self.links).get(&(*src_addr, *dst_addr)) {
            Some((link, _)) => Ok(link.clone()),
            None => {
                let e = format!(
                    "Can not get UDP link because it has not been found: {} => {}",
                    src_addr, dst_addr
                );
                log::trace!("{}", e);
                zerror!(ZErrorKind::InvalidLink { descr: e })
            }
        }
    }

    async fn new_listener(&self, a_self: &Arc<Self>, addr: &SocketAddr) -> ZResult<Locator> {
        if let Some(listener) = zasyncread!(self.listeners).get(addr) {
            listener.activate();
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
        let listener = Arc::new(ListenerUdpInner::new(socket));
        // Update the list of active listeners on the manager
        zasyncwrite!(self.listeners).insert(local_addr, listener.clone());

        // Spawn the accept loop for the listener
        let c_self = a_self.clone();
        let c_addr = local_addr;
        task::spawn(async move {
            // Wait for the accept loop to terminate
            accept_read_task(&c_self, listener).await;
            // Delete the listener from the manager
            zasyncwrite!(c_self.listeners).remove(&c_addr);
        });
        Ok(Locator::Udp(local_addr))
    }

    async fn del_listener(&self, _a_self: &Arc<Self>, addr: &SocketAddr) -> ZResult<()> {
        // Stop the listener
        let mut guard = zasyncwrite!(self.listeners);
        match guard.remove(&addr) {
            Some(listener) => {
                // Deactivate the listener to not accept any new incoming connections
                listener.deactivate();
                // Check if there are no open connections
                let has_links = zasyncread!(self.links)
                    .keys()
                    .any(|&(src_addr, _)| &src_addr == addr);
                if has_links {
                    // We can not remove yet the listener from the hashmap: reinsert it
                    guard.insert(*addr, listener);
                } else {
                    // Send the stop signal
                    if listener.sender.send(()).await.is_ok() {
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
        zasyncread!(self.listeners)
            .keys()
            .map(|x| Locator::Udp(*x))
            .collect()
    }

    #[inline]
    async fn get_locators(&self) -> Vec<Locator> {
        self.get_listeners().await
    }
}

async fn accept_read_task(a_self: &Arc<ManagerUdpInner>, listener: Arc<ListenerUdpInner>) {
    // The accept/read future
    let accept_read_loop = async {
        macro_rules! zreceive {
            ($link:expr, $lobj:expr, $msgs:expr) => {
                let mut guard = zasynclock!($link.transport);
                for msg in $msgs.drain(..) {
                    log::trace!("Received UDP message: {:?}", msg);
                    let res = guard.receive_message(&$lobj, msg).await;
                    // Enforce the action as instructed by the upper logic
                    match res {
                        Ok(action) => match action {
                            Action::Read => {}
                            Action::ChangeTransport(transport) => {
                                log::trace!("Change transport on UDP link: {}", $link);
                                *guard = transport
                            }
                            Action::Close => {
                                let _ = $link.close().await;
                                continue;
                            }
                        },
                        Err(_) => {
                            let _ = $link.close().await;
                            continue;
                        }
                    }
                }
            };
        }

        macro_rules! zaddlinktuple {
            ($src:expr, $dst:expr, $link:expr, $lobj:expr) => {
                zasyncwrite!(a_self.links).insert(($src, $dst), ($link.clone(), $lobj.clone()));
            };
        }

        macro_rules! zgetlinktuple {
            ($src:expr, $dst:expr) => {
                match zasyncread!(a_self.links).get(&($src, $dst)) {
                    Some((link, lobj)) => Some((link.clone(), lobj.clone())),
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
        let mut buff = vec![0; *UDP_READ_BUFFER_SIZE];
        let mut rbuf = RBuf::new();
        let mut msgs = Vec::with_capacity(*UDP_READ_MESSAGES_VEC_SIZE);
        loop {
            // Clear the rbuf
            rbuf.clear();
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

            // Add the received bytes to the RBuf for deserialization
            let mut slice = Vec::with_capacity(n);
            slice.extend_from_slice(&buff[..n]);
            rbuf.add_slice(ArcSlice::new(Arc::new(slice), 0, n));

            log::trace!("UDP received: {} {}", n, rbuf);
            // Deserialize all the messages from the current RBuf
            msgs.clear();
            let mut error = false;
            while rbuf.can_read() {
                let res = rbuf.read_session_message();
                log::trace!("UDP deserialization from {}: {:?}", dst_addr, res);
                match res {
                    Some(msg) => msgs.push(msg),
                    None => {
                        log::warn!("Closing UDP link: {}", dst_addr);
                        error = true;
                    }
                }
            }

            let key = (src_addr, dst_addr);
            if error {
                let mut guard = zasyncwrite!(a_self.links);
                // Remove the link from the set of available peers
                if let Some((link, lobj)) = guard.remove(&key) {
                    zreceive!(link, lobj, msgs);
                }
                // Continue reading
                continue;
            }

            let res = zgetlinktuple!(src_addr, dst_addr);
            match res {
                Some((link, lobj)) => {
                    // Process all the messages
                    zreceive!(link, lobj, msgs);
                }
                None => {
                    // Create a new link if the listener is active
                    if listener.is_active() {
                        // A new peers has sent data to this socket
                        log::debug!("Accepted UDP connection on {}: {}", src_addr, dst_addr);
                        // Retrieve the initial temporary session
                        let initial = a_self.inner.get_initial_transport().await;
                        // Create the new link object
                        let link = Arc::new(Udp::new(
                            listener.socket.clone(),
                            src_addr,
                            dst_addr,
                            initial,
                            a_self.clone(),
                            false,
                        ));
                        link.initizalize(Arc::downgrade(&link));
                        let lobj = Link::new(link.clone());
                        // Add the new link to the set of connected peers
                        zaddlinktuple!(src_addr, dst_addr, link, lobj);
                        // Process all the messages
                        zreceive!(link, lobj, msgs);
                    } else {
                        log::warn!(
                            "Rejected UDP connection from {}: listerner {} is not active",
                            src_addr,
                            dst_addr
                        );
                    }
                }
            }
        }
    };

    let stop = listener.receiver.recv();
    let _ = accept_read_loop.race(stop).await;
    listener.barrier.wait().await;
}
