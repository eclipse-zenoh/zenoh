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
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use futures::FutureExt;
use quinn::{
    crypto::rustls::{HandshakeData, QuicClientConfig, QuicServerConfig},
    EndpointConfig,
};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zenoh_config::{EndPoint, Locator};
use zenoh_core::{bail, zerror};
use zenoh_protocol::core::{Address, Metadata, Priority};
use zenoh_result::ZResult;

use crate::{
    parse_dscp,
    quic::{
        get_quic_addr,
        get_quic_host,
        QuicMtuConfig,
        TlsClientConfig,
        TlsServerConfig,
        PROTOCOL_LEGACY,
        PROTOCOL_MULTI_STREAM,
        PROTOCOL_SINGLE_STREAM, // TODO: remove this alias
    },
    set_dscp, LinkUnicast, LinkUnicastTrait, NewLinkChannelSender, BIND_INTERFACE, BIND_SOCKET,
};

/// Quic endpoint `multistream` config
enum MultiStreamConfig {
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

    fn set_nb_concurrent_streams(
        &self,
        quic_transport_conf: &mut quinn::TransportConfig,
        is_streamed: bool,
    ) {
        quic_transport_conf
            .max_concurrent_bidi_streams(is_streamed.then(|| 1u8).unwrap_or(0u8).into());
        quic_transport_conf.max_concurrent_uni_streams(
            is_streamed
                .then(|| self.max_concurrent_uni_streams())
                .unwrap_or(0u8.into()),
        );
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

pub struct QuicLink {}

impl QuicLink {
    pub async fn server(
        endpoint: &EndPoint,
        is_streamed: bool,
    ) -> ZResult<(quinn::Endpoint, Locator, SocketAddr, bool)> {
        let epaddr = endpoint.address();
        let epconf = endpoint.config();

        if epconf.is_empty() {
            bail!("No QUIC configuration provided");
        };

        let addr = get_quic_addr(&epaddr).await?;
        let host = get_quic_host(&epaddr)?;

        // Server config
        let mut server_crypto = TlsServerConfig::new(&epconf)
            .await
            .map_err(|e| zerror!("Cannot create a new QUIC listener on {addr}: {e}"))?;

        let multistream = MultiStreamConfig::new(endpoint.metadata())?;
        server_crypto.server_config.alpn_protocols = multistream.alpn_protocols();

        // Install ring based rustls CryptoProvider.
        rustls::crypto::ring::default_provider()
            // This can be called successfully at most once in any process execution.
            // Call this early in your process to configure which provider is used for the provider.
            // The configuration should happen before any use of ClientConfig::builder() or ServerConfig::builder().
            .install_default()
            // Ignore the error here, because `rustls::crypto::ring::default_provider().install_default()` will inevitably be executed multiple times
            // when there are multiple quic links, and all but the first execution will fail.
            .ok();

        let quic_config: QuicServerConfig = server_crypto
            .server_config
            .try_into()
            .map_err(|e| zerror!("Can not create a new QUIC listener on {addr}: {e}"))?;

        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_config));
        multistream.set_nb_concurrent_streams(
            Arc::get_mut(&mut server_config.transport).unwrap(),
            is_streamed,
        );

        let mtu_config = QuicMtuConfig::try_from(&epconf)?;
        mtu_config.apply_to_transport(Arc::get_mut(&mut server_config.transport).unwrap());

        // Initialize the Endpoint
        let quic_endpoint = if let Some(iface) = server_crypto.bind_iface {
            async {
                // Bind the UDP socket
                let socket = tokio::net::UdpSocket::bind(addr).await?;
                zenoh_util::net::set_bind_to_device_udp_socket(&socket, iface)?;

                // create the Endpoint with this socket
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
        } else {
            quinn::Endpoint::server(server_config, addr).map_err(Into::into)
        }
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

        Ok((
            quic_endpoint,
            locator,
            local_addr,
            server_crypto.tls_close_link_on_expiration,
        ))
    }

    pub async fn connect(
        endpoint: &EndPoint,
        is_streamed: bool,
    ) -> ZResult<(
        quinn::Connection,
        Option<QuicStreams>,
        SocketAddr,
        SocketAddr,
        bool,
    )> {
        let epaddr = endpoint.address();
        let host = get_quic_host(&epaddr)?;
        let epconf = endpoint.config();
        let dst_addr = get_quic_addr(&epaddr).await?;

        // if both `iface`, and `bind` are present, return error
        if let (Some(_), Some(_)) = (epconf.get(BIND_INTERFACE), epconf.get(BIND_SOCKET)) {
            bail!(
                "Using Config options `iface` and `bind` in conjunction is unsupported at this time {} {:?}",
                BIND_INTERFACE,
                BIND_SOCKET
            )
        }

        // Initialize the QUIC connection
        let mut client_crypto = TlsClientConfig::new(&epconf)
            .await
            .map_err(|e| zerror!("Cannot create a new QUIC client on {dst_addr}: {e}"))?;

        let multistream = MultiStreamConfig::new(endpoint.metadata())?;
        client_crypto.client_config.alpn_protocols = multistream.alpn_protocols();

        let src_addr = if let Some(bind_socket_str) = epconf.get(BIND_SOCKET) {
            get_quic_addr(&Address::from(bind_socket_str)).await?
        } else if dst_addr.is_ipv4() {
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)
        } else {
            SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 0)
        };

        let socket = tokio::net::UdpSocket::bind(src_addr).await?;
        if let Some(dscp) = parse_dscp(&epconf)? {
            set_dscp(&socket, src_addr, dscp)?;
        }

        // Initialize the Endpoint
        if let Some(iface) = client_crypto.bind_iface {
            zenoh_util::net::set_bind_to_device_udp_socket(&socket, iface)?;
        };

        let mut quic_endpoint = {
            // create the Endpoint with this socket
            let runtime = quinn::default_runtime()
                .ok_or_else(|| std::io::Error::other("no async runtime found"))?;
            ZResult::Ok(quinn::Endpoint::new_with_abstract_socket(
                EndpointConfig::default(),
                None,
                runtime.wrap_udp_socket(socket.into_std()?)?,
                runtime,
            )?)
        }
        .map_err(|e| zerror!("Can not create a new QUIC link bound to {host}: {e}"))?;

        let quic_config: QuicClientConfig = client_crypto
            .client_config
            .try_into()
            .map_err(|e| zerror!("Can not get QUIC config {host}: {e}"))?;
        quic_endpoint.set_default_client_config({
            let mut client_config = quinn::ClientConfig::new(Arc::new(quic_config));
            let mut transport_config = quinn::TransportConfig::default();
            multistream.set_nb_concurrent_streams(&mut transport_config, is_streamed);
            client_config.transport_config(transport_config.into());
            client_config
        });

        let src_addr = quic_endpoint
            .local_addr()
            .map_err(|e| zerror!("Can not get QUIC local_addr bound to {}: {}", host, e))?;

        let connection = quic_endpoint
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
            let quic_streams = QuicStreams::open(&connection)
                .await
                .map_err(|e| zerror!("Can not open QUIC bi-directional channel {}: {}", host, e))?;
            streams = Some(quic_streams);
        }

        Ok((
            connection,
            streams,
            src_addr,
            dst_addr,
            client_crypto.tls_close_link_on_expiration,
        ))
    }

    pub async fn accept_task<Callback>(
        quic_endpoint: quinn::Endpoint,
        token: CancellationToken,
        manager: NewLinkChannelSender,
        is_streamed: bool,
        throttle_time: Duration,
        make_link: Callback,
    ) -> ZResult<()>
    where
        Callback: Fn(QuicLinkMaterial) -> ZResult<Arc<dyn LinkUnicastTrait>>,
    {
        async fn accept(acceptor: quinn::Accept<'_>) -> ZResult<quinn::Connection> {
            let qc = acceptor
                .await
                .ok_or_else(|| zerror!("Can not accept QUIC connections: acceptor closed"))?;

            let conn = qc.await.map_err(|e| {
                let e = zerror!("QUIC acceptor failed: {:?}", e);
                tracing::warn!("{}", e);
                e
            })?;

            Ok(conn)
        }

        let src_addr = quic_endpoint
            .local_addr()
            .map_err(|e| zerror!("Can not accept QUIC connections: {}", e))?;

        tracing::trace!("Ready to accept QUIC connections on: {:?}", src_addr);

        loop {
            tokio::select! {
                _ = token.cancelled() => break,

                res = accept(quic_endpoint.accept()) => {
                    match res {
                        Ok(quic_conn) => {
                            let mut streams = None;
                            if is_streamed {
                                match QuicStreams::accept(&quic_conn).await {
                                    Ok(quic_stream) => {
                                        streams = Some(quic_stream);
                                    },
                                    Err(e) => {
                                        tracing::warn!("QUIC connection has no streams: {:?}", e);
                                        continue;
                                    }
                                }
                            };

                            // Get the right source address in case an unsepecified IP (i.e. 0.0.0.0 or [::]) is used
                            let src_addr =  match quic_conn.local_ip()  {
                                Some(ip) => SocketAddr::new(ip, src_addr.port()),
                                None => {
                                    tracing::debug!("Can not accept QUIC connection: empty local IP");
                                    continue;
                                }
                            };
                            let dst_addr = quic_conn.remote_address();

                            let link = make_link(QuicLinkMaterial {
                                quic_conn,
                                src_addr,
                                dst_addr,
                                streams
                            })?;
                            // Communicate the new link to the initial transport manager
                            if let Err(e) = manager.send_async(LinkUnicast(link)).await {
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
                            tokio::time::sleep(throttle_time).await;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Material for building a link after accepting a new connection on a QUIC listener
pub struct QuicLinkMaterial {
    pub quic_conn: quinn::Connection,
    pub src_addr: SocketAddr,
    pub dst_addr: SocketAddr,
    pub streams: Option<QuicStreams>,
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
        let uni_streams = UniStreams::try_open(&connection)?;
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
