//
// Copyright (c) 2026 ZettaScale Technology
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
    collections::HashMap,
    future::{Future, IntoFuture},
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
};

use futures::FutureExt;
use quinn::{
    crypto::rustls::{HandshakeData, QuicClientConfig, QuicServerConfig},
    EndpointConfig,
};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zenoh_config::{EndPoint, Locator};
use zenoh_core::zerror;
use zenoh_protocol::core::{Metadata, Priority};
use zenoh_result::ZResult;

use crate::{
    quic::{
        get_quic_addr, get_quic_host,
        plaintext::{PlainTextClientConfig, PlainTextServerConfig},
        socket::QuicSocketConfig,
        QuicMtuConfig, QuicTransportConfigurator, TlsClientConfig, TlsServerConfig,
        PROTOCOL_LEGACY, PROTOCOL_MULTI_STREAM, PROTOCOL_SINGLE_STREAM,
    },
    LinkUnicast, LinkUnicastTrait, NewLinkChannelSender,
};

/// Quic endpoint `multistream` config
pub(crate) enum MultiStreamConfig {
    /// `multistream=false`
    Disabled,
    /// `multistream=true`
    Enabled,
    /// default, or `multistream=auto`
    Auto,
}

impl MultiStreamConfig {
    /// Parse multistream configuration.
    fn new(metadata: Metadata) -> ZResult<Self> {
        let multistream = metadata.get(Metadata::MULTISTREAM).unwrap_or("auto");
        if multistream == "auto" {
            return Ok(Self::Auto);
        }
        if multistream
            .parse()
            .map_err(|_| zerror!("Invalid multistream config:  {multistream}"))?
        {
            Ok(Self::Enabled)
        } else {
            Ok(Self::Disabled)
        }
    }

    /// Returns the list of protocols supported for QUIC ALPN.
    ///
    /// Protocols are ordered by decreasing selection priority.
    fn alpn_protocols(&self) -> Vec<Vec<u8>> {
        match self {
            // Disabled must be compatible with legacy protocol
            Self::Disabled => vec![PROTOCOL_SINGLE_STREAM.into(), PROTOCOL_LEGACY.into()],
            // Enabled forces multistream, so it's not compatible with legacy protocol
            Self::Enabled => vec![PROTOCOL_MULTI_STREAM.into()],
            // Auto prioritize multistream, but must also be compatible with legacy protocol
            Self::Auto => vec![
                PROTOCOL_MULTI_STREAM.into(),
                PROTOCOL_SINGLE_STREAM.into(),
                PROTOCOL_LEGACY.into(),
            ],
        }
    }

    /// Returns the maximum concurrent uni streams that should be opened, i.e. one per priority
    /// except Control for multistream, zero otherwise.
    fn max_concurrent_uni_streams(&self) -> quinn::VarInt {
        match self {
            Self::Disabled => 0u8.into(),
            _ => (Priority::NUM as u8 - 1).into(),
        }
    }

    pub(crate) fn set_nb_concurrent_streams(
        &self,
        quic_transport_conf: &mut quinn::TransportConfig,
    ) {
        quic_transport_conf.max_concurrent_bidi_streams(1u8.into());
        quic_transport_conf.max_concurrent_uni_streams(self.max_concurrent_uni_streams());
    }
}

/// Priority-mapped uni streams.
///
/// `quinn` doesn't allow direct stream index manipulation, but provides instead API guarantees:
/// - streams are opened with increasing indexes
/// - streams creation doesn't yield if it doesn't overflow the limit, hence `now_or_never`
///
/// So, in order to map streams on priorities (except Control already mapped on the bi stream),
/// one stream per priority must be opened successively in the priority order. This way, the
/// stream index (starting from 0) can be used to retrieve its priority (starting from 1).
///
/// Streams could be opened directly in [`LinkUnicastQuic`] constructor, but this one cannot fail
/// because it's used in `Arc::new_cyclic`. So the failing part is extracted into this type.
struct UniStreams(Vec<quinn::SendStream>);

impl UniStreams {
    /// Opens priority-mapped uni streams if supported.
    ///
    /// This method leverages on QUIC ALPN (see [`MultiStreamConfig::alpn_protocols`]): if the
    /// negotiated protocol is [`PROTOCOL_MULTI_STREAM`], then uni streams are opened.
    /// Otherwise, it returns None.
    fn try_open(connection: &quinn::Connection) -> ZResult<Option<Self>> {
        let handshake_data = connection
            .handshake_data()
            .ok_or_else(|| zerror!("No handshake data"))?;
        let handshake_data = handshake_data
            .downcast_ref::<HandshakeData>()
            .expect("HandshakeData should be only existing implementation");
        let open_uni = |_prio| {
            let open = connection.open_uni().now_or_never();
            Ok(open.ok_or_else(|| zerror!("Cannot open uni stream"))??)
        };
        Ok(match handshake_data.protocol.as_deref() {
            Some(PROTOCOL_MULTI_STREAM) => Some(Self(
                (1..Priority::NUM).map(open_uni).collect::<ZResult<_>>()?,
            )),
            Some(PROTOCOL_SINGLE_STREAM | PROTOCOL_LEGACY) => None,
            _ => unreachable!(),
        })
    }
}

/// A maybe-pending [`quinn::RecvStream`].
///
/// `quinn` streams are only "accepted" when data is received, so they start with a "pending" state,
/// and are notified by [`RecvStream::acceptor_task`].
enum RecvStream {
    /// A pending channel waiting for [`RecvStream::acceptor_task`] notification.
    Pending(oneshot::Receiver<quinn::RecvStream>),
    /// An accepted stream
    Accepted(quinn::RecvStream),
}

impl RecvStream {
    /// Instantiate a task to accept uni streams and notify the associated pending channel.
    ///
    /// Streams are mapped to their priority using their index, see [`UniStreams`].
    /// The task stop when all streams have been received, or with connection errors; there is no
    /// cancellation to handle as the connection will be closed eventually, triggering an error
    /// if the task is still alive.
    async fn acceptor_task(
        connection: quinn::Connection,
        mut priority_txs: HashMap<usize, oneshot::Sender<quinn::RecvStream>>,
    ) -> ZResult<()> {
        while !priority_txs.is_empty() {
            let recv = connection.accept_uni().await?;
            // Uni streams' indexes starts from zero, while priorities above Control starts from 1,
            // hence the `+ 1`
            let prio = recv.id().index() as usize + 1;
            if let Some(tx) = priority_txs.remove(&prio) {
                // If the channel is closed, then the link is closed, so we don't care
                // as `accept_uni` above should fail quickly after
                tx.send(recv).ok();
            }
        }
        Ok(())
    }
}

pub struct QuicServerBuilder<'a, F: AcceptorCallback> {
    endpoint: &'a EndPoint,
    acceptor_params: QuicAcceptorParams<F>,
    is_streamed: bool,
    is_secure: bool,
}

impl<'a, F: AcceptorCallback> QuicServerBuilder<'a, F> {
    pub fn new(endpoint: &'a EndPoint, acceptor_params: QuicAcceptorParams<F>) -> Self {
        Self {
            endpoint,
            acceptor_params,
            is_streamed: true,
            is_secure: true,
        }
    }

    pub fn streamed(mut self, is_streamed: bool) -> Self {
        self.is_streamed = is_streamed;
        self
    }

    #[cfg(feature = "unsecure_quic")]
    pub fn security(mut self, is_secure: bool) -> Self {
        self.is_secure = is_secure;
        self
    }
}

impl<'a, F: AcceptorCallback> IntoFuture for QuicServerBuilder<'a, F> {
    type Output = ZResult<QuicServer<F>>;

    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(QuicServer::new(
            self.endpoint,
            self.acceptor_params,
            self.is_streamed,
            self.is_secure,
        ))
    }
}

pub struct QuicServer<F: AcceptorCallback> {
    pub quic_acceptor: QuicAcceptor<F>,
    pub locator: Locator,
    pub local_addr: SocketAddr,
}

impl<F: AcceptorCallback> QuicServer<F> {
    async fn new(
        endpoint: &EndPoint,
        acceptor_params: QuicAcceptorParams<F>,
        is_streamed: bool,
        is_secure: bool,
    ) -> ZResult<Self> {
        let epaddr = endpoint.address();
        let epconf = endpoint.config();
        let addr = get_quic_addr(&epaddr).await?;
        let host = get_quic_host(&epaddr)?;

        // Server config
        let mut server_crypto = TlsServerConfig::new(&epconf, is_secure)
            .await
            .map_err(|e| zerror!("Cannot create a new QUIC listener on {addr}: {e}"))?;

        let multistream = if is_streamed {
            Some(MultiStreamConfig::new(endpoint.metadata())?)
        } else {
            None
        };
        server_crypto.server_config.alpn_protocols = multistream
            .as_ref()
            .map(|m| m.alpn_protocols())
            .unwrap_or(vec![PROTOCOL_LEGACY.into()]);

        let quic_config: QuicServerConfig = server_crypto
            .server_config
            .try_into()
            .map_err(|e| zerror!("Can not create a new QUIC listener on {addr}: {e}"))?;

        let mut server_config = quinn::ServerConfig::with_crypto({
            if is_secure {
                Arc::new(quic_config)
            } else {
                Arc::new(PlainTextServerConfig::new(quic_config.into()))
            }
        });
        {
            let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
            QuicTransportConfigurator(transport_config)
                .configure_max_concurrent_streams(multistream.as_ref())
                .configure_mtu(&QuicMtuConfig::try_from(&epconf)?);
        }
        // Initialize the Endpoint
        let quic_endpoint = async {
            let socket = QuicSocketConfig::new(&epconf)
                .await
                .map_err(|e| zerror!("error parsing socket config: {e}"))?
                .new_listener(&addr)
                .await?;
            // create the Endpoint with the socket
            let runtime = quinn::default_runtime()
                .ok_or_else(|| std::io::Error::other("no async runtime found"))?;
            ZResult::Ok(quinn::Endpoint::new_with_abstract_socket(
                EndpointConfig::default(),
                Some(server_config),
                runtime.wrap_udp_socket(socket.into_std()?)?,
                runtime,
            )?)
        }
        .await
        .map_err(|e| zerror!("Can not create a new QUIC listener on {}: {}", addr, e))?;

        let local_addr = quic_endpoint
            .local_addr()
            .map_err(|e| zerror!("Can not create a new QUIC listener on {}: {}", addr, e))?;
        let local_port = local_addr.port();

        let locator = Locator::new(
            endpoint.protocol(),
            format!("{host}:{local_port}"),
            endpoint.metadata(),
        )?;

        Ok(Self {
            quic_acceptor: QuicAcceptor {
                quic_endpoint,
                tls_close_link_on_expiration: server_crypto.tls_close_link_on_expiration,
                is_streamed,
                inner: acceptor_params,
            },
            locator,
            local_addr,
        })
    }
}

pub struct QuicClientBuilder<'a> {
    endpoint: &'a EndPoint,
    is_streamed: bool,
    is_secure: bool,
}

impl<'a> QuicClientBuilder<'a> {
    pub fn new(endpoint: &'a EndPoint) -> Self {
        Self {
            endpoint,
            is_streamed: true,
            is_secure: true,
        }
    }

    pub fn streamed(mut self, is_streamed: bool) -> Self {
        self.is_streamed = is_streamed;
        self
    }

    #[cfg(feature = "unsecure_quic")]
    pub fn security(mut self, is_secure: bool) -> Self {
        self.is_secure = is_secure;
        self
    }
}

impl<'a> IntoFuture for QuicClientBuilder<'a> {
    type Output = ZResult<QuicClient>;

    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(QuicClient::new(
            self.endpoint,
            self.is_streamed,
            self.is_secure,
        ))
    }
}
pub struct QuicClient {
    pub quic_conn: quinn::Connection,
    pub streams: Option<QuicStreams>,
    pub src_addr: SocketAddr,
    pub dst_addr: SocketAddr,
    pub tls_close_link_on_expiration: bool,
}

impl QuicClient {
    async fn new(endpoint: &EndPoint, is_streamed: bool, is_secure: bool) -> ZResult<Self> {
        let epaddr = endpoint.address();
        let host = get_quic_host(&epaddr)?;
        let epconf = endpoint.config();
        let dst_addr = get_quic_addr(&epaddr).await?;

        // Initialize the QUIC connection
        let mut client_crypto = TlsClientConfig::new(&epconf, is_secure)
            .await
            .map_err(|e| zerror!("Cannot create a new QUIC client on {dst_addr}: {e}"))?;

        let multistream = if is_streamed {
            Some(MultiStreamConfig::new(endpoint.metadata())?)
        } else {
            None
        };

        client_crypto.client_config.alpn_protocols = multistream
            .as_ref()
            .map(|m| m.alpn_protocols())
            .unwrap_or(vec![PROTOCOL_LEGACY.into()]);

        let mut quic_endpoint = async {
            let socket = QuicSocketConfig::new(&epconf)
                .await
                .map_err(|e| zerror!("error parsing socket config: {e}"))?
                .new_link(&dst_addr)
                .await?;
            // create the Endpoint with the socket
            let runtime = quinn::default_runtime()
                .ok_or_else(|| std::io::Error::other("no async runtime found"))?;
            ZResult::Ok(quinn::Endpoint::new_with_abstract_socket(
                EndpointConfig::default(),
                None,
                runtime.wrap_udp_socket(socket.into_std()?)?,
                runtime,
            )?)
        }
        .await
        .map_err(|e| zerror!("Can not create a new QUIC link bound to {host}: {e}"))?;

        let quic_config: QuicClientConfig = client_crypto
            .client_config
            .try_into()
            .map_err(|e| zerror!("Can not get QUIC config {host}: {e}"))?;
        quic_endpoint.set_default_client_config({
            let mut client_config = quinn::ClientConfig::new({
                if is_secure {
                    Arc::new(quic_config)
                } else {
                    Arc::new(PlainTextClientConfig::new(quic_config.into()))
                }
            });
            let mut transport_config = quinn::TransportConfig::default();
            QuicTransportConfigurator(&mut transport_config)
                .configure_max_concurrent_streams(multistream.as_ref())
                .configure_mtu(&QuicMtuConfig::try_from(&epconf)?);
            client_config.transport_config(transport_config.into());
            client_config
        });

        let src_addr = quic_endpoint
            .local_addr()
            .map_err(|e| zerror!("Can not get QUIC local_addr bound to {}: {}", host, e))?;

        let quic_conn = quic_endpoint
            .connect(dst_addr, host)
            .map_err(|e| {
                zerror!(
                    "Can not get connect quick endpoint : {} : {} : {}",
                    dst_addr,
                    host,
                    e
                )
            })?
            .await
            .map_err(|e| zerror!("Can not create a new QUIC link bound to {}: {}", host, e))?;

        let mut streams = None;
        if is_streamed {
            let quic_streams = QuicStreams::open(&quic_conn)
                .await
                .map_err(|e| zerror!("Cannot initialize QUIC streams {}: {}", host, e))?;
            streams = Some(quic_streams);
        }

        Ok(Self {
            quic_conn,
            streams,
            src_addr,
            dst_addr,
            tls_close_link_on_expiration: client_crypto.tls_close_link_on_expiration,
        })
    }
}

// Boilerplate to avoid repeating the Fn bound in all generics that require it
pub trait AcceptorCallback:
    Fn(QuicLinkMaterial) -> ZResult<Arc<dyn LinkUnicastTrait>> + Send + Sync + 'static
{
}

impl<T: Fn(QuicLinkMaterial) -> ZResult<Arc<dyn LinkUnicastTrait>> + Send + Sync + 'static>
    AcceptorCallback for T
{
}

pub struct QuicAcceptorParams<F: AcceptorCallback> {
    pub token: CancellationToken,
    pub manager: NewLinkChannelSender,
    pub throttle_time: std::time::Duration,
    pub make_link: F,
}

pub struct QuicAcceptor<F: AcceptorCallback> {
    quic_endpoint: quinn::Endpoint,
    tls_close_link_on_expiration: bool,
    is_streamed: bool,
    inner: QuicAcceptorParams<F>,
}

impl<F: AcceptorCallback> IntoFuture for QuicAcceptor<F> {
    type Output = ZResult<()>;

    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.accept_task())
    }
}

impl<F: AcceptorCallback> QuicAcceptor<F> {
    async fn accept_task(self) -> ZResult<()> {
        async fn accept_connection(acceptor: quinn::Accept<'_>) -> ZResult<quinn::Connection> {
            let qc = acceptor
                .await
                .ok_or_else(|| zerror!("Can not accept QUIC connections: acceptor closed"))?;

            let conn = qc
                .await
                .map_err(|e| zerror!("QUIC acceptor failed: {:?}", e))?;

            Ok(conn)
        }

        let src_addr = self
            .quic_endpoint
            .local_addr()
            .map_err(|e| zerror!("Cannot start QUIC acceptor: {:?}", e))?;

        tracing::trace!("Ready to accept QUIC connections on: {:?}", src_addr);

        loop {
            tokio::select! {
                _ = self.inner.token.cancelled() => break,

                res = accept_connection(self.quic_endpoint.accept()) => {
                    match res {
                        Ok(quic_conn) => {
                            let link = match self.handle_accepted_connection(quic_conn, &src_addr).await {
                                Ok(link) => link,
                                Err(e) => {
                                    tracing::error!("Cannot accept QUIC connection: {e:?}");
                                    continue;
                                }
                            };
                            // Communicate the new link to the initial transport manager
                            if let Err(e) = self.inner.manager.send_async(LinkUnicast(link)).await {
                                tracing::error!("{}-{}: {}", file!(), line!(), e)
                            }
                        }
                        Err(e) => {
                            tracing::warn!("{} Hint: increase the system open file limit.", e);
                            // Throttle the accept loop upon an error
                            // NOTE: This might be due to various factors. However, the most common case is that
                            //       the process has reached the maximum number of open files in the system. On
                            //       Linux systems this limit can be changed by using the "ulimit" command line
                            //       tool. In case of systemd-based systems, this can be changed by using the
                            //       "sysctl" command line tool.
                            tokio::time::sleep(self.inner.throttle_time).await;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Handles an accepted [`quinn::Connection`], returning a link made by the provided callback.
    async fn handle_accepted_connection(
        &self,
        quic_conn: quinn::Connection,
        src_addr: &SocketAddr,
    ) -> ZResult<Arc<dyn LinkUnicastTrait>> {
        let streams = if self.is_streamed {
            Some(
                QuicStreams::accept(&quic_conn)
                    .await
                    .map_err(|e| zerror!("cannot initialize QUIC streams: {:?}", e))?,
            )
        } else {
            None
        };

        // Get the right source address in case an unsepecified IP (i.e. 0.0.0.0 or [::]) is used
        let ip = quic_conn.local_ip().ok_or(zerror!("empty local IP"))?;
        let src_addr = SocketAddr::new(ip, src_addr.port());
        let dst_addr = quic_conn.remote_address();

        let tls_close_link_on_expiration = self.tls_close_link_on_expiration;
        let link = (self.inner.make_link)(QuicLinkMaterial {
            quic_conn,
            src_addr,
            dst_addr,
            streams,
            tls_close_link_on_expiration,
        })?;

        Ok(link)
    }
}

/// Material for building a link after accepting a new connection on a QUIC listener
pub struct QuicLinkMaterial {
    pub quic_conn: quinn::Connection,
    pub src_addr: SocketAddr,
    pub dst_addr: SocketAddr,
    pub streams: Option<QuicStreams>,
    pub tls_close_link_on_expiration: bool,
}

pub struct QuicStreams {
    send: [UnsafeCell<Option<quinn::SendStream>>; Priority::NUM],
    recv: [UnsafeCell<Option<RecvStream>>; Priority::NUM],
    pub is_multistream: bool,
}

unsafe impl Sync for QuicStreams {}

impl QuicStreams {
    async fn open(connection: &quinn::Connection) -> ZResult<Self> {
        let (send, recv) = connection
            .open_bi()
            .await
            .map_err(|e| zerror!("Can not open QUIC bi-directional channel: {e}"))?;
        Self::new(connection, send, recv).await
    }

    async fn accept(connection: &quinn::Connection) -> ZResult<Self> {
        let (send, recv) = connection
            .accept_bi()
            .await
            .map_err(|e| zerror!("Can not accept QUIC bi-directional channel: {e}"))?;
        Self::new(connection, send, recv).await
    }

    async fn new(
        connection: &quinn::Connection,
        send: quinn::SendStream,
        recv: quinn::RecvStream,
    ) -> ZResult<Self> {
        let uni_streams = UniStreams::try_open(connection)?;
        // Initialize the streams with Control bi stream
        let mut send = vec![UnsafeCell::new(Some(send))];
        let mut recv = vec![UnsafeCell::new(Some(RecvStream::Accepted(recv)))];
        let is_multistream = uni_streams.is_some();
        // If multistream is enabled, initializes the priority-mapped streams
        if let Some(streams) = uni_streams {
            send.extend(streams.0.into_iter().map(Some).map(UnsafeCell::new));
            let mut priority_txs = HashMap::new();
            // For each priority, creates a channel to notify the acceptation and initialize
            // the stream to pending
            for prio in 1..Priority::NUM {
                let (tx, rx) = oneshot::channel();
                priority_txs.insert(prio, tx);
                recv.push(UnsafeCell::new(Some(RecvStream::Pending(rx))));
            }
            zenoh_runtime::ZRuntime::Acceptor
                .spawn(RecvStream::acceptor_task(connection.clone(), priority_txs));
        } else {
            send.resize_with(Priority::NUM, Default::default);
            recv.resize_with(Priority::NUM, Default::default);
        }
        Ok(Self {
            send: send.try_into().unwrap(),
            recv: recv.try_into().unwrap(),
            is_multistream,
        })
    }

    /// # Safety
    ///
    /// There should be no concurrent calls to read/read_exact per priority.
    pub async unsafe fn read(
        &self,
        buffer: &mut [u8],
        priority: Option<Priority>,
    ) -> ZResult<usize> {
        let recv = unsafe { self.read_stream(priority).await? };
        recv.read(buffer)
            .await
            .map_err(Into::<zenoh_result::Error>::into)?
            .ok_or_else(|| zerror!("stream {} has been closed", recv.id()).into())
    }

    /// # Safety
    ///
    /// There should be no concurrent calls to read/read_exact per priority.
    pub async unsafe fn read_exact(
        &self,
        buffer: &mut [u8],
        priority: Option<Priority>,
    ) -> ZResult<()> {
        let recv = unsafe { self.read_stream(priority).await? };
        recv.read_exact(buffer).await.map_err(Into::into)
    }

    /// # Safety
    ///
    /// There should be no concurrent calls to write/write_all per priority.
    pub async unsafe fn write(&self, buffer: &[u8], priority: Option<Priority>) -> ZResult<usize> {
        unsafe { self.write_stream(priority) }
            .write(buffer)
            .await
            .map_err(Into::into)
    }

    /// # Safety
    ///
    /// There should be no concurrent calls to write/write_all per priority.
    pub async unsafe fn write_all(&self, buffer: &[u8], priority: Option<Priority>) -> ZResult<()> {
        unsafe { self.write_stream(priority) }
            .write_all(buffer)
            .await
            .map_err(Into::into)
    }

    /// Retrieved the write-stream mapped to the priority
    ///
    /// # Safety
    ///
    /// There should be only one caller per priority.
    #[allow(clippy::mut_from_ref)]
    unsafe fn write_stream(&self, priority: Option<Priority>) -> &mut quinn::SendStream {
        let prio = priority.unwrap_or(Priority::Control) as usize;
        unsafe { &mut *self.send[prio].get() }
            .as_mut()
            .expect("multistream should have been started")
    }

    /// Retrieved the read-stream mapped to the priority
    ///
    /// The stream may be pending, in which case we wait until it is accepted.
    ///
    /// # Safety
    ///
    /// There should be only one caller per priority.
    #[allow(clippy::mut_from_ref)]
    async unsafe fn read_stream(
        &self,
        priority: Option<Priority>,
    ) -> ZResult<&mut quinn::RecvStream> {
        let prio = priority.unwrap_or(Priority::Control) as usize;
        match unsafe { &mut *self.recv[prio].get() }
            .as_mut()
            .expect("multistream should have been started")
        {
            stream @ RecvStream::Pending(_) => {
                let RecvStream::Pending(rx) = stream else {
                    unreachable!()
                };
                let recv = rx.await.map_err(|_| zerror!("Connection closed"))?;
                *stream = RecvStream::Accepted(recv);
                let RecvStream::Accepted(recv) = stream else {
                    unreachable!()
                };
                Ok(recv)
            }
            RecvStream::Accepted(recv) => Ok(recv),
        }
    }
}
