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
use std::{
    cell::UnsafeCell,
    convert::TryInto,
    fmt::{self, Debug},
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use time::OffsetDateTime;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex as AsyncMutex,
};
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};
use tokio_util::sync::CancellationToken;
use x509_parser::prelude::{FromDer, X509Certificate};
use zenoh_core::{bail, zasynclock};
use zenoh_link_commons::{
    get_ip_interface_names,
    tls::expiration::{LinkCertExpirationManager, LinkWithCertExpiration},
    LinkAuthId, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, ListenersUnicastIP,
    NewLinkChannelSender, BIND_INTERFACE, BIND_SOCKET,
};
use zenoh_protocol::{
    core::{EndPoint, Locator},
    transport::BatchSize,
};
use zenoh_result::{zerror, ZResult};

use crate::{
    utils::{get_tls_addr, get_tls_host, get_tls_server_name, TlsClientConfig, TlsServerConfig},
    TLS_ACCEPT_THROTTLE_TIME, TLS_DEFAULT_MTU, TLS_LINGER_TIMEOUT, TLS_LOCATOR_PREFIX,
};

#[derive(Default, Debug, PartialEq, Eq, Hash)]
pub struct TlsCommonName(String);

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
    auth_identifier: LinkAuthId,
    mtu: BatchSize,
    expiration_manager: Option<LinkCertExpirationManager>,
}

unsafe impl Send for LinkUnicastTls {}
unsafe impl Sync for LinkUnicastTls {}

impl LinkUnicastTls {
    fn new(
        socket: TlsStream<TcpStream>,
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
        auth_identifier: LinkAuthId,
        expiration_manager: Option<LinkCertExpirationManager>,
    ) -> LinkUnicastTls {
        let (tcp_stream, _) = socket.get_ref();
        // Set the TLS nodelay option
        if let Err(err) = tcp_stream.set_nodelay(true) {
            tracing::warn!(
                "Unable to set NODEALY option on TLS link {} => {}: {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Set the TLS linger option
        if let Err(err) = tcp_stream.set_linger(Some(Duration::from_secs(
            (*TLS_LINGER_TIMEOUT).try_into().unwrap(),
        ))) {
            tracing::warn!(
                "Unable to set LINGER option on TLS link {} => {}: {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Compute the MTU
        // See IETF RFC6691: https://datatracker.ietf.org/doc/rfc6691/
        let header = match src_addr.ip() {
            std::net::IpAddr::V4(_) => 40,
            std::net::IpAddr::V6(_) => 60,
        };
        #[allow(unused_mut)] // mut is not needed when target_family != unix
        let mut mtu = *TLS_DEFAULT_MTU - header;

        // target limitation of socket2: https://docs.rs/socket2/latest/src/socket2/sys/unix.rs.html#1544
        #[cfg(target_family = "unix")]
        {
            let socket = socket2::SockRef::from(&tcp_stream);
            // Get the MSS and divide it by 2 to ensure we can at least fill half the MSS
            let mss = socket.mss().unwrap_or(mtu as u32) / 2;
            // Compute largest multiple of TCP MSS that is smaller of default MTU
            let mut tgt = mss;
            while (tgt + mss) < mtu as u32 {
                tgt += mss;
            }
            mtu = (mtu as u32).min(tgt) as BatchSize;
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
            auth_identifier,
            mtu,
            expiration_manager,
        }
    }

    // NOTE: It is safe to suppress Clippy warning since no concurrent reads
    //       or concurrent writes will ever happen. The read_mtx and write_mtx
    //       are respectively acquired in any read and write operation.
    #[allow(clippy::mut_from_ref)]
    fn get_mut_socket(&self) -> &mut TlsStream<TcpStream> {
        unsafe { &mut *self.inner.get() }
    }

    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing TLS link: {}", self);
        // Flush the TLS stream
        let _guard = zasynclock!(self.write_mtx);
        let tls_stream = self.get_mut_socket();
        let res = tls_stream.flush().await;
        tracing::trace!("TLS link flush {}: {:?}", self, res);
        // Close the underlying TCP stream
        let (tcp_stream, _) = tls_stream.get_mut();
        let res = tcp_stream.shutdown().await;
        tracing::trace!("TLS link shutdown {}: {:?}", self, res);
        res.map_err(|e| zerror!(e).into())
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastTls {
    async fn close(&self) -> ZResult<()> {
        if let Some(expiration_manager) = &self.expiration_manager {
            if !expiration_manager.set_closing() {
                // expiration_task is closing link, return its returned ZResult to Transport
                return expiration_manager.wait_for_expiration_task().await;
            }
            // cancel the expiration task and close link
            expiration_manager.cancel_expiration_task();
            let res = self.close().await;
            let _ = expiration_manager.wait_for_expiration_task().await;
            return res;
        }
        self.close().await
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        let _guard = zasynclock!(self.write_mtx);
        self.get_mut_socket().write(buffer).await.map_err(|e| {
            tracing::trace!("Write error on TLS link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let _guard = zasynclock!(self.write_mtx);
        self.get_mut_socket().write_all(buffer).await.map_err(|e| {
            tracing::trace!("Write error on TLS link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let _guard = zasynclock!(self.read_mtx);
        self.get_mut_socket().read(buffer).await.map_err(|e| {
            tracing::trace!("Read error on TLS link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let _guard = zasynclock!(self.read_mtx);
        let _ = self
            .get_mut_socket()
            .read_exact(buffer)
            .await
            .map_err(|e| {
                tracing::trace!("Read error on TLS link {}: {}", self, e);
                zerror!(e)
            })?;
        Ok(())
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
    fn get_mtu(&self) -> BatchSize {
        self.mtu
    }

    #[inline(always)]
    fn get_interface_names(&self) -> Vec<String> {
        get_ip_interface_names(&self.src_addr)
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        super::IS_RELIABLE
    }

    #[inline(always)]
    fn is_streamed(&self) -> bool {
        true
    }

    #[inline(always)]
    fn get_auth_id(&self) -> &LinkAuthId {
        &self.auth_identifier
    }
}

#[async_trait]
impl LinkWithCertExpiration for LinkUnicastTls {
    async fn expire(&self) -> ZResult<()> {
        let expiration_manager = self
            .expiration_manager
            .as_ref()
            .expect("expiration_manager should be set");
        if expiration_manager.set_closing() {
            return self.close().await;
        }
        // Transport is already closing the link
        Ok(())
    }
}

impl Drop for LinkUnicastTls {
    fn drop(&mut self) {
        // Close the underlying TCP stream
        let (tcp_stream, _) = self.get_mut_socket().get_mut();
        let _ = zenoh_runtime::ZRuntime::Acceptor
            .block_in_place(async move { tcp_stream.shutdown().await });
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

pub struct LinkManagerUnicastTls {
    manager: NewLinkChannelSender,
    listeners: ListenersUnicastIP,
}

impl LinkManagerUnicastTls {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: ListenersUnicastIP::new(),
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

        // if both `iface`, and `bind` are present, return error
        if let (Some(_), Some(_)) = (epconf.get(BIND_INTERFACE), epconf.get(BIND_SOCKET)) {
            bail!(
                "Using Config options `iface` and `bind` in conjunction is unsupported at this time {} {:?}",
                BIND_INTERFACE,
                BIND_SOCKET
            )
        }

        // Initialize the TLS Config
        let client_config = TlsClientConfig::new(&epconf)
            .await
            .map_err(|e| zerror!("Cannot create a new TLS listener to {endpoint}: {e}"))?;
        let config = Arc::new(client_config.client_config);
        let connector = TlsConnector::from(config);

        // Initialize the TcpStream
        let (tcp_stream, src_addr, dst_addr) = client_config
            .tcp_socket_config
            .new_link(&addr)
            .await
            .map_err(|e| {
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

        let (_, tls_conn) = tls_stream.get_ref();
        let auth_identifier = get_server_cert_common_name(tls_conn)?;
        let certchain_expiration_time = get_cert_chain_expiration(&tls_conn.peer_certificates())?
            .expect("server should have certificate chain");

        let tls_stream = TlsStream::Client(tls_stream);

        let link = Arc::<LinkUnicastTls>::new_cyclic(|weak_link| {
            let mut expiration_manager = None;
            if client_config.tls_close_link_on_expiration {
                // setup expiration manager
                expiration_manager = Some(LinkCertExpirationManager::new(
                    weak_link.clone(),
                    src_addr,
                    dst_addr,
                    TLS_LOCATOR_PREFIX,
                    certchain_expiration_time,
                ))
            }
            LinkUnicastTls::new(
                tls_stream,
                src_addr,
                dst_addr,
                auth_identifier.into(),
                expiration_manager,
            )
        });

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
        let (socket, local_addr) = tls_server_config
            .tcp_socket_config
            .new_listener(&addr)
            .map_err(|e| zerror!("Can not create a new TLS listener on {}: {}", addr, e))?;

        let local_port = local_addr.port();

        // Initialize the TlsAcceptor
        let token = self.listeners.token.child_token();

        let task = {
            let acceptor = TlsAcceptor::from(Arc::new(tls_server_config.server_config));
            let token = token.clone();
            let manager = self.manager.clone();

            async move {
                accept_task(
                    socket,
                    acceptor,
                    token,
                    manager,
                    tls_server_config.tls_handshake_timeout,
                    tls_server_config.tls_close_link_on_expiration,
                )
                .await
            }
        };

        // Update the endpoint locator address
        let locator = Locator::new(
            endpoint.protocol(),
            format!("{host}:{local_port}"),
            endpoint.metadata(),
        )?;
        let endpoint = EndPoint::new(
            locator.protocol(),
            locator.address(),
            locator.metadata(),
            endpoint.config(),
        )?;
        self.listeners
            .add_listener(endpoint, local_addr, task, token)
            .await?;

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let epaddr = endpoint.address();
        let addr = get_tls_addr(&epaddr).await?;
        self.listeners.del_listener(addr).await
    }

    async fn get_listeners(&self) -> Vec<EndPoint> {
        self.listeners.get_endpoints()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        self.listeners.get_locators()
    }
}

async fn accept_task(
    socket: TcpListener,
    acceptor: TlsAcceptor,
    token: CancellationToken,
    manager: NewLinkChannelSender,
    tls_handshake_timeout: Duration,
    tls_close_link_on_expiration: bool,
) -> ZResult<()> {
    let src_addr = socket.local_addr().map_err(|e| {
        let e = zerror!("Can not accept TLS connections: {}", e);
        tracing::warn!("{}", e);
        e
    })?;

    let mut listener = tls_listener::builder(acceptor)
        .handshake_timeout(tls_handshake_timeout)
        .listen(socket);

    tracing::trace!("Ready to accept TLS connections on: {:?}", src_addr);
    loop {
        tokio::select! {
            _ = token.cancelled() => break,

            res = listener.accept() => {
                match res {
                    Ok((tls_stream, dst_addr)) => {
                        let (tcp_stream, tls_conn) = tls_stream.get_ref();
                        let src_addr =  match tcp_stream.local_addr()  {
                            Ok(sa) => sa,
                            Err(e) => {
                                tracing::debug!("Can not accept TLS connection: {}", e);
                                continue;
                            }
                        };
                        let auth_identifier = get_client_cert_common_name(tls_conn)?;

                        // Get certificate chain expiration
                        let mut maybe_expiration_time = None;
                        if tls_close_link_on_expiration {
                            match get_cert_chain_expiration(&tls_conn.peer_certificates())? {
                                exp @ Some(_) => maybe_expiration_time = exp,
                                None => tracing::warn!(
                                    "Cannot monitor expiration for TLS link {:?} => {:?}: client does not have certificates",
                                    src_addr,
                                    dst_addr,
                                ),
                            }
                        }

                        tracing::debug!("Accepted TLS connection on {:?}: {:?}. {:?}.", src_addr, dst_addr, auth_identifier);
                        // Create the new link object
                        let link = Arc::<LinkUnicastTls>::new_cyclic(|weak_link| {
                            let mut expiration_manager = None;
                            if let Some(certchain_expiration_time) = maybe_expiration_time {
                                // setup expiration manager
                                expiration_manager = Some(LinkCertExpirationManager::new(
                                    weak_link.clone(),
                                    src_addr,
                                    dst_addr,
                                    TLS_LOCATOR_PREFIX,
                                    certchain_expiration_time,
                                ));
                            }
                            LinkUnicastTls::new(
                                tokio_rustls::TlsStream::Server(tls_stream),
                                src_addr,
                                dst_addr,
                                auth_identifier.into(),
                                expiration_manager,
                            )
                        });

                        // Communicate the new link to the initial transport manager
                        if let Err(e) = manager.send_async(LinkUnicast(link)).await {
                            tracing::error!("{}-{}: {}", file!(), line!(), e)
                        }
                    }
                    Err(e) => {
                        tracing::warn!("{}. Hint: increase the system open file limit.", e);
                        // Throttle the accept loop upon an error
                        // NOTE: This might be due to various factors. However, the most common case is that
                        //       the process has reached the maximum number of open files in the system. On
                        //       Linux systems this limit can be changed by using the "ulimit" command line
                        //       tool. In case of systemd-based systems, this can be changed by using the
                        //       "sysctl" command line tool.
                        tokio::time::sleep(Duration::from_micros(*TLS_ACCEPT_THROTTLE_TIME)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

fn get_client_cert_common_name(tls_conn: &rustls::CommonState) -> ZResult<TlsAuthId> {
    if let Some(serv_certs) = tls_conn.peer_certificates() {
        let (_, cert) = X509Certificate::from_der(serv_certs[0].as_ref())?;
        let subject_name = &cert
            .subject
            .iter_common_name()
            .next()
            .and_then(|cn| cn.as_str().ok())
            .unwrap();

        Ok(TlsAuthId {
            auth_value: Some(subject_name.to_string()),
        })
    } else {
        Ok(TlsAuthId { auth_value: None })
    }
}

fn get_server_cert_common_name(tls_conn: &rustls::ClientConnection) -> ZResult<TlsAuthId> {
    let serv_certs = tls_conn.peer_certificates().unwrap();
    let mut auth_id = TlsAuthId { auth_value: None };

    // Need the first certificate in the chain so no need for looping
    if let Some(item) = serv_certs.iter().next() {
        let (_, cert) = X509Certificate::from_der(item.as_ref())?;
        let subject_name = &cert
            .subject
            .iter_common_name()
            .next()
            .and_then(|cn| cn.as_str().ok())
            .unwrap();

        auth_id = TlsAuthId {
            auth_value: Some(subject_name.to_string()),
        };
        return Ok(auth_id);
    }
    Ok(auth_id)
}

/// Returns the minimum value of the `not_after` field in the given certificate chain.
/// Returns `None` if the input certificate chain is empty or `None`
fn get_cert_chain_expiration(
    cert_chain: &Option<&[rustls_pki_types::CertificateDer]>,
) -> ZResult<Option<OffsetDateTime>> {
    let mut link_expiration: Option<OffsetDateTime> = None;
    if let Some(remote_certs) = cert_chain {
        for cert in *remote_certs {
            let (_, cert) = X509Certificate::from_der(cert.as_ref())?;
            let cert_expiration = cert.validity().not_after.to_datetime();
            link_expiration = link_expiration
                .map(|current_min| current_min.min(cert_expiration))
                .or(Some(cert_expiration));
        }
    }
    Ok(link_expiration)
}

struct TlsAuthId {
    auth_value: Option<String>,
}

impl Debug for TlsAuthId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Common Name: {}",
            self.auth_value.as_deref().unwrap_or("None")
        )
    }
}

impl From<TlsAuthId> for LinkAuthId {
    fn from(value: TlsAuthId) -> Self {
        LinkAuthId::Tls(value.auth_value.clone())
    }
}
