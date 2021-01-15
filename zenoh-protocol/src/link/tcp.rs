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
use async_std::net::{SocketAddr, TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::sync::{Arc, Barrier, RwLock};
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::net::Shutdown;
use std::time::Duration;

use super::{Link, LinkManagerTrait, LinkTrait, Locator};
use crate::session::SessionManager;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasyncread, zasyncwrite, zerror, zerror2};

// Default MTU (TCP PDU) in bytes.
// NOTE: Since TCP is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the TCP MTU is constrained to
//       2^16 + 1 bytes (i.e., 65537).
const TCP_MAX_MTU: usize = 65_537;

zconfigurable! {
    // Default MTU (TCP PDU) in bytes.
    static ref TCP_DEFAULT_MTU: usize = TCP_MAX_MTU;
    // The LINGER option causes the shutdown() call to block until (1) all application data is delivered
    // to the remote end or (2) a timeout expires. The timeout is expressed in seconds.
    // More info on the LINGER option and its dynamics can be found at:
    // https://blog.netherlabs.nl/articles/2009/01/18/the-ultimate-so_linger-page-or-why-is-my-tcp-not-reliable
    static ref TCP_LINGER_TIMEOUT: i32 = 10;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref TCP_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[allow(unreachable_patterns)]
fn get_tcp_addr(locator: &Locator) -> ZResult<&SocketAddr> {
    match locator {
        Locator::Tcp(addr) => Ok(addr),
        _ => {
            let e = format!("Not a TCP locator: {}", locator);
            log::debug!("{}", e);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

/*************************************/
/*              LINK                 */
/*************************************/
pub struct Tcp {
    // The underlying socket as returned from the async-std library
    socket: TcpStream,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
}

impl Tcp {
    fn new(socket: TcpStream, src_addr: SocketAddr, dst_addr: SocketAddr) -> Tcp {
        // Set the TCP nodelay option
        if let Err(err) = socket.set_nodelay(true) {
            log::warn!(
                "Unable to set NODEALY option on TCP link {} => {} : {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Set the TCP linger option
        if let Err(err) = zenoh_util::net::set_linger(
            &socket,
            Some(Duration::from_secs(
                (*TCP_LINGER_TIMEOUT).try_into().unwrap(),
            )),
        ) {
            log::warn!(
                "Unable to set LINGER option on TCP link {} => {} : {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Build the Tcp object
        Tcp {
            socket,
            src_addr,
            dst_addr,
        }
    }
}

#[async_trait]
impl LinkTrait for Tcp {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing TCP link: {}", self);
        // Close the underlying TCP socket
        let res = self.socket.shutdown(Shutdown::Both);
        log::trace!("TCP link shutdown {}: {:?}", self, res);
        res.map_err(|e| {
            zerror2!(ZErrorKind::IOError {
                descr: format!("{}", e),
            })
        })
    }

    #[inline]
    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        match (&self.socket).write(buffer).await {
            Ok(n) => Ok(n),
            Err(e) => {
                log::trace!("Transmission error on TCP link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        match (&self.socket).write_all(buffer).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::trace!("Transmission error on TCP link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        match (&self.socket).read(buffer).await {
            Ok(n) => Ok(n),
            Err(e) => {
                log::trace!("Reception error on TCP link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        match (&self.socket).read_exact(buffer).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::trace!("Reception error on TCP link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    fn get_src(&self) -> Locator {
        Locator::Tcp(self.src_addr)
    }

    #[inline]
    fn get_dst(&self) -> Locator {
        Locator::Tcp(self.dst_addr)
    }

    #[inline]
    fn get_mtu(&self) -> usize {
        *TCP_DEFAULT_MTU
    }

    #[inline]
    fn is_reliable(&self) -> bool {
        true
    }

    #[inline]
    fn is_streamed(&self) -> bool {
        true
    }
}

impl Drop for Tcp {
    fn drop(&mut self) {
        // Close the underlying TCP socket
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

impl fmt::Display for Tcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for Tcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tcp")
            .field("src", &self.src_addr)
            .field("dst", &self.dst_addr)
            .finish()
    }
}

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerTcp {
    socket: Arc<TcpListener>,
    sender: Sender<()>,
    receiver: Receiver<()>,
    barrier: Arc<Barrier>,
}

impl ListenerTcp {
    fn new(socket: Arc<TcpListener>) -> ListenerTcp {
        // Create the channel necessary to break the accept loop
        let (sender, receiver) = bounded::<()>(1);
        // Create the barrier necessary to detect the termination of the accept loop
        let barrier = Arc::new(Barrier::new(2));
        // Update the list of active listeners on the manager
        ListenerTcp {
            socket,
            sender,
            receiver,
            barrier,
        }
    }
}

pub struct LinkManagerTcp {
    manager: SessionManager,
    listener: Arc<RwLock<HashMap<SocketAddr, Arc<ListenerTcp>>>>,
}

impl LinkManagerTcp {
    pub(crate) fn new(manager: SessionManager) -> Self {
        Self {
            manager,
            listener: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerTrait for LinkManagerTcp {
    async fn new_link(&self, locator: &Locator) -> ZResult<Link> {
        let dst_addr = get_tcp_addr(locator)?;

        // Create the TCP connection
        let stream = match TcpStream::connect(dst_addr).await {
            Ok(stream) => stream,
            Err(e) => {
                let e = format!("Can not create a new TCP link bound to {}: {}", dst_addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }
        };
        // Create a new link object
        let src_addr = match stream.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not create a new TCP link bound to {}: {}", dst_addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };
        let dst_addr = match stream.peer_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not create a new TCP link bound to {}: {}", dst_addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };
        let link = Arc::new(Tcp::new(stream, src_addr, dst_addr));
        let link = Link::new(link);

        Ok(link)
    }

    async fn new_listener(&self, locator: &Locator) -> ZResult<Locator> {
        let addr = get_tcp_addr(locator)?;

        // Bind the TCP socket
        let socket = match TcpListener::bind(addr).await {
            Ok(socket) => Arc::new(socket),
            Err(e) => {
                let e = format!("Can not create a new TCP listener on {}: {}", addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        let local_addr = match socket.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not create a new TCP listener on {}: {}", addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };
        let listener = Arc::new(ListenerTcp::new(socket));
        // Update the list of active listeners on the manager
        zasyncwrite!(self.listener).insert(local_addr, listener.clone());

        // Spawn the accept loop for the listener
        let c_listeners = self.listener.clone();
        let c_addr = local_addr;
        let c_manager = self.manager.clone();
        task::spawn(async move {
            // Wait for the accept loop to terminate
            accept_task(listener, c_manager).await;
            // Delete the listener from the manager
            zasyncwrite!(c_listeners).remove(&c_addr);
        });

        Ok(Locator::Tcp(local_addr))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_tcp_addr(locator)?;

        // Stop the listener
        match zasyncwrite!(self.listener).remove(&addr) {
            Some(listener) => {
                // Send the stop signal
                if listener.sender.send(()).await.is_ok() {
                    // Wait for the accept loop to be stopped
                    listener.barrier.wait().await;
                }
                Ok(())
            }
            None => {
                let e = format!(
                    "Can not delete the TCP listener because it has not been found: {}",
                    addr
                );
                log::trace!("{}", e);
                zerror!(ZErrorKind::InvalidLink { descr: e })
            }
        }
    }

    async fn get_listeners(&self) -> Vec<Locator> {
        zasyncread!(self.listener)
            .keys()
            .map(|x| Locator::Tcp(*x))
            .collect()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        let mut locators = vec![];
        for addr in zasyncread!(self.listener).keys() {
            if addr.ip() == std::net::Ipv4Addr::new(0, 0, 0, 0) {
                match zenoh_util::net::get_local_addresses() {
                    Ok(ipaddrs) => {
                        for ipaddr in ipaddrs {
                            if !ipaddr.is_loopback() && ipaddr.is_ipv4() {
                                locators.push(SocketAddr::new(ipaddr, addr.port()));
                            }
                        }
                    }
                    Err(err) => log::error!("Unable to get local addresses : {}", err),
                }
            } else {
                locators.push(*addr)
            }
        }
        locators.into_iter().map(Locator::Tcp).collect()
    }
}

async fn accept_task(listener: Arc<ListenerTcp>, manager: SessionManager) {
    // The accept future
    let accept_loop = async {
        log::trace!(
            "Ready to accept TCP connections on: {:?}",
            listener.socket.local_addr()
        );
        loop {
            // Wait for incoming connections
            let stream = match listener.socket.accept().await {
                Ok((stream, _)) => stream,
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
                    task::sleep(Duration::from_micros(*TCP_ACCEPT_THROTTLE_TIME)).await;
                    continue;
                }
            };
            // Get the source and destination TCP addresses
            let src_addr = match stream.local_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    let e = format!("Can not accept TCP connection: {}", e);
                    log::warn!("{}", e);
                    continue;
                }
            };
            let dst_addr = match stream.peer_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    let e = format!("Can not accept TCP connection: {}", e);
                    log::warn!("{}", e);
                    continue;
                }
            };

            log::debug!("Accepted TCP connection on {:?}: {:?}", src_addr, dst_addr);
            // Create the new link object
            let link = Arc::new(Tcp::new(stream, src_addr, dst_addr));

            // Communicate the new link to the initial session manager
            manager.handle_new_link(Link::new(link)).await;
        }
    };

    let stop = listener.receiver.recv();
    let _ = accept_loop.race(stop).await;
    listener.barrier.wait().await;
}
