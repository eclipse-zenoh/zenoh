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
use crate::{
    config::*, get_tls_addr, get_tls_host, get_tls_server_name, TLS_ACCEPT_THROTTLE_TIME,
    TLS_DEFAULT_MTU, TLS_LINGER_TIMEOUT, TLS_LOCATOR_PREFIX,
};
use async_rustls::rustls::server::AllowAnyAuthenticatedClient;
use async_rustls::rustls::version::TLS13;
pub use async_rustls::rustls::*;
use async_rustls::{TlsAcceptor, TlsConnector, TlsStream};
use async_std::fs;
use async_std::net::{SocketAddr, TcpListener, TcpStream};
use async_std::prelude::FutureExt;
use async_std::sync::Mutex as AsyncMutex;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use futures::io::AsyncReadExt;
use futures::io::AsyncWriteExt;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::net::{IpAddr, Shutdown};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
pub use webpki::*;
use zenoh_core::{zasynclock, zread, zwrite};
use zenoh_link_commons::{
    LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, NewLinkChannelSender,
};
use zenoh_protocol::core::endpoint::Config;
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{bail, zerror, ZResult};
use zenoh_sync::Signal;

pub struct LinkUnicastTls {
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
    src_locator: Locator,
    // The destination socket address of this link (address used on the local host)
    dst_addr: SocketAddr,
    dst_locator: Locator,
    // Make sure there are no concurrent read or writes
    write_mtx: AsyncMutex<()>,
    read_mtx: AsyncMutex<()>,
}

unsafe impl Send for LinkUnicastTls {}
unsafe impl Sync for LinkUnicastTls {}

impl LinkUnicastTls {
    fn new(
        socket: TlsStream<TcpStream>,
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
    ) -> LinkUnicastTls {
        let (tcp_stream, _) = socket.get_ref();
        // Set the TLS nodelay option
        if let Err(err) = tcp_stream.set_nodelay(true) {
            log::warn!(
                "Unable to set NODEALY option on TLS link {} => {}: {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Set the TLS linger option
        if let Err(err) = zenoh_util::net::set_linger(
            tcp_stream,
            Some(Duration::from_secs(
                (*TLS_LINGER_TIMEOUT).try_into().unwrap(),
            )),
        ) {
            log::warn!(
                "Unable to set LINGER option on TLS link {} => {}: {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Build the Tls object
        LinkUnicastTls {
            inner: UnsafeCell::new(socket),
            src_addr,
            src_locator: Locator::new(TLS_LOCATOR_PREFIX, src_addr.to_string(), "").unwrap(),
            dst_addr,
            dst_locator: Locator::new(TLS_LOCATOR_PREFIX, dst_addr.to_string(), "").unwrap(),
            write_mtx: AsyncMutex::new(()),
            read_mtx: AsyncMutex::new(()),
        }
    }

    // NOTE: It is safe to suppress Clippy warning since no concurrent reads
    //       or concurrent writes will ever happen. The read_mtx and write_mtx
    //       are respectively acquired in any read and write operation.
    #[allow(clippy::mut_from_ref)]
    fn get_sock_mut(&self) -> &mut TlsStream<TcpStream> {
        unsafe { &mut *self.inner.get() }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastTls {
    async fn close(&self) -> ZResult<()> {
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
        res.map_err(|e| zerror!(e).into())
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        let _guard = zasynclock!(self.write_mtx);
        self.get_sock_mut().write(buffer).await.map_err(|e| {
            log::trace!("Write error on TLS link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let _guard = zasynclock!(self.write_mtx);
        self.get_sock_mut().write_all(buffer).await.map_err(|e| {
            log::trace!("Write error on TLS link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let _guard = zasynclock!(self.read_mtx);
        self.get_sock_mut().read(buffer).await.map_err(|e| {
            log::trace!("Read error on TLS link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let _guard = zasynclock!(self.read_mtx);
        self.get_sock_mut().read_exact(buffer).await.map_err(|e| {
            log::trace!("Read error on TLS link {}: {}", self, e);
            zerror!(e).into()
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
    fn get_mtu(&self) -> u16 {
        *TLS_DEFAULT_MTU
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

impl Drop for LinkUnicastTls {
    fn drop(&mut self) {
        // Close the underlying TCP stream
        let (tcp_stream, _) = self.get_sock_mut().get_ref();
        let _ = tcp_stream.shutdown(Shutdown::Both);
    }
}

impl fmt::Display for LinkUnicastTls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastTls {
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
struct ListenerUnicastTls {
    endpoint: EndPoint,
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastTls {
    fn new(
        endpoint: EndPoint,
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> ListenerUnicastTls {
        ListenerUnicastTls {
            endpoint,
            active,
            signal,
            handle,
        }
    }
}

pub struct LinkManagerUnicastTls {
    manager: NewLinkChannelSender,
    listeners: Arc<RwLock<HashMap<SocketAddr, ListenerUnicastTls>>>,
}

impl LinkManagerUnicastTls {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastTls {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let epaddr = endpoint.address();
        let epconf = endpoint.config();

        let server_name = get_tls_server_name(&epaddr)?;
        let addr = get_tls_addr(&epaddr).await?;

        // Initialize the TLS Config
        let client_config = TlsClientConfig::new(&epconf)
            .await
            .map_err(|e| zerror!("Cannot create a new TLS listener to {endpoint}: {e}"))?;
        let config = Arc::new(client_config.client_config);
        let connector = TlsConnector::from(config);

        // Initialize the TcpStream
        let tcp_stream = TcpStream::connect(addr).await.map_err(|e| {
            zerror!(
                "Can not create a new TLS link bound to {:?}: {}",
                server_name,
                e
            )
        })?;

        let src_addr = tcp_stream.local_addr().map_err(|e| {
            zerror!(
                "Can not create a new TLS link bound to {:?}: {}",
                server_name,
                e
            )
        })?;

        let dst_addr = tcp_stream.peer_addr().map_err(|e| {
            zerror!(
                "Can not create a new TLS link bound to {:?}: {}",
                server_name,
                e
            )
        })?;

        // Initialize the TlsStream
        let tls_stream = connector
            .connect(server_name.to_owned(), tcp_stream)
            .await
            .map_err(|e| {
                zerror!(
                    "Can not create a new TLS link bound to {:?}: {}",
                    server_name,
                    e
                )
            })?;
        let tls_stream = TlsStream::Client(tls_stream);

        let link = Arc::new(LinkUnicastTls::new(tls_stream, src_addr, dst_addr));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
        let epaddr = endpoint.address();
        let epconf = endpoint.config();

        let addr = get_tls_addr(&epaddr).await?;
        let host = get_tls_host(&epaddr)?;

        // Initialize TlsConfig
        let tls_server_config = TlsServerConfig::new(&epconf)
            .await
            .map_err(|e| zerror!("Cannot create a new TLS listener on {addr}. {e}"))?;

        // Initialize the TcpListener
        let socket = TcpListener::bind(addr)
            .await
            .map_err(|e| zerror!("Can not create a new TLS listener on {}: {}", addr, e))?;

        let local_addr = socket
            .local_addr()
            .map_err(|e| zerror!("Can not create a new TLS listener on {}: {}", addr, e))?;
        let local_port = local_addr.port();

        // Initialize the TlsAcceptor
        let acceptor = TlsAcceptor::from(Arc::new(tls_server_config.server_config));
        let active = Arc::new(AtomicBool::new(true));
        let signal = Signal::new();

        // Spawn the accept loop for the listener
        let c_active = active.clone();
        let c_signal = signal.clone();
        let c_manager = self.manager.clone();
        let c_listeners = self.listeners.clone();
        let c_addr = local_addr;
        let handle = task::spawn(async move {
            // Wait for the accept loop to terminate
            let res = accept_task(socket, acceptor, c_active, c_signal, c_manager).await;
            zwrite!(c_listeners).remove(&c_addr);
            res
        });

        // Update the endpoint locator address
        let locator = Locator::new(
            endpoint.protocol(),
            format!("{host}:{local_port}"),
            endpoint.metadata(),
        )?;

        let listener = ListenerUnicastTls::new(endpoint, active, signal, handle);
        // Update the list of active listeners on the manager
        zwrite!(self.listeners).insert(local_addr, listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let epaddr = endpoint.address();

        let addr = get_tls_addr(&epaddr).await?;

        // Stop the listener
        let listener = zwrite!(self.listeners).remove(&addr).ok_or_else(|| {
            let e = zerror!(
                "Can not delete the TLS listener because it has not been found: {}",
                addr
            );
            log::trace!("{}", e);
            e
        })?;

        // Send the stop signal
        listener.active.store(false, Ordering::Release);
        listener.signal.trigger();
        listener.handle.await
    }

    fn get_listeners(&self) -> Vec<EndPoint> {
        zread!(self.listeners)
            .values()
            .map(|x| x.endpoint.clone())
            .collect()
    }

    fn get_locators(&self) -> Vec<Locator> {
        let mut locators = vec![];

        let guard = zread!(self.listeners);
        for (key, value) in guard.iter() {
            let (kip, kpt) = (key.ip(), key.port());

            // Either ipv4/0.0.0.0 or ipv6/[::]
            if kip.is_unspecified() {
                let mut addrs = match kip {
                    IpAddr::V4(_) => zenoh_util::net::get_ipv4_ipaddrs(),
                    IpAddr::V6(_) => zenoh_util::net::get_ipv6_ipaddrs(),
                };
                let iter = addrs.drain(..).map(|x| {
                    Locator::new(
                        value.endpoint.protocol(),
                        SocketAddr::new(x, kpt).to_string(),
                        value.endpoint.metadata(),
                    )
                    .unwrap()
                });
                locators.extend(iter);
            } else {
                locators.push(value.endpoint.to_locator());
            }
        }

        locators
    }
}

async fn accept_task(
    socket: TcpListener,
    acceptor: TlsAcceptor,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: NewLinkChannelSender,
) -> ZResult<()> {
    enum Action {
        Accept((TcpStream, SocketAddr)),
        Stop,
    }

    async fn accept(socket: &TcpListener) -> ZResult<Action> {
        let res = socket.accept().await.map_err(|e| zerror!(e))?;
        Ok(Action::Accept(res))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let src_addr = socket.local_addr().map_err(|e| {
        let e = zerror!("Can not accept TLS connections: {}", e);
        log::warn!("{}", e);
        e
    })?;

    log::trace!("Ready to accept TLS connections on: {:?}", src_addr);
    while active.load(Ordering::Acquire) {
        // Wait for incoming connections
        let (tcp_stream, dst_addr) = match accept(&socket).race(stop(signal.clone())).await {
            Ok(action) => match action {
                Action::Accept((tcp_stream, dst_addr)) => (tcp_stream, dst_addr),
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
                task::sleep(Duration::from_micros(*TLS_ACCEPT_THROTTLE_TIME)).await;
                continue;
            }
        };
        // Accept the TLS connection
        let tls_stream = match acceptor.accept(tcp_stream).await {
            Ok(stream) => TlsStream::Server(stream),
            Err(e) => {
                let e = format!("Can not accept TLS connection: {e}");
                log::warn!("{}", e);
                continue;
            }
        };

        log::debug!("Accepted TLS connection on {:?}: {:?}", src_addr, dst_addr);
        // Create the new link object
        let link = Arc::new(LinkUnicastTls::new(tls_stream, src_addr, dst_addr));

        // Communicate the new link to the initial transport manager
        if let Err(e) = manager.send_async(LinkUnicast(link)).await {
            log::error!("{}-{}: {}", file!(), line!(), e)
        }
    }

    Ok(())
}

struct TlsServerConfig {
    server_config: ServerConfig,
}

impl TlsServerConfig {
    pub async fn new(config: &Config<'_>) -> ZResult<TlsServerConfig> {
        let mut client_auth: bool = TLS_CLIENT_AUTH_DEFAULT.parse().unwrap();
        let tls_server_private_key = TlsServerConfig::load_tls_private_key(config).await?;
        let tls_server_certificate = TlsServerConfig::load_tls_certificate(config).await?;

        let mut keys: Vec<PrivateKey> =
            rustls_pemfile::rsa_private_keys(&mut Cursor::new(&tls_server_private_key))
                .map_err(|e| zerror!(e))
                .map(|mut keys| keys.drain(..).map(PrivateKey).collect())?;

        if keys.is_empty() {
            keys = rustls_pemfile::pkcs8_private_keys(&mut Cursor::new(&tls_server_private_key))
                .map_err(|e| zerror!(e))
                .map(|mut keys| keys.drain(..).map(PrivateKey).collect())?;
        }

        if keys.is_empty() {
            keys = rustls_pemfile::ec_private_keys(&mut Cursor::new(&tls_server_private_key))
                .map_err(|e| zerror!(e))
                .map(|mut keys| keys.drain(..).map(PrivateKey).collect())?;
        }

        if keys.is_empty() {
            bail!("No private key found");
        }

        let certs: Vec<Certificate> =
            rustls_pemfile::certs(&mut Cursor::new(&tls_server_certificate))
                .map_err(|e| zerror!(e))
                .map(|mut certs| certs.drain(..).map(Certificate).collect())?;

        if let Some(value) = config.get(TLS_CLIENT_AUTH) {
            client_auth = value.parse()?
        }

        let sc = if client_auth {
            let root_cert_store = load_trust_anchors(config)?.map_or_else(
                || {
                    Err(zerror!(
                        "Missing root certificates while client authentication is enabled."
                    ))
                },
                Ok,
            )?;
            ServerConfig::builder()
                .with_safe_default_cipher_suites()
                .with_safe_default_kx_groups()
                .with_protocol_versions(&[&TLS13]) // Force TLS 1.3
                .map_err(|e| zerror!(e))?
                .with_client_cert_verifier(Arc::new(AllowAnyAuthenticatedClient::new(root_cert_store)))
                .with_single_cert(certs, keys.remove(0))
                .map_err(|e| zerror!(e))?
        } else {
            ServerConfig::builder()
                .with_safe_defaults()
                .with_no_client_auth()
                .with_single_cert(certs, keys.remove(0))
                .map_err(|e| zerror!(e))?
        };
        Ok(TlsServerConfig { server_config: sc })
    }

    async fn load_tls_private_key(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_key(
            config,
            TLS_SERVER_PRIVATE_KEY_RAW,
            TLS_SERVER_PRIVATE_KEY_FILE,
        )
        .await
    }

    async fn load_tls_certificate(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_certificate(
            config,
            TLS_SERVER_CERTIFICATE_RAW,
            TLS_SERVER_CERTIFICATE_FILE,
        )
        .await
    }
}

struct TlsClientConfig {
    client_config: ClientConfig,
}

impl TlsClientConfig {
    pub async fn new(config: &Config<'_>) -> ZResult<TlsClientConfig> {
        let mut client_auth: bool = TLS_CLIENT_AUTH_DEFAULT.parse().unwrap();
        if let Some(value) = config.get(TLS_CLIENT_AUTH) {
            client_auth = value.parse()?
        }

        let root_cert_store =
            load_trust_anchors(config)?.map_or_else(|| {
                log::debug!("Field 'root_ca_certificate' not specified. Loading default Web PKI certificates instead.");
                load_default_webpki_certs()
            }, |certs| certs);
        let cc = if client_auth {
            log::debug!("Loading client authentication key and certificate...");
            let tls_client_private_key = TlsClientConfig::load_tls_private_key(config).await?;
            let tls_client_certificate = TlsClientConfig::load_tls_certificate(config).await?;

            let certs: Vec<Certificate> =
                rustls_pemfile::certs(&mut Cursor::new(&tls_client_certificate))
                    .map_err(|e| zerror!(e))
                    .map(|mut certs| certs.drain(..).map(Certificate).collect())?;

            let mut keys: Vec<PrivateKey> =
                rustls_pemfile::rsa_private_keys(&mut Cursor::new(&tls_client_private_key))
                    .map_err(|e| zerror!(e))
                    .map(|mut keys| keys.drain(..).map(PrivateKey).collect())?;

            if keys.is_empty() {
                keys =
                    rustls_pemfile::pkcs8_private_keys(&mut Cursor::new(&tls_client_private_key))
                        .map_err(|e| zerror!(e))
                        .map(|mut keys| keys.drain(..).map(PrivateKey).collect())?;
            }

            if keys.is_empty() {
                bail!("No private key found");
            }

            ClientConfig::builder()
                .with_safe_default_cipher_suites()
                .with_safe_default_kx_groups()
                .with_protocol_versions(&[&TLS13])
                .unwrap()
                .with_root_certificates(root_cert_store)
                .with_single_cert(certs, keys.remove(0))
                .expect("bad certificate/key")
        } else {
            ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(root_cert_store)
                .with_no_client_auth()
        };
        Ok(TlsClientConfig { client_config: cc })
    }

    async fn load_tls_private_key(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_key(
            config,
            TLS_CLIENT_PRIVATE_KEY_RAW,
            TLS_CLIENT_PRIVATE_KEY_FILE,
        )
        .await
    }

    async fn load_tls_certificate(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_certificate(
            config,
            TLS_CLIENT_CERTIFICATE_RAW,
            TLS_CLIENT_CERTIFICATE_FILE,
        )
        .await
    }
}

async fn load_tls_key(
    config: &Config<'_>,
    tls_private_key_raw_config_key: &str,
    tls_private_key_file_config_key: &str,
) -> ZResult<Vec<u8>> {
    if let Some(value) = config.get(tls_private_key_raw_config_key) {
        return Ok(value.as_bytes().to_vec());
    } else if let Some(value) = config.get(tls_private_key_file_config_key) {
        return Ok(fs::read(value)
            .await
            .map_err(|e| zerror!("Invalid TLS private key file: {}", e))?)
        .and_then(|result| {
            if result.is_empty() {
                Err(zerror!("Empty TLS key.").into())
            } else {
                Ok(result)
            }
        });
    }
    Err(zerror!("Missing TLS private key.").into())
}

async fn load_tls_certificate(
    config: &Config<'_>,
    tls_certificate_raw_config_key: &str,
    tls_certificate_file_config_key: &str,
) -> ZResult<Vec<u8>> {
    if let Some(value) = config.get(tls_certificate_raw_config_key) {
        return Ok(value.as_bytes().to_vec());
    } else if let Some(value) = config.get(tls_certificate_file_config_key) {
        return Ok(fs::read(value)
            .await
            .map_err(|e| zerror!("Invalid TLS certificate file: {}", e))?);
    }
    Err(zerror!("Missing tls certificates.").into())
}

fn load_trust_anchors(config: &Config<'_>) -> ZResult<Option<RootCertStore>> {
    let mut root_cert_store = RootCertStore::empty();
    if let Some(value) = config.get(TLS_ROOT_CA_CERTIFICATE_RAW) {
        let mut pem = BufReader::new(value.as_bytes());
        let certs = rustls_pemfile::certs(&mut pem)?;
        let trust_anchors = certs.iter().map(|cert| {
            let ta = TrustAnchor::try_from_cert_der(&cert[..]).unwrap();
            OwnedTrustAnchor::from_subject_spki_name_constraints(
                ta.subject,
                ta.spki,
                ta.name_constraints,
            )
        });
        root_cert_store.add_server_trust_anchors(trust_anchors.into_iter());
        return Ok(Some(root_cert_store));
    }
    if let Some(filename) = config.get(TLS_ROOT_CA_CERTIFICATE_FILE) {
        let mut pem = BufReader::new(File::open(filename)?);
        let certs = rustls_pemfile::certs(&mut pem)?;
        let trust_anchors = certs.iter().map(|cert| {
            let ta = TrustAnchor::try_from_cert_der(&cert[..]).unwrap();
            OwnedTrustAnchor::from_subject_spki_name_constraints(
                ta.subject,
                ta.spki,
                ta.name_constraints,
            )
        });
        root_cert_store.add_server_trust_anchors(trust_anchors.into_iter());
        return Ok(Some(root_cert_store));
    }
    Ok(None)
}

fn load_default_webpki_certs() -> RootCertStore {
    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    }));
    root_cert_store
}
