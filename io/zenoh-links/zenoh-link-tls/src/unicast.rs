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
#[cfg(all(feature = "uring", target_os = "linux"))]
use std::os::fd::RawFd;
use std::{
    convert::TryInto,
    fmt::{self, Debug},
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use time::OffsetDateTime;
use tokio::{
    io::{split, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
    sync::Mutex as AsyncMutex,
};
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};
use tokio_util::sync::CancellationToken;
use x509_parser::prelude::{FromDer, X509Certificate};
use zenoh_core::{bail, zasynclock};
use zenoh_link_commons::{
    get_ip_interface_names,
    tcp::get_tcp_addrs,
    tls::expiration::{LinkCertExpirationManager, LinkWithCertExpiration},
    LinkAuthId, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, ListenersUnicastIP,
    NewLinkChannelSender, BIND_INTERFACE, BIND_SOCKET,
};
use zenoh_protocol::{
    core::{EndPoint, Locator, Priority},
    transport::BatchSize,
};
use zenoh_result::{zerror, ZResult};

use crate::{
    utils::{get_tls_addr, get_tls_host, get_tls_server_name, TlsClientConfig, TlsServerConfig},
    TLS_ACCEPT_THROTTLE_TIME, TLS_CLOSE_TIMEOUT, TLS_DEFAULT_MTU, TLS_LINGER_TIMEOUT,
    TLS_LOCATOR_PREFIX,
};

#[derive(Default, Debug, PartialEq, Eq, Hash)]
pub struct TlsCommonName(String);

pub struct LinkUnicastTls {
    // rustls::Connection is one object for both directions. A read
    // (`process_new_packets`) takes that object out and puts
    // `HandshakeNotComplete` in its place while it runs, and it may also append
    // to the send queue (TLS 1.3 key updates, alerts). A write on another
    // thread observes or frees that same state. rustls does not support this
    // (rustls/rustls#288).
    //
    // `tokio::io::split` locks the stream only for the duration of each poll, so
    // a blocked read does not hold rustls across the await, and a read poll and
    // a write poll never enter it together. The per-half mutexes still stop one
    // `read_exact` / `write_all` from interleaving with another on the same half.
    reader: AsyncMutex<ReadHalf<TlsStream<TcpStream>>>,
    writer: AsyncMutex<WriteHalf<TlsStream<TcpStream>>>,
    // A second handle to the TCP socket, because the halves do not expose it.
    // `close` uses it to shut down and reset the connection when the close times out.
    abort_socket: socket2::Socket,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    src_locator: Locator,
    // The destination socket address of this link (address used on the local host)
    dst_addr: SocketAddr,
    dst_locator: Locator,
    auth_identifier: LinkAuthId,
    mtu: BatchSize,
    expiration_manager: Option<LinkCertExpirationManager>,
}

impl LinkUnicastTls {
    fn new(
        socket: TlsStream<TcpStream>,
        abort_socket: socket2::Socket,
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
        #[allow(deprecated)]
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

        // Split before any task can read and write. TCP options above must be
        // applied first: the halves do not expose the TcpStream.
        let (reader, writer) = split(socket);

        // Build the Tls object
        LinkUnicastTls {
            reader: AsyncMutex::new(reader),
            writer: AsyncMutex::new(writer),
            abort_socket,
            src_addr,
            src_locator: Locator::new(TLS_LOCATOR_PREFIX, src_addr.to_string(), "").unwrap(),
            dst_addr,
            dst_locator: Locator::new(TLS_LOCATOR_PREFIX, dst_addr.to_string(), "").unwrap(),
            auth_identifier,
            mtu,
            expiration_manager,
        }
    }

    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing TLS link: {}", self);
        let close = async {
            let mut writer = zasynclock!(self.writer);
            let res = writer.flush().await;
            tracing::trace!("TLS link flush {}: {:?}", self, res);
            // close_notify, then the underlying TCP shutdown.
            writer.shutdown().await
        };
        // The flush and the shutdown wait until the kernel takes all pending TLS
        // records. A peer that is gone or does not read can make this take minutes.
        let timeout = Duration::from_millis(*TLS_CLOSE_TIMEOUT);
        let Ok(res) = tokio::time::timeout(timeout, close).await else {
            // A linger of 0 makes the last close of the socket reset the connection
            // instead of waiting for the pending data.
            if let Err(e) = self.abort_socket.set_linger(Some(Duration::ZERO)) {
                tracing::warn!("Unable to reset TLS link {}: {}", self, e);
            }
            // The reset happens only when the link drops. The shutdown makes the reads and
            // writes of the link fail now, so that its owner drops it.
            if let Err(e) = self.abort_socket.shutdown(std::net::Shutdown::Both) {
                tracing::warn!("Unable to shut down TLS link {}: {}", self, e);
            }
            bail!("TLS link {} close timed out after {:?}", self, timeout);
        };
        tracing::trace!("TLS link shutdown {}: {:?}", self, res);
        res.map_err(|e| zerror!(e).into())
    }
}

fn abort_socket(socket: &TlsStream<TcpStream>) -> std::io::Result<socket2::Socket> {
    socket2::SockRef::from(socket.get_ref().0).try_clone()
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

    async fn write(&self, buffer: &[u8], _priority: Option<Priority>) -> ZResult<usize> {
        let mut writer = zasynclock!(self.writer);
        writer.write(buffer).await.map_err(|e| {
            tracing::trace!("Write error on TLS link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn write_all(&self, buffer: &[u8], _priority: Option<Priority>) -> ZResult<()> {
        let mut writer = zasynclock!(self.writer);
        if let Err(e) = writer.write_all(buffer).await {
            tracing::trace!("Write error on TLS link {}: {}", self, e);
            return Err(zerror!(e).into());
        }
        // tokio-rustls can accept plaintext and still leave ciphertext buffered
        // once the socket is full. The peer cannot read those bytes until flush.
        if let Err(e) = writer.flush().await {
            tracing::trace!("Flush error on TLS link {}: {}", self, e);
            return Err(zerror!(e).into());
        }
        Ok(())
    }

    async fn read(&self, buffer: &mut [u8], _priority: Option<Priority>) -> ZResult<usize> {
        let mut reader = zasynclock!(self.reader);
        reader.read(buffer).await.map_err(|e| {
            tracing::trace!("Read error on TLS link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn read_exact(&self, buffer: &mut [u8], _priority: Option<Priority>) -> ZResult<()> {
        let mut reader = zasynclock!(self.reader);
        reader.read_exact(buffer).await.map(|_| ()).map_err(|e| {
            tracing::trace!("Read error on TLS link {}: {}", self, e);
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

    #[cfg(all(feature = "uring", target_os = "linux"))]
    fn get_fd(&self) -> ZResult<RawFd> {
        bail!("Correct FD unavailable for TLS extension")
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

impl fmt::Debug for LinkManagerUnicastTls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkManagerUnicastTls")
            .field("manager", &self.manager)
            .field("listeners", &self.listeners)
            .finish()
    }
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
        let addrs = get_tcp_addrs(endpoint.address()).await?;

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
            .new_link_first_reachable(addrs)
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
        let abort_socket = abort_socket(&tls_stream).map_err(|e| {
            zerror!(
                "Can not create a new TLS link bound to {:?}: {}",
                server_name,
                e
            )
        })?;

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
                abort_socket,
                src_addr,
                dst_addr,
                auth_identifier.into(),
                expiration_manager,
            )
        });

        Ok(LinkUnicast::from(link as Arc<dyn LinkUnicastTrait>))
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

    /// See [`zenoh_link_commons::ListenersUnicastIP::get_locators`].
    async fn get_locators(&self) -> Vec<Locator> {
        self.listeners.get_locators()
    }

    /// See [`zenoh_link_commons::ListenersUnicastIP::get_locators_noloopback`].
    async fn get_locators_noloopback(&self) -> Vec<Locator> {
        self.listeners.get_locators_noloopback()
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
                        let auth_identifier = match get_client_cert_common_name(tls_conn) {
                            Ok(auth_id) => auth_id,
                            Err(e) => {
                                tracing::warn!("Error getting client cert common name: {e}");
                                continue;
                            }
                        };

                        // Get certificate chain expiration
                        let mut maybe_expiration_time = None;
                        if tls_close_link_on_expiration {
                            let exp = match get_cert_chain_expiration(&tls_conn.peer_certificates()) {
                                Ok(exp) => exp,
                                Err(e) => {
                                    tracing::warn!("Error getting client cert expiration: {e}");
                                    continue;
                                }
                            };
                            match exp {
                                exp @ Some(_) => maybe_expiration_time = exp,
                                None => tracing::warn!(
                                    "Cannot monitor expiration for TLS link {:?} => {:?}: client does not have certificates",
                                    src_addr,
                                    dst_addr,
                                ),
                            }
                        }

                        tracing::debug!("Accepted TLS connection on {:?}: {:?}. {:?}.", src_addr, dst_addr, auth_identifier);
                        let tls_stream = tokio_rustls::TlsStream::Server(tls_stream);
                        let abort_socket = match abort_socket(&tls_stream) {
                            Ok(abort_socket) => abort_socket,
                            Err(e) => {
                                tracing::warn!("Can not create TLS link {:?} => {:?}: {}", src_addr, dst_addr, e);
                                continue;
                            }
                        };
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
                                tls_stream,
                                abort_socket,
                                src_addr,
                                dst_addr,
                                auth_identifier.into(),
                                expiration_manager,
                            )
                        });

                        // Communicate the new link to the initial transport manager
                        if let Err(e) = manager
                            .send_async(LinkUnicast::from(link as Arc<dyn LinkUnicastTrait>))
                            .await
                        {
                            tracing::error!("{}-{}: {}", file!(), line!(), e)
                        }
                    }
                    // A failed handshake concerns one client only. A sleep here would also stop the
                    // handshakes of all other clients, because they make progress only in accept().
                    Err(tls_listener::Error::TlsAcceptError { error, peer_addr, .. }) => {
                        tracing::warn!("TLS handshake with {} failed: {}", peer_addr, error);
                    }
                    Err(tls_listener::Error::HandshakeTimeout { peer_addr, .. }) => {
                        tracing::warn!("TLS handshake with {} timed out", peer_addr);
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
    if let Some(client_certs) = tls_conn.peer_certificates() {
        let (_, cert) = X509Certificate::from_der(client_certs[0].as_ref())?;
        let subject_name = &cert
            .subject
            .iter_common_name()
            .next()
            .and_then(|cn| cn.as_str().ok());

        Ok(TlsAuthId {
            auth_value: subject_name.map(|cn| cn.to_string()),
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
            .and_then(|cn| cn.as_str().ok());

        auth_id = TlsAuthId {
            auth_value: subject_name.map(|cn| cn.to_string()),
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

#[cfg(test)]
#[path = "concurrent_io_tests.rs"]
mod concurrent_io_tests;
