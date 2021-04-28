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
pub use async_rustls::rustls::*;
pub use async_rustls::webpki::*;
use async_rustls::{rustls::internal::pemfile, TlsAcceptor, TlsConnector, TlsStream};
use async_std::channel::{bounded, Receiver, Sender};
use async_std::fs;
use async_std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use async_std::prelude::*;
use async_std::sync::{Arc, Barrier, Mutex, RwLock};
use async_std::task;
use async_trait::async_trait;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::io::Cursor;
use std::net::Shutdown;
use std::str::FromStr;
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::*;
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
            LocatorTls::DnsName(addr) => match addr.to_socket_addrs().await {
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
            LocatorTls::DnsName(addr) => {
                // Separate the domain from the port.
                // E.g. zenoh.io:7447 returns (zenoh.io, 7447).
                let split: Vec<&str> = addr.split(':').collect();
                match split.get(0) {
                    Some(dom) => {
                        let domain = DNSNameRef::try_from_ascii_str(dom).map_err(|e| {
                            let e = e.to_string();
                            zerror2!(ZErrorKind::InvalidLocator { descr: e })
                        })?;
                        Ok(domain.to_owned())
                    }
                    None => {
                        let e = format!("Couldn't get domain for: {}", addr);
                        zerror!(ZErrorKind::InvalidLocator { descr: e })
                    }
                }
            }
        },
        _ => {
            let e = format!("Not a TLS locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
fn get_tls_prop(property: &LocatorProperty) -> ZResult<&LocatorPropertyTls> {
    match property {
        LocatorProperty::Tls(prop) => Ok(prop),
        _ => {
            let e = "Not a TLS property".to_string();
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
    DnsName(String),
}

impl FromStr for LocatorTls {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(addr) => Ok(LocatorTls::SocketAddr(addr)),
            Err(_) => Ok(LocatorTls::DnsName(s.to_string())),
        }
    }
}

impl fmt::Display for LocatorTls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocatorTls::SocketAddr(addr) => write!(f, "{}", addr)?,
            LocatorTls::DnsName(addr) => write!(f, "{}", addr)?,
        }
        Ok(())
    }
}

/*************************************/
/*            PROPERTY               */
/*************************************/
#[derive(Clone)]
pub struct LocatorPropertyTls {
    client: Option<Arc<ClientConfig>>,
    server: Option<Arc<ServerConfig>>,
}

impl LocatorPropertyTls {
    fn new(
        client: Option<Arc<ClientConfig>>,
        server: Option<Arc<ServerConfig>>,
    ) -> LocatorPropertyTls {
        LocatorPropertyTls { client, server }
    }

    pub(super) async fn from_properties(
        config: &ConfigProperties,
    ) -> ZResult<Option<LocatorProperty>> {
        let mut client_config: Option<ClientConfig> = None;
        if let Some(tls_ca_certificate) = config.get(&ZN_TLS_ROOT_CA_CERTIFICATE_KEY) {
            let ca = fs::read(tls_ca_certificate).await.map_err(|e| {
                zerror2!(ZErrorKind::Other {
                    descr: format!("Invalid TLS CA certificate file: {}", e)
                })
            })?;
            let mut cc = ClientConfig::new();
            let _ = cc
                .root_store
                .add_pem_file(&mut Cursor::new(ca))
                .map_err(|_| {
                    zerror2!(ZErrorKind::Other {
                        descr: "Invalid TLS CA certificate file".to_string()
                    })
                })?;
            client_config = Some(cc);
            log::debug!("TLS client is configured");
        }

        let mut server_config: Option<ServerConfig> = None;
        if let Some(tls_server_private_key) = config.get(&ZN_TLS_SERVER_PRIVATE_KEY_KEY) {
            if let Some(tls_server_certificate) = config.get(&ZN_TLS_SERVER_CERTIFICATE_KEY) {
                let pkey = fs::read(tls_server_private_key).await.map_err(|e| {
                    zerror2!(ZErrorKind::Other {
                        descr: format!("Invalid TLS private key file: {}", e)
                    })
                })?;
                let mut keys = pemfile::rsa_private_keys(&mut Cursor::new(pkey)).unwrap();

                let cert = fs::read(tls_server_certificate).await.map_err(|e| {
                    zerror2!(ZErrorKind::Other {
                        descr: format!("Invalid TLS server certificate file: {}", e)
                    })
                })?;
                let certs = pemfile::certs(&mut Cursor::new(cert)).unwrap();

                let mut sc = ServerConfig::new(NoClientAuth::new());
                sc.set_single_cert(certs, keys.remove(0)).unwrap();
                server_config = Some(sc);
                log::debug!("TLS server is configured");
            }
        }

        if client_config.is_none() && server_config.is_none() {
            Ok(None)
        } else {
            Ok(Some((client_config, server_config).into()))
        }
    }
}

impl From<LocatorPropertyTls> for LocatorProperty {
    fn from(property: LocatorPropertyTls) -> LocatorProperty {
        LocatorProperty::Tls(property)
    }
}

impl From<(Arc<ClientConfig>, Arc<ServerConfig>)> for LocatorProperty {
    fn from(tuple: (Arc<ClientConfig>, Arc<ServerConfig>)) -> LocatorProperty {
        Self::from(LocatorPropertyTls::new(Some(tuple.0), Some(tuple.1)))
    }
}

impl From<(ClientConfig, ServerConfig)> for LocatorProperty {
    fn from(tuple: (ClientConfig, ServerConfig)) -> LocatorProperty {
        Self::from((Arc::new(tuple.0), Arc::new(tuple.1)))
    }
}

impl From<Arc<ClientConfig>> for LocatorProperty {
    fn from(client: Arc<ClientConfig>) -> LocatorProperty {
        Self::from(LocatorPropertyTls::new(Some(client), None))
    }
}

impl From<ClientConfig> for LocatorProperty {
    fn from(client: ClientConfig) -> LocatorProperty {
        Self::from(Arc::new(client))
    }
}

impl From<Arc<ServerConfig>> for LocatorProperty {
    fn from(server: Arc<ServerConfig>) -> LocatorProperty {
        Self::from(LocatorPropertyTls::new(None, Some(server)))
    }
}

impl From<ServerConfig> for LocatorProperty {
    fn from(server: ServerConfig) -> LocatorProperty {
        Self::from(Arc::new(server))
    }
}

impl From<(Option<Arc<ClientConfig>>, Option<Arc<ServerConfig>>)> for LocatorProperty {
    fn from(tuple: (Option<Arc<ClientConfig>>, Option<Arc<ServerConfig>>)) -> LocatorProperty {
        Self::from(LocatorPropertyTls::new(tuple.0, tuple.1))
    }
}

impl From<(Option<Arc<ServerConfig>>, Option<Arc<ClientConfig>>)> for LocatorProperty {
    fn from(tuple: (Option<Arc<ServerConfig>>, Option<Arc<ClientConfig>>)) -> LocatorProperty {
        Self::from(LocatorPropertyTls::new(tuple.1, tuple.0))
    }
}

impl From<(Option<ClientConfig>, Option<ServerConfig>)> for LocatorProperty {
    fn from(mut tuple: (Option<ClientConfig>, Option<ServerConfig>)) -> LocatorProperty {
        let client_config = tuple.0.take().map(Arc::new);
        let server_config = tuple.1.take().map(Arc::new);
        Self::from((client_config, server_config))
    }
}

impl From<(Option<ServerConfig>, Option<ClientConfig>)> for LocatorProperty {
    fn from(mut tuple: (Option<ServerConfig>, Option<ClientConfig>)) -> LocatorProperty {
        let client_config = tuple.1.take().map(Arc::new);
        let server_config = tuple.0.take().map(Arc::new);
        Self::from((client_config, server_config))
    }
}

/*************************************/
/*              LINK                 */
/*************************************/
pub struct LinkTls {
    // The underlying socket as returned from the async-rustls library
    // NOTE: TlsStream requires &mut for read and write operations. This means
    //       that concurrent reads and writes are not possible. To achieve that,
    //       we use an UnsafeCell for interior mutability. Using an UnsafeCell
    //       is safe in our case since the transmission and reception logic
    //       already ensures that no concurrent reads or writes can happen on
    //       the same stream: there is only one task at the time that writes on
    //       the stream and only one task at the time that reads from the stream.
    inner: UnsafeCell<TlsStream<TcpStream>>,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    // The destination socket address of this link (address used on the local host)
    dst_addr: SocketAddr,
    // Make sure there are no concurrent read or writes
    write_mtx: Mutex<()>,
    read_mtx: Mutex<()>,
}

unsafe impl Send for LinkTls {}
unsafe impl Sync for LinkTls {}

impl LinkTls {
    fn new(socket: TlsStream<TcpStream>, src_addr: SocketAddr, dst_addr: SocketAddr) -> LinkTls {
        let (tcp_stream, _) = socket.get_ref();
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

        // Build the Tls object
        LinkTls {
            inner: UnsafeCell::new(socket),
            src_addr,
            dst_addr,
            write_mtx: Mutex::new(()),
            read_mtx: Mutex::new(()),
        }
    }

    // NOTE: It is safe to suppress Clippy warning since no concurrent reads
    //       or concurrent writes will ever happen. The read_mtx and write_mtx
    //       are respectively acquired in any read and write operation.
    #[allow(clippy::mut_from_ref)]
    fn get_sock_mut(&self) -> &mut TlsStream<TcpStream> {
        unsafe { &mut *self.inner.get() }
    }

    pub(crate) async fn close(&self) -> ZResult<()> {
        log::trace!("Closing TLS link: {}", self);
        // Flush the TLS stream
        let _guard = zasynclock!(self.write_mtx);
        let tls_stream = self.get_sock_mut();
        let res = tls_stream.flush().await;
        log::trace!("TLS link flush {}: {:?}", self, res);
        // Close the underlying TCP stream
        let (tcp_stream, _) = tls_stream.get_ref();
        let res = tcp_stream.shutdown(Shutdown::Both);
        log::trace!("TLS link shutdown {}: {:?}", self, res);
        res.map_err(|e| {
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string(),
            })
        })
    }

    pub(crate) async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        let _guard = zasynclock!(self.write_mtx);
        self.get_sock_mut().write(buffer).await.map_err(|e| {
            log::trace!("Write error on TLS link {}: {}", self, e);
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })
    }

    pub(crate) async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let _guard = zasynclock!(self.write_mtx);
        self.get_sock_mut().write_all(buffer).await.map_err(|e| {
            log::trace!("Write error on TLS link {}: {}", self, e);
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })
    }

    pub(crate) async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let _guard = zasynclock!(self.read_mtx);
        self.get_sock_mut().read(buffer).await.map_err(|e| {
            log::trace!("Read error on TLS link {}: {}", self, e);
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })
    }

    pub(crate) async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let _guard = zasynclock!(self.read_mtx);
        self.get_sock_mut().read_exact(buffer).await.map_err(|e| {
            log::trace!("Read error on TLS link {}: {}", self, e);
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })
    }

    #[inline(always)]
    pub(crate) fn get_src(&self) -> Locator {
        Locator::Tls(LocatorTls::SocketAddr(self.src_addr))
    }

    #[inline(always)]
    pub(crate) fn get_dst(&self) -> Locator {
        Locator::Tls(LocatorTls::SocketAddr(self.dst_addr))
    }

    #[inline(always)]
    pub(crate) fn get_mtu(&self) -> usize {
        *TLS_DEFAULT_MTU
    }

    #[inline(always)]
    pub(crate) fn is_reliable(&self) -> bool {
        true
    }

    #[inline(always)]
    pub(crate) fn is_streamed(&self) -> bool {
        true
    }
}

impl Drop for LinkTls {
    fn drop(&mut self) {
        // Close the underlying TCP stream
        let (tcp_stream, _) = self.get_sock_mut().get_ref();
        let _ = tcp_stream.shutdown(Shutdown::Both);
    }
}

impl fmt::Display for LinkTls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for LinkTls {
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
    socket: TcpListener,
    acceptor: TlsAcceptor,
    sender: Sender<()>,
    receiver: Receiver<()>,
    barrier: Arc<Barrier>,
}

impl ListenerTls {
    fn new(socket: TcpListener, acceptor: TlsAcceptor) -> ListenerTls {
        // Create the channel necessary to break the accept loop
        let (sender, receiver) = bounded::<()>(1);
        // Create the barrier necessary to detect the termination of the accept loop
        let barrier = Arc::new(Barrier::new(2));
        // Update the list of active listeners on the manager
        ListenerTls {
            socket,
            acceptor,
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
    async fn new_link(&self, locator: &Locator, ps: Option<&LocatorProperty>) -> ZResult<Link> {
        let domain = get_tls_dns(locator).await?;
        let addr = get_tls_addr(locator).await?;
        let host: &str = domain.as_ref().into();

        // Initialize the TcpStream
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

        let link = Arc::new(LinkTls::new(tls_stream, src_addr, dst_addr));

        Ok(Link::Tls(link))
    }

    async fn new_listener(
        &self,
        locator: &Locator,
        ps: Option<&LocatorProperty>,
    ) -> ZResult<Locator> {
        let addr = get_tls_addr(locator).await?;

        // Verify there is a valid ServerConfig
        let prop = ps.as_ref().ok_or_else(|| {
            let e = format!(
                "Can not create a new TLS listener on {}: no ServerConfig provided",
                addr
            );
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;
        let tls_prop = get_tls_prop(prop)?;
        let config = tls_prop.server.as_ref().ok_or_else(|| {
            let e = format!(
                "Can not create a new TLS listener on {}: no ServerConfig provided",
                addr
            );
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Initialize the TcpListener
        let socket = TcpListener::bind(addr).await.map_err(|e| {
            let e = format!("Can not create a new TLS listener on {}: {}", addr, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let local_addr = socket.local_addr().map_err(|e| {
            let e = format!("Can not create a new TLS listener on {}: {}", addr, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Initialize the TlsAcceptor
        let acceptor = TlsAcceptor::from(config.clone());
        let listener = Arc::new(ListenerTls::new(socket, acceptor));
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

        Ok(Locator::Tls(LocatorTls::SocketAddr(local_addr)))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_tls_addr(locator).await?;

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
            .map(|x| Locator::Tls(LocatorTls::SocketAddr(*x)))
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
        locators
            .into_iter()
            .map(|x| Locator::Tls(LocatorTls::SocketAddr(x)))
            .collect()
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
            let tcp_stream = match listener.socket.accept().await {
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
            let src_addr = match tcp_stream.local_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    let e = format!("Can not accept TLS connection: {}", e);
                    log::warn!("{}", e);
                    continue;
                }
            };
            let dst_addr = match tcp_stream.peer_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    let e = format!("Can not accept TLS connection: {}", e);
                    log::warn!("{}", e);
                    continue;
                }
            };

            // Accept the TLS connection
            let tls_stream = match listener.acceptor.accept(tcp_stream).await {
                Ok(stream) => TlsStream::Server(stream),
                Err(e) => {
                    let e = format!("Can not accept TLS connection: {}", e);
                    log::warn!("{}", e);
                    continue;
                }
            };

            log::debug!("Accepted TLS connection on {:?}: {:?}", src_addr, dst_addr);
            // Create the new link object
            let link = Arc::new(LinkTls::new(tls_stream, src_addr, dst_addr));

            // Communicate the new link to the initial session manager
            manager.handle_new_link(Link::Tls(link), None).await;
        }
    };

    let stop = listener.receiver.recv();
    let _ = accept_loop.race(stop).await;
    listener.barrier.wait().await;
}
