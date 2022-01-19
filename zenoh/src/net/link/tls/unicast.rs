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
use super::config::*;
use super::*;
use crate::net::transport::TransportManager;
use async_rustls::rustls::internal::pemfile;
pub use async_rustls::rustls::*;
pub use async_rustls::webpki::*;
use async_rustls::{TlsAcceptor, TlsConnector, TlsStream};
use async_std::fs;
use async_std::net::{SocketAddr, TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::sync::Mutex as AsyncMutex;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::io::Cursor;
use std::net::Shutdown;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_core::Result as ZResult;
use zenoh_core::{zasynclock, zerror, zread, zwrite};
use zenoh_util::sync::Signal;

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
    // The destination socket address of this link (address used on the local host)
    dst_addr: SocketAddr,
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
                "Unable to set NODEALY option on TLS link {} => {} : {}",
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
                "Unable to set LINGER option on TLS link {} => {} : {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Build the Tls object
        LinkUnicastTls {
            inner: UnsafeCell::new(socket),
            src_addr,
            dst_addr,
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
    fn get_src(&self) -> Locator {
        Locator {
            address: LocatorAddress::Tls(LocatorTls::SocketAddr(self.src_addr)),
            metadata: None,
        }
    }

    #[inline(always)]
    fn get_dst(&self) -> Locator {
        Locator {
            address: LocatorAddress::Tls(LocatorTls::SocketAddr(self.dst_addr)),
            metadata: None,
        }
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
    locator: Locator,
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastTls {
    fn new(
        endpoint: EndPoint,
        locator: Locator,
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> ListenerUnicastTls {
        ListenerUnicastTls {
            endpoint,
            locator,
            active,
            signal,
            handle,
        }
    }
}

pub struct LinkManagerUnicastTls {
    manager: TransportManager,
    listeners: Arc<RwLock<HashMap<SocketAddr, ListenerUnicastTls>>>,
}

impl LinkManagerUnicastTls {
    pub(crate) fn new(manager: TransportManager) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastTls {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let domain = get_tls_dns(&endpoint.locator.address).await?;
        let addr = get_tls_addr(&endpoint.locator.address).await?;
        let host: &str = domain.as_ref().into();

        // Initialize the TcpStream
        let tcp_stream = TcpStream::connect(addr)
            .await
            .map_err(|e| zerror!("Can not create a new TLS link bound to {}: {}", host, e))?;

        let src_addr = tcp_stream
            .local_addr()
            .map_err(|e| zerror!("Can not create a new TLS link bound to {}: {}", host, e))?;

        let dst_addr = tcp_stream
            .peer_addr()
            .map_err(|e| zerror!("Can not create a new TLS link bound to {}: {}", host, e))?;

        // Initialize the TLS stream
        let bytes = match endpoint.config.as_ref() {
            Some(config) => match config.get(TLS_ROOT_CA_CERTIFICATE_RAW) {
                Some(tls_ca_certificate) => tls_ca_certificate.as_bytes().to_vec(),
                None => match config.get(TLS_ROOT_CA_CERTIFICATE_FILE) {
                    Some(tls_ca_certificate) => fs::read(tls_ca_certificate)
                        .await
                        .map_err(|e| zerror!("Invalid TLS CA certificate file: {}", e))?,
                    None => vec![],
                },
            },
            None => vec![],
        };

        let config = if bytes.is_empty() {
            Arc::new(ClientConfig::new())
        } else {
            let mut cc = ClientConfig::new();
            let _ = cc
                .root_store
                .add_pem_file(&mut Cursor::new(&bytes))
                .map_err(|_| zerror!("Invalid TLS CA certificate file"))?;
            Arc::new(cc)
        };

        let connector = TlsConnector::from(config);
        let tls_stream = connector
            .connect(domain.as_ref(), tcp_stream)
            .await
            .map_err(|e| zerror!("Can not create a new TLS link bound to {}: {}", host, e))?;
        let tls_stream = TlsStream::Client(tls_stream);

        let link = Arc::new(LinkUnicastTls::new(tls_stream, src_addr, dst_addr));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
        let addr = get_tls_addr(&endpoint.locator.address).await?;
        let host = get_tls_host(&endpoint.locator.address)?;

        // Verify there is a valid ServerConfig
        let config = endpoint.config.as_ref().ok_or_else(|| {
            zerror!(
                "Can not create a new TLS listener on {}: no ServerConfig provided",
                addr
            )
        })?;

        // Configure the server private key
        let bytes = match config.get(TLS_SERVER_PRIVATE_KEY_RAW) {
            Some(tls_server_private_key) => tls_server_private_key.as_bytes().to_vec(),
            None => match config.get(TLS_SERVER_PRIVATE_KEY_FILE) {
                Some(tls_server_private_key) => fs::read(tls_server_private_key)
                    .await
                    .map_err(|e| zerror!("Invalid TLS private key file: {}", e))?,
                None => {
                    bail!(
                        "Can not create a new TLS listener on {}. ServerConfig not provided: {}.",
                        addr,
                        TLS_SERVER_PRIVATE_KEY_FILE
                    );
                }
            },
        };
        let mut keys = pemfile::rsa_private_keys(&mut Cursor::new(bytes.as_slice())).unwrap();

        // Configure the server certificate
        let bytes = match config.get(TLS_SERVER_CERTIFICATE_RAW) {
            Some(tls_server_certificate) => tls_server_certificate.as_bytes().to_vec(),
            None => match config.get(TLS_SERVER_CERTIFICATE_FILE) {
                Some(tls_server_certificate) => fs::read(tls_server_certificate)
                    .await
                    .map_err(|e| zerror!("Invalid TLS server certificate file: {}", e))?,
                None => {
                    bail!(
                        "Can not create a new TLS listener on {}. ServerConfig not provided: {}.",
                        addr,
                        TLS_SERVER_CERTIFICATE_FILE
                    )
                }
            },
        };
        let certs = pemfile::certs(&mut Cursor::new(bytes.as_slice())).unwrap();

        // Configure the client authentication
        let client_auth: bool = match config.get(TLS_CLIENT_AUTH) {
            Some(ca) => ca,
            None => TLS_CLIENT_AUTH_DEFAULT,
        }
        .parse()
        .map_err(|e| zerror!("Invalid {} value: {}", TLS_CLIENT_AUTH, e))?;

        let mut sc = if client_auth {
            // @TODO: implement Client authentication
            bail!(
                "Can not create a new TLS listener on {}. ClientAuth not supported.",
                addr
            );
        } else {
            ServerConfig::new(NoClientAuth::new())
        };

        sc.set_single_cert(certs, keys.remove(0)).unwrap();

        // Initialize the TcpListener
        let socket = TcpListener::bind(addr)
            .await
            .map_err(|e| zerror!("Can not create a new TLS listener on {}: {}", addr, e))?;

        let local_addr = socket
            .local_addr()
            .map_err(|e| zerror!("Can not create a new TLS listener on {}: {}", addr, e))?;
        let local_port = local_addr.port();

        // Initialize the TlsAcceptor
        let acceptor = TlsAcceptor::from(Arc::new(sc));
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
        let locator = Locator {
            address: LocatorAddress::Tls(LocatorTls::DnsName(format!("{}:{}", host, local_port))),
            metadata: endpoint.locator.metadata.clone(),
        };

        let listener = ListenerUnicastTls::new(endpoint, locator.clone(), active, signal, handle);
        // Update the list of active listeners on the manager
        zwrite!(self.listeners).insert(local_addr, listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let addr = get_tls_addr(&endpoint.locator.address).await?;

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
        zread!(self.listeners)
            .values()
            .map(|x| x.locator.clone())
            .collect()
    }
}

async fn accept_task(
    socket: TcpListener,
    acceptor: TlsAcceptor,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: TransportManager,
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
                let e = format!("Can not accept TLS connection: {}", e);
                log::warn!("{}", e);
                continue;
            }
        };

        log::debug!("Accepted TLS connection on {:?}: {:?}", src_addr, dst_addr);
        // Create the new link object
        let link = Arc::new(LinkUnicastTls::new(tls_stream, src_addr, dst_addr));

        // Communicate the new link to the initial transport manager
        manager.handle_new_link_unicast(LinkUnicast(link)).await;
    }

    Ok(())
}
