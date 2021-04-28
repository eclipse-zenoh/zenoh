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
use async_std::fs;
use async_std::net::{SocketAddr, ToSocketAddrs};
use async_std::prelude::*;
use async_std::sync::{Arc, Barrier, Mutex, RwLock};
use async_std::task;
use async_trait::async_trait;
pub use quinn::*;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;
use webpki::{DnsName, DnsNameRef};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::*;
use zenoh_util::{zasynclock, zasyncread, zasyncwrite, zerror, zerror2};

// Default ALPN protocol
pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];

// Default MTU (QUIC PDU) in bytes.
// NOTE: Since QUIC is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the QUIC MTU is constrained to
//       2^16 + 1 bytes (i.e., 65537).
const QUIC_MAX_MTU: usize = 65_537;

zconfigurable! {
    // Default MTU (QUIC PDU) in bytes.
    static ref QUIC_DEFAULT_MTU: usize = QUIC_MAX_MTU;
    // The LINGER option causes the shutdown() call to block until (1) all application data is delivered
    // to the remote end or (2) a timeout expires. The timeout is expressed in seconds.
    // More info on the LINGER option and its dynamics can be found at:
    // https://blog.netherlabs.nl/articles/2009/01/18/the-ultimate-so_linger-page-or-why-is-my-tcp-not-reliable
    static ref QUIC_LINGER_TIMEOUT: i32 = 10;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref QUIC_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[allow(unreachable_patterns)]
async fn get_quic_addr(locator: &Locator) -> ZResult<SocketAddr> {
    match locator {
        Locator::Quic(addr) => match addr {
            LocatorQuic::SocketAddr(addr) => Ok(*addr),
            LocatorQuic::DnsName(addr) => match addr.to_socket_addrs().await {
                Ok(mut addr_iter) => {
                    if let Some(addr) = addr_iter.next() {
                        Ok(addr)
                    } else {
                        let e = format!("Couldn't resolve QUIC locator: {}", addr);
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
            let e = format!("Not a QUIC locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
async fn get_quic_dns(locator: &Locator) -> ZResult<DnsName> {
    match locator {
        Locator::Quic(addr) => match addr {
            LocatorQuic::SocketAddr(addr) => {
                let e = format!("Couldn't get domain from SocketAddr: {}", addr);
                zerror!(ZErrorKind::InvalidLocator { descr: e })
            }
            LocatorQuic::DnsName(addr) => {
                // Separate the domain from the port.
                // E.g. zenoh.io:7447 returns (zenoh.io, 7447).
                let split: Vec<&str> = addr.split(':').collect();
                match split.get(0) {
                    Some(dom) => {
                        let domain = DnsNameRef::try_from_ascii_str(dom).map_err(|e| {
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
            let e = format!("Not a QUIC locator: {}", locator);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

#[allow(unreachable_patterns)]
fn get_quic_prop(property: &LocatorProperty) -> ZResult<&LocatorPropertyQuic> {
    match property {
        LocatorProperty::Quic(prop) => Ok(prop),
        _ => {
            let e = "Not a QUIC property".to_string();
            log::debug!("{}", e);
            return zerror!(ZErrorKind::InvalidLocator { descr: e });
        }
    }
}

/*************************************/
/*             LOCATOR               */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocatorQuic {
    SocketAddr(SocketAddr),
    DnsName(String),
}

impl FromStr for LocatorQuic {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(addr) => Ok(LocatorQuic::SocketAddr(addr)),
            Err(_) => Ok(LocatorQuic::DnsName(s.to_string())),
        }
    }
}

impl fmt::Display for LocatorQuic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocatorQuic::SocketAddr(addr) => write!(f, "{}", addr)?,
            LocatorQuic::DnsName(addr) => write!(f, "{}", addr)?,
        }
        Ok(())
    }
}

/*************************************/
/*            PROPERTY               */
/*************************************/
#[derive(Clone)]
pub struct LocatorPropertyQuic {
    client: Option<ClientConfigBuilder>,
    server: Option<ServerConfigBuilder>,
}

impl LocatorPropertyQuic {
    fn new(
        client: Option<ClientConfigBuilder>,
        server: Option<ServerConfigBuilder>,
    ) -> LocatorPropertyQuic {
        LocatorPropertyQuic { client, server }
    }

    pub(super) async fn from_properties(
        config: &ConfigProperties,
    ) -> ZResult<Option<LocatorProperty>> {
        let mut client_config: Option<ClientConfigBuilder> = None;
        if let Some(tls_ca_certificate) = config.get(&ZN_TLS_ROOT_CA_CERTIFICATE_KEY) {
            let ca = fs::read(tls_ca_certificate).await.map_err(|e| {
                let e = format!("Invalid QUIC CA certificate file: {}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })?;
            let ca = Certificate::from_pem(&ca).map_err(|e| {
                let e = format!("Invalid QUIC CA certificate file: {}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })?;

            let mut cc = ClientConfigBuilder::default();
            cc.protocols(ALPN_QUIC_HTTP);
            cc.add_certificate_authority(ca).map_err(|e| {
                let e = format!("Invalid QUIC CA certificate file: {}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })?;

            client_config = Some(cc);
            log::debug!("QUIC client is configured");
        }

        let mut server_config: Option<ServerConfigBuilder> = None;
        if let Some(tls_server_private_key) = config.get(&ZN_TLS_SERVER_PRIVATE_KEY_KEY) {
            if let Some(tls_server_certificate) = config.get(&ZN_TLS_SERVER_CERTIFICATE_KEY) {
                let pkey = fs::read(tls_server_private_key).await.map_err(|e| {
                    let e = format!("Invalid TLS private key file: {}", e);
                    zerror2!(ZErrorKind::IoError { descr: e })
                })?;
                let keys = PrivateKey::from_pem(&pkey).unwrap();

                let certs = fs::read(tls_server_certificate).await.map_err(|e| {
                    let e = format!("Invalid TLS server certificate file: {}", e);
                    zerror2!(ZErrorKind::IoError { descr: e })
                })?;
                let certs = CertificateChain::from_pem(&certs).map_err(|e| {
                    let e = format!("Invalid TLS server certificate file: {}", e);
                    zerror2!(ZErrorKind::IoError { descr: e })
                })?;

                let mut tc = TransportConfig::default();
                // We do not accept unidireactional streams.
                tc.max_concurrent_uni_streams(0).map_err(|e| {
                    let e = format!("Invalid QUIC server configuration: {}", e);
                    zerror2!(ZErrorKind::IoError { descr: e })
                })?;
                // For the time being we only allow one bidirectional stream
                tc.max_concurrent_bidi_streams(1).map_err(|e| {
                    let e = format!("Invalid QUIC server configuration: {}", e);
                    zerror2!(ZErrorKind::IoError { descr: e })
                })?;
                let mut sc = ServerConfig::default();
                sc.transport = Arc::new(tc);
                let mut sc = ServerConfigBuilder::new(sc);
                sc.protocols(ALPN_QUIC_HTTP);
                sc.certificate(certs, keys).map_err(|e| {
                    let e = format!("Invalid TLS server configuration: {}", e);
                    zerror2!(ZErrorKind::Other { descr: e })
                })?;

                server_config = Some(sc);
                log::debug!("QUIC server is configured");
            }
        }

        if client_config.is_none() && server_config.is_none() {
            Ok(None)
        } else {
            Ok(Some((client_config, server_config).into()))
        }
    }
}

impl From<LocatorPropertyQuic> for LocatorProperty {
    fn from(property: LocatorPropertyQuic) -> LocatorProperty {
        LocatorProperty::Quic(property)
    }
}

impl From<ClientConfigBuilder> for LocatorProperty {
    fn from(client: ClientConfigBuilder) -> LocatorProperty {
        Self::from(LocatorPropertyQuic::new(Some(client), None))
    }
}

impl From<ServerConfigBuilder> for LocatorProperty {
    fn from(server: ServerConfigBuilder) -> LocatorProperty {
        Self::from(LocatorPropertyQuic::new(None, Some(server)))
    }
}

impl From<(Option<ClientConfigBuilder>, Option<ServerConfigBuilder>)> for LocatorProperty {
    fn from(tuple: (Option<ClientConfigBuilder>, Option<ServerConfigBuilder>)) -> LocatorProperty {
        Self::from(LocatorPropertyQuic::new(tuple.0, tuple.1))
    }
}

impl From<(Option<ServerConfigBuilder>, Option<ClientConfigBuilder>)> for LocatorProperty {
    fn from(tuple: (Option<ServerConfigBuilder>, Option<ClientConfigBuilder>)) -> LocatorProperty {
        Self::from(LocatorPropertyQuic::new(tuple.1, tuple.0))
    }
}

impl From<(ServerConfigBuilder, ClientConfigBuilder)> for LocatorProperty {
    fn from(tuple: (ServerConfigBuilder, ClientConfigBuilder)) -> LocatorProperty {
        Self::from((Some(tuple.0), Some(tuple.1)))
    }
}

impl From<(ClientConfigBuilder, ServerConfigBuilder)> for LocatorProperty {
    fn from(tuple: (ClientConfigBuilder, ServerConfigBuilder)) -> LocatorProperty {
        Self::from((Some(tuple.0), Some(tuple.1)))
    }
}

/*************************************/
/*              LINK                 */
/*************************************/
pub struct LinkQuic {
    connection: NewConnection,
    src_addr: SocketAddr,
    send: Mutex<SendStream>,
    recv: Mutex<RecvStream>,
}

impl LinkQuic {
    fn new(
        connection: NewConnection,
        src_addr: SocketAddr,
        send: SendStream,
        recv: RecvStream,
    ) -> LinkQuic {
        // Build the Quic object
        LinkQuic {
            connection,
            src_addr,
            send: Mutex::new(send),
            recv: Mutex::new(recv),
        }
    }

    pub(crate) async fn close(&self) -> ZResult<()> {
        log::trace!("Closing QUIC link: {}", self);
        // Flush the QUIC stream
        let mut guard = zasynclock!(self.send);
        if let Err(e) = guard.finish().await {
            log::trace!("Error closing QUIC stream {}: {}", self, e);
        }
        self.connection.connection.close(VarInt::from_u32(0), &[0]);
        Ok(())
    }

    pub(crate) async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        let mut guard = zasynclock!(self.send);
        guard.write(buffer).await.map_err(|e| {
            log::trace!("Write error on QUIC link {}: {}", self, e);
            let e = e.to_string();
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    pub(crate) async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut guard = zasynclock!(self.send);
        guard.write_all(buffer).await.map_err(|e| {
            log::trace!("Write error on QUIC link {}: {}", self, e);
            let e = e.to_string();
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    pub(crate) async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let mut guard = zasynclock!(self.recv);
        guard
            .read(buffer)
            .await
            .map_err(|e| {
                log::trace!("Read error on QUIC link {}: {}", self, e);
                let e = e.to_string();
                zerror2!(ZErrorKind::IoError { descr: e })
            })?
            .ok_or_else(|| {
                let e = format!(
                    "Read error on QUIC link {}: stream {} has been closed",
                    self,
                    guard.id()
                );
                log::trace!("{}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })
    }

    pub(crate) async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let mut guard = zasynclock!(self.recv);
        guard.read_exact(buffer).await.map_err(|e| {
            log::trace!("Read error on QUIC link {}: {}", self, e);
            let e = e.to_string();
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    #[inline(always)]
    pub(crate) fn get_src(&self) -> Locator {
        Locator::Quic(LocatorQuic::SocketAddr(self.src_addr))
    }

    #[inline(always)]
    pub(crate) fn get_dst(&self) -> Locator {
        Locator::Quic(LocatorQuic::SocketAddr(
            self.connection.connection.remote_address(),
        ))
    }

    #[inline(always)]
    pub(crate) fn get_mtu(&self) -> usize {
        *QUIC_DEFAULT_MTU
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

impl Drop for LinkQuic {
    fn drop(&mut self) {
        self.connection.connection.close(VarInt::from_u32(0), &[0]);
    }
}

impl fmt::Display for LinkQuic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} => {}",
            self.src_addr,
            self.connection.connection.remote_address()
        )?;
        Ok(())
    }
}

impl fmt::Debug for LinkQuic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Quic")
            .field("src", &self.src_addr)
            .field("dst", &self.connection.connection.remote_address())
            .finish()
    }
}

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerQuic {
    endpoint: Endpoint,
    sender: Sender<()>,
    receiver: Receiver<()>,
    barrier: Arc<Barrier>,
}

impl ListenerQuic {
    fn new(endpoint: Endpoint) -> ListenerQuic {
        // Create the channel necessary to break the accept loop
        let (sender, receiver) = bounded::<()>(1);
        // Create the barrier necessary to detect the termination of the accept loop
        let barrier = Arc::new(Barrier::new(2));
        // Update the list of active listeners on the manager
        ListenerQuic {
            endpoint,
            sender,
            receiver,
            barrier,
        }
    }
}

pub struct LinkManagerQuic {
    manager: SessionManager,
    listener: Arc<RwLock<HashMap<SocketAddr, Arc<ListenerQuic>>>>,
}

impl LinkManagerQuic {
    pub(crate) fn new(manager: SessionManager) -> Self {
        Self {
            manager,
            listener: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerTrait for LinkManagerQuic {
    async fn new_link(&self, locator: &Locator, ps: Option<&LocatorProperty>) -> ZResult<Link> {
        let domain = get_quic_dns(locator).await?;
        let addr = get_quic_addr(locator).await?;
        let host: &str = domain.as_ref().into();

        // Initialize the QUIC connection
        let mut config = match ps {
            Some(prop) => {
                let quic_prop = get_quic_prop(prop)?;
                match quic_prop.client.as_ref() {
                    Some(conf) => conf.clone(),
                    None => ClientConfigBuilder::default(),
                }
            }
            None => ClientConfigBuilder::default(),
        };
        config.protocols(ALPN_QUIC_HTTP);

        let mut endpoint = Endpoint::builder();
        endpoint.default_client_config(config.build());

        let (endpoint, _) = if addr.is_ipv4() {
            endpoint.bind(&"0.0.0.0:0".parse().unwrap())
        } else {
            endpoint.bind(&"[::]:0".parse().unwrap())
        }
        .map_err(|e| {
            let e = format!("Can not create a new QUIC link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let src_addr = endpoint.local_addr().map_err(|e| {
            let e = format!("Can not create a new QUIC link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let quic_conn = endpoint
            .connect(&addr, &host)
            .map_err(|e| {
                let e = format!("Can not create a new QUIC link bound to {}: {}", host, e);
                zerror2!(ZErrorKind::InvalidLink { descr: e })
            })?
            .await
            .map_err(|e| {
                let e = format!("Can not create a new QUIC link bound to {}: {}", host, e);
                zerror2!(ZErrorKind::InvalidLink { descr: e })
            })?;

        let (send, recv) = quic_conn.connection.open_bi().await.map_err(|e| {
            let e = format!("Can not create a new QUIC link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let link = Arc::new(LinkQuic::new(quic_conn, src_addr, send, recv));

        Ok(Link::Quic(link))
    }

    async fn new_listener(
        &self,
        locator: &Locator,
        ps: Option<&LocatorProperty>,
    ) -> ZResult<Locator> {
        let addr = get_quic_addr(locator).await?;

        // Verify there is a valid ServerConfig
        let prop = ps.as_ref().ok_or_else(|| {
            let e = format!(
                "Can not create a new QUIC listener on {}: no ServerConfig provided",
                addr
            );
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;
        let quic_prop = get_quic_prop(prop)?;
        let config = quic_prop.server.as_ref().ok_or_else(|| {
            let e = format!(
                "Can not create a new QUIC listener on {}: no ServerConfig provided",
                addr
            );
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Initialize the Endpoint
        let mut endpoint = Endpoint::builder();
        endpoint.listen(config.clone().build());
        let (endpoint, acceptor) = endpoint.bind(&addr).map_err(|e| {
            let e = format!("Can not create a new QUIC listener on {}: {}", addr, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let local_addr = endpoint.local_addr().map_err(|e| {
            let e = format!("Can not create a new QUIC listener on {}: {}", addr, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Initialize the QuicAcceptor
        let listener = Arc::new(ListenerQuic::new(endpoint));
        // Update the list of active listeners on the manager
        zasyncwrite!(self.listener).insert(local_addr, listener.clone());

        // Spawn the accept loop for the listener
        let c_listeners = self.listener.clone();
        let c_addr = local_addr;
        let c_manager = self.manager.clone();
        task::spawn(async move {
            // Wait for the accept loop to terminate
            accept_task(listener, acceptor, c_manager).await;
            // Delete the listener from the manager
            zasyncwrite!(c_listeners).remove(&c_addr);
        });

        Ok(Locator::Quic(LocatorQuic::SocketAddr(local_addr)))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_quic_addr(locator).await?;

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
                    "Can not delete the QUIC listener because it has not been found: {}",
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
            .map(|x| Locator::Quic(LocatorQuic::SocketAddr(*x)))
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
            .map(|x| Locator::Quic(LocatorQuic::SocketAddr(x)))
            .collect()
    }
}

async fn accept_task(listener: Arc<ListenerQuic>, mut acceptor: Incoming, manager: SessionManager) {
    // The accept future
    let accept_loop = async {
        log::trace!(
            "Ready to accept QUIC connections on: {:?}",
            listener.endpoint.local_addr()
        );
        loop {
            // Wait for incoming connections
            let mut quic_conn = match acceptor.next().await {
                Some(qc) => match qc.await {
                    Ok(qc) => qc,
                    Err(e) => {
                        log::warn!("QUIC acceptor failed: {:?}", e);
                        continue;
                    }
                },
                None => {
                    log::warn!("QUIC acceptor failed. Hint: you might want to increase the system open file limit",);
                    // Throttle the accept loop upon an error
                    // NOTE: This might be due to various factors. However, the most common case is that
                    //       the process has reached the maximum number of open files in the system. On
                    //       Linux systems this limit can be changed by using the "ulimit" command line
                    //       tool. In case of systemd-based systems, this can be changed by using the
                    //       "sysctl" command line tool.
                    task::sleep(Duration::from_micros(*QUIC_ACCEPT_THROTTLE_TIME)).await;
                    continue;
                }
            };

            // Get the bideractional streams. Note that we don't allow unidirectional streams.
            let (send, recv) = match quic_conn.bi_streams.next().await {
                Some(bs) => match bs {
                    Ok((send, recv)) => (send, recv),
                    Err(e) => {
                        log::warn!("QUIC acceptor failed: {:?}", e);
                        continue;
                    }
                },
                None => {
                    log::warn!("QUIC connection has no streams: {:?}", quic_conn.connection);
                    continue;
                }
            };

            let src_addr = match listener.endpoint.local_addr() {
                Ok(addr) => addr,
                Err(e) => {
                    log::warn!("QUIC connection has no streams: {:?}", e);
                    continue;
                }
            };
            let dst_addr = quic_conn.connection.remote_address();
            log::debug!("Accepted QUIC connection on {:?}: {:?}", src_addr, dst_addr);
            // Create the new link object
            let link = Arc::new(LinkQuic::new(quic_conn, src_addr, send, recv));

            // Communicate the new link to the initial session manager
            manager.handle_new_link(Link::Quic(link), None).await;
        }
    };

    let stop = listener.receiver.recv();
    let _ = accept_loop.race(stop).await;
    listener.barrier.wait().await;
}
