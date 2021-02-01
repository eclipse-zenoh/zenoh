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
use super::{Link, LinkManagerTrait, LinkProperty, LinkTrait, Locator};
use crate::session::SessionManager;
use async_rustls::rustls::{ClientConfig, ServerConfig};
use async_rustls::webpki::{DNSName, DNSNameRef};
use async_rustls::{TlsConnector, TlsStream};
use async_std::channel::{bounded, Receiver, Sender};
use async_std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use async_std::prelude::*;
use async_std::sync::{Arc, Barrier, RwLock};
use async_std::task;
use async_trait::async_trait;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasyncread, zasyncwrite, zerror, zerror2};

// Default MTU (TLS PDU) in bytes.
// NOTE: Since TLS is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the TLS MTU is constrained to
//       2^16 + 1 bytes (i.e., 65537).
const TLS_MAX_MTU: usize = 65_537;

zconfigurable! {
    // Default MTU (TLS PDU) in bytes.
    static ref TLS_DEFAULT_MTU: usize = TLS_MAX_MTU;
    // The LINGER option causes the shutdown() call to block until (1) all application data is delivered
    // to the remote end or (2) a timeout expires. The timeout is expressed in seconds.
    // More info on the LINGER option and its dynamics can be found at:
    // https://blog.netherlabs.nl/articles/2009/01/18/the-ultimate-so_linger-page-or-why-is-my-tcp-not-reliable
    static ref TLS_LINGER_TIMEOUT: i32 = 10;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref TLS_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[allow(unreachable_patterns)]
async fn get_tls_addr(locator: &Locator) -> ZResult<SocketAddr> {
    match locator {
        Locator::Tls(addr) => match addr {
            LocatorTls::SocketAddr(addr) => Ok(*addr),
            LocatorTls::DNSName(addr) => match addr.to_socket_addrs().await {
                Ok(mut addr_iter) => {
                    if let Some(addr) = addr_iter.next() {
                        Ok(addr)
                    } else {
                        let e = format!("Couldn't resolve TLS locator: {}", addr);
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
            let e = format!("Not a TLS locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
async fn get_tls_dns(locator: &Locator) -> ZResult<DNSName> {
    match locator {
        Locator::Tls(addr) => match addr {
            LocatorTls::SocketAddr(addr) => {
                let e = format!("Couldn't get domain from SocketAddr: {}", addr);
                zerror!(ZErrorKind::InvalidLocator { descr: e })
            }
            LocatorTls::DNSName(addr) => {
                let domain = DNSNameRef::try_from_ascii_str(addr).map_err(|e| {
                    let e = format!("{}", e);
                    zerror2!(ZErrorKind::InvalidLocator { descr: e })
                })?;
                Ok(domain.to_owned())
            }
        },
        _ => {
            let e = format!("Not a TCP locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
fn get_tls_prop(property: &LinkProperty) -> ZResult<&LinkPropertyTls> {
    match property {
        LinkProperty::Tls(prop) => Ok(prop),
        _ => {
            let e = format!("Not a TLS property");
            log::debug!("{}", e);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

/*************************************/
/*             LOCATOR               */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocatorTls {
    SocketAddr(SocketAddr),
    DNSName(String),
}

impl FromStr for LocatorTls {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(addr) => Ok(LocatorTls::SocketAddr(addr)),
            Err(_) => Ok(LocatorTls::DNSName(s.to_string())),
        }
    }
}

impl fmt::Display for LocatorTls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocatorTls::SocketAddr(addr) => write!(f, "{}", addr)?,
            LocatorTls::DNSName(addr) => write!(f, "{}", addr)?,
        }
        Ok(())
    }
}

/*************************************/
/*            PROPERTY               */
/*************************************/
pub struct LinkPropertyTls {
    client: Option<Arc<ClientConfig>>,
    server: Option<ServerConfig>,
}

/*************************************/
/*              LINK                 */
/*************************************/
pub struct Tls {
    // The underlying socket as returned from the async-rustls library
    // NOTE: TlsStream requires &mut for read and write operations. This means
    //       that concurrent reads and writes are not possible. To achieve that,
    //       we use an UnsafeCell for interior mutability. Usingg an UnsafeCell
    //       is safe in our case since the transmission and reception logic
    //       already ensures that no concurrent reads or writes can happen on
    //       the same stream: there is only one task at the time that writes on
    //       the stream and only one task at the time that reads from the stream.
    inner: UnsafeCell<TlsStream<TcpStream>>,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    // The destination socket address of this link (address used on the local host)
    dst_addr: SocketAddr,
}

impl Tls {
    fn new(socket: TlsStream<TcpStream>, src_addr: SocketAddr, dst_addr: SocketAddr) -> Tls {
        // Build the Tls object
        Tls {
            inner: UnsafeCell::new(socket),
            src_addr,
            dst_addr,
        }
    }

    fn get_sock(&self) -> &TlsStream<TcpStream> {
        unsafe { &*self.inner.get() }
    }

    fn get_sock_mut(&self) -> &mut TlsStream<TcpStream> {
        unsafe { &mut *self.inner.get() }
    }
}

unsafe impl Send for Tls {}
unsafe impl Sync for Tls {}

#[async_trait]
impl LinkTrait for Tls {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing TLS link: {}", self);
        // Close the underlying TLS socket
        let res = self.get_sock_mut().flush().await;
        log::trace!("TLS link shutdown {}: {:?}", self, res);
        res.map_err(|e| {
            zerror2!(ZErrorKind::IOError {
                descr: format!("{}", e),
            })
        })
    }

    #[inline]
    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        match self.get_sock_mut().write(buffer).await {
            Ok(n) => Ok(n),
            Err(e) => {
                log::trace!("Transmission error on TLS link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        match self.get_sock_mut().write_all(buffer).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::trace!("Transmission error on TLS link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        match self.get_sock_mut().read(buffer).await {
            Ok(n) => Ok(n),
            Err(e) => {
                log::trace!("Reception error on TLS link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        match self.get_sock_mut().read_exact(buffer).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::trace!("Reception error on TLS link {}: {}", self, e);
                zerror!(ZErrorKind::IOError {
                    descr: format!("{}", e)
                })
            }
        }
    }

    #[inline]
    fn get_src(&self) -> Locator {
        Locator::Tls(LocatorTls::SocketAddr(self.src_addr))
    }

    #[inline]
    fn get_dst(&self) -> Locator {
        Locator::Tls(LocatorTls::SocketAddr(self.dst_addr))
    }

    #[inline]
    fn get_mtu(&self) -> usize {
        *TLS_DEFAULT_MTU
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

impl fmt::Display for Tls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for Tls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tls")
            .field("src", &self.src_addr)
            .field("dst", &self.dst_addr)
            .finish()
    }
}

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerTls {
    socket: Arc<TcpListener>,
    sender: Sender<()>,
    receiver: Receiver<()>,
    barrier: Arc<Barrier>,
}

impl ListenerTls {
    fn new(socket: Arc<TcpListener>) -> ListenerTls {
        // Create the channel necessary to break the accept loop
        let (sender, receiver) = bounded::<()>(1);
        // Create the barrier necessary to detect the termination of the accept loop
        let barrier = Arc::new(Barrier::new(2));
        // Update the list of active listeners on the manager
        ListenerTls {
            socket,
            sender,
            receiver,
            barrier,
        }
    }
}

pub struct LinkManagerTls {
    manager: SessionManager,
    listener: Arc<RwLock<HashMap<SocketAddr, Arc<ListenerTls>>>>,
}

impl LinkManagerTls {
    pub(crate) fn new(manager: SessionManager) -> Self {
        Self {
            manager,
            listener: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerTrait for LinkManagerTls {
    async fn new_link(&self, locator: &Locator, ps: Option<&LinkProperty>) -> ZResult<Link> {
        let domain = get_tls_dns(locator).await?;
        let addr = get_tls_addr(locator).await?;

        // Initialize the TcpStream
        let host: &str = domain.as_ref().into();
        let addr = match host.to_socket_addrs().await {
            Ok(mut addr_iter) => {
                if let Some(addr) = addr_iter.next() {
                    Ok(addr)
                } else {
                    let e = format!("Couldn't resolve TLS locator: {}", host);
                    zerror!(ZErrorKind::InvalidLocator { descr: e })
                }
            }
            Err(e) => {
                let e = format!("{}: {}", e, host);
                zerror!(ZErrorKind::InvalidLocator { descr: e })
            }
        }?;

        let tcp_stream = TcpStream::connect(addr).await.map_err(|e| {
            let e = format!("Can not create a new TLS link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::Other { descr: e })
        })?;

        let src_addr = tcp_stream.local_addr().map_err(|e| {
            let e = format!("Can not create a new TLS link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let dst_addr = tcp_stream.peer_addr().map_err(|e| {
            let e = format!("Can not create a new TLS link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Set the TLS nodelay option
        if let Err(err) = tcp_stream.set_nodelay(true) {
            log::warn!(
                "Unable to set NODEALY option on TLS link {} => {} : {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Set the TLS linger option
        if let Err(err) = zenoh_util::net::set_linger(
            &tcp_stream,
            Some(Duration::from_secs(
                (*TLS_LINGER_TIMEOUT).try_into().unwrap(),
            )),
        ) {
            log::warn!(
                "Unable to set LINGER option on TLS link {} => {} : {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Initialize the TLS stream
        let config = match ps {
            Some(prop) => {
                let tls_prop = get_tls_prop(prop)?;
                match tls_prop.client.as_ref() {
                    Some(conf) => conf.clone(),
                    None => Arc::new(ClientConfig::new()),
                }
            }
            None => Arc::new(ClientConfig::new()),
        };
        let connector = TlsConnector::from(config);
        let tls_stream = connector
            .connect(domain.as_ref(), tcp_stream)
            .await
            .map_err(|e| {
                let e = format!("Can not create a new TLS link bound to {}: {}", host, e);
                zerror2!(ZErrorKind::InvalidLink { descr: e })
            })?;
        let tls_stream = TlsStream::Client(tls_stream);

        let link = Arc::new(Tls::new(tls_stream, src_addr, dst_addr));

        Ok(Link::new(link))
    }

    async fn new_listener(&self, locator: &Locator, _ps: Option<LinkProperty>) -> ZResult<Locator> {
        let (addr, domain) = get_tls_addr(locator).await?;

        // Bind the TLS socket
        let socket = match TcpListener::bind(addr).await {
            Ok(socket) => Arc::new(socket),
            Err(e) => {
                let e = format!("Can not create a new TLS listener on {}: {}", addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };

        let local_addr = match socket.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                let e = format!("Can not create a new TLS listener on {}: {}", addr, e);
                log::warn!("{}", e);
                return zerror!(ZErrorKind::InvalidLink { descr: e });
            }
        };
        let listener = Arc::new(ListenerTls::new(socket));
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

        Ok(Locator::Tls((local_addr, domain.to_owned())))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_tls_addr(locator)?;

        // Stop the listener
        match zasyncwrite!(self.listener).remove(&addr) {
            Some(listener) => {
                // Send the stop signal
                let res = listener.sender.send(()).await;
                if res.is_ok() {
                    // Wait for the accept loop to be stopped
                    listener.barrier.wait().await;
                }
                Ok(())
            }
            None => {
                let e = format!(
                    "Can not delete the TLS listener because it has not been found: {}",
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
            .map(|x| Locator::Tls(*x))
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
        locators.into_iter().map(Locator::Tls).collect()
    }
}

async fn accept_task(listener: Arc<ListenerTls>, manager: SessionManager) {
    // The accept future
    let accept_loop = async {
        log::trace!(
            "Ready to accept TLS connections on: {:?}",
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
                    task::sleep(Duration::from_micros(*TLS_ACCEPT_THROTTLE_TIME)).await;
                    continue;
                }
            };
            // Get the source and destination TLS addresses
            let src_addr = match stream.local_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    let e = format!("Can not accept TLS connection: {}", e);
                    log::warn!("{}", e);
                    continue;
                }
            };
            let dst_addr = match stream.peer_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    let e = format!("Can not accept TLS connection: {}", e);
                    log::warn!("{}", e);
                    continue;
                }
            };

            log::debug!("Accepted TLS connection on {:?}: {:?}", src_addr, dst_addr);
            // Create the new link object
            let link = Arc::new(Tls::new(stream, src_addr, dst_addr));

            // Communicate the new link to the initial session manager
            manager.handle_new_link(Link::new(link), None).await;
        }
    };

    let stop = listener.receiver.recv();
    let _ = accept_loop.race(stop).await;
    listener.barrier.wait().await;
}
