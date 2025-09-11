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
    fmt,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use quinn::{
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
    EndpointConfig,
};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use zenoh_core::zasynclock;
use zenoh_link_commons::{
    get_ip_interface_names, parse_dscp,
    quic::{
        get_cert_chain_expiration, get_cert_common_name, get_quic_addr, get_quic_host,
        TlsClientConfig, TlsServerConfig, ALPN_QUIC_HTTP,
    },
    set_dscp,
    tls::expiration::{LinkCertExpirationManager, LinkWithCertExpiration},
    LinkAuthId, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, ListenersUnicastIP,
    NewLinkChannelSender, BIND_INTERFACE, BIND_SOCKET,
};
use zenoh_protocol::{
    core::{Address, EndPoint, Locator},
    transport::BatchSize,
};
use zenoh_result::{bail, zerror, ZResult};

use super::{QUIC_ACCEPT_THROTTLE_TIME, QUIC_DEFAULT_MTU, QUIC_LOCATOR_PREFIX};

pub struct LinkUnicastQuic {
    connection: quinn::Connection,
    src_addr: SocketAddr,
    src_locator: Locator,
    dst_locator: Locator,
    send: AsyncMutex<quinn::SendStream>,
    recv: AsyncMutex<quinn::RecvStream>,
    auth_identifier: LinkAuthId,
    expiration_manager: Option<LinkCertExpirationManager>,
}

impl LinkUnicastQuic {
    fn new(
        connection: quinn::Connection,
        src_addr: SocketAddr,
        dst_locator: Locator,
        send: quinn::SendStream,
        recv: quinn::RecvStream,
        auth_identifier: LinkAuthId,
        expiration_manager: Option<LinkCertExpirationManager>,
    ) -> LinkUnicastQuic {
        // Build the Quic object
        LinkUnicastQuic {
            connection,
            src_addr,
            src_locator: Locator::new(QUIC_LOCATOR_PREFIX, src_addr.to_string(), "").unwrap(),
            dst_locator,
            send: AsyncMutex::new(send),
            recv: AsyncMutex::new(recv),
            auth_identifier,
            expiration_manager,
        }
    }

    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing QUIC link: {}", self);
        // Flush the QUIC stream
        let mut guard = zasynclock!(self.send);
        if let Err(e) = guard.finish() {
            tracing::trace!("Error closing QUIC stream {}: {}", self, e);
        }
        self.connection.close(quinn::VarInt::from_u32(0), &[0]);
        Ok(())
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastQuic {
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
        let mut guard = zasynclock!(self.send);
        guard.write(buffer).await.map_err(|e| {
            tracing::trace!("Write error on QUIC link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut guard = zasynclock!(self.send);
        guard.write_all(buffer).await.map_err(|e| {
            tracing::trace!("Write error on QUIC link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let mut guard = zasynclock!(self.recv);
        guard
            .read(buffer)
            .await
            .map_err(|e| {
                let e = zerror!("Read error on QUIC link {}: {}", self, e);
                tracing::trace!("{}", &e);
                e
            })?
            .ok_or_else(|| {
                let e = zerror!(
                    "Read error on QUIC link {}: stream {} has been closed",
                    self,
                    guard.id()
                );
                tracing::trace!("{}", &e);
                e.into()
            })
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let mut guard = zasynclock!(self.recv);
        guard.read_exact(buffer).await.map_err(|e| {
            let e = zerror!("Read error on QUIC link {}: {}", self, e);
            tracing::trace!("{}", &e);
            e.into()
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
        *QUIC_DEFAULT_MTU
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
impl LinkWithCertExpiration for LinkUnicastQuic {
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

impl Drop for LinkUnicastQuic {
    fn drop(&mut self) {
        self.connection.close(quinn::VarInt::from_u32(0), &[0]);
    }
}

impl fmt::Display for LinkUnicastQuic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} => {}",
            self.src_addr,
            self.connection.remote_address()
        )?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastQuic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Quic")
            .field("src", &self.src_addr)
            .field("dst", &self.connection.remote_address())
            .finish()
    }
}

pub struct LinkManagerUnicastQuic {
    manager: NewLinkChannelSender,
    listeners: ListenersUnicastIP,
}

impl LinkManagerUnicastQuic {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: ListenersUnicastIP::new(),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastQuic {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
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

        client_crypto.client_config.alpn_protocols =
            ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

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
        quic_endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(quic_config)));

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

        let (send, recv) = quic_conn
            .open_bi()
            .await
            .map_err(|e| zerror!("Can not open Quic bi-directional channel {}: {}", host, e))?;

        let auth_id = get_cert_common_name(&quic_conn)?;
        let certchain_expiration_time =
            get_cert_chain_expiration(&quic_conn)?.expect("server should have certificate chain");

        let link = Arc::<LinkUnicastQuic>::new_cyclic(|weak_link| {
            let mut expiration_manager = None;
            if client_crypto.tls_close_link_on_expiration {
                // setup expiration manager
                expiration_manager = Some(LinkCertExpirationManager::new(
                    weak_link.clone(),
                    src_addr,
                    dst_addr,
                    QUIC_LOCATOR_PREFIX,
                    certchain_expiration_time,
                ))
            }
            LinkUnicastQuic::new(
                quic_conn,
                src_addr,
                endpoint.into(),
                send,
                recv,
                auth_id.into(),
                expiration_manager,
            )
        });

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
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
        server_crypto.server_config.alpn_protocols =
            ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

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

        // We do not accept unidireactional streams.
        Arc::get_mut(&mut server_config.transport)
            .unwrap()
            .max_concurrent_uni_streams(0_u8.into());
        // For the time being we only allow one bidirectional stream
        Arc::get_mut(&mut server_config.transport)
            .unwrap()
            .max_concurrent_bidi_streams(1_u8.into());

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

        // Spawn the accept loop for the listener
        let token = self.listeners.token.child_token();

        let task = {
            let token = token.clone();
            let manager = self.manager.clone();

            async move {
                accept_task(
                    quic_endpoint,
                    token,
                    manager,
                    server_crypto.tls_close_link_on_expiration,
                )
                .await
            }
        };

        // Initialize the QuicAcceptor
        self.listeners
            .add_listener(endpoint, local_addr, task, token)
            .await?;

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let epaddr = endpoint.address();
        let addr = get_quic_addr(&epaddr).await?;
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
    quic_endpoint: quinn::Endpoint,
    token: CancellationToken,
    manager: NewLinkChannelSender,
    tls_close_link_on_expiration: bool,
) -> ZResult<()> {
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

    // The accept future
    tracing::trace!("Ready to accept QUIC connections on: {:?}", src_addr);

    loop {
        tokio::select! {
            _ = token.cancelled() => break,

            res = accept(quic_endpoint.accept()) => {
                match res {
                    Ok(quic_conn) => {
                        // Get the bideractional streams. Note that we don't allow unidirectional streams.
                        let (send, recv) = match quic_conn.accept_bi().await {
                            Ok(stream) => stream,
                            Err(e) => {
                                tracing::warn!("QUIC connection has no streams: {:?}", e);
                                continue;
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
                        let dst_locator = Locator::new(QUIC_LOCATOR_PREFIX, dst_addr.to_string(), "")?;
                        // Get Quic auth identifier
                        let auth_id = get_cert_common_name(&quic_conn)?;

                        // Get certificate chain expiration
                        let mut maybe_expiration_time = None;
                        if tls_close_link_on_expiration {
                            match get_cert_chain_expiration(&quic_conn)? {
                                exp @ Some(_) => maybe_expiration_time = exp,
                                None => tracing::warn!(
                                    "Cannot monitor expiration for QUIC link {:?} => {:?} : client does not have certificates",
                                    src_addr,
                                    dst_addr,
                                ),
                            }
                        }

                        tracing::debug!("Accepted QUIC connection on {:?}: {:?}. {:?}.", src_addr, dst_addr, auth_id);
                        // Create the new link object
                        let link = Arc::<LinkUnicastQuic>::new_cyclic(|weak_link| {
                            let mut expiration_manager = None;
                            if let Some(certchain_expiration_time) = maybe_expiration_time {
                                // setup expiration manager
                                expiration_manager = Some(LinkCertExpirationManager::new(
                                    weak_link.clone(),
                                    src_addr,
                                    dst_addr,
                                    QUIC_LOCATOR_PREFIX,
                                    certchain_expiration_time,
                                ));
                            }
                            LinkUnicastQuic::new(
                                quic_conn,
                                src_addr,
                                dst_locator,
                                send,
                                recv,
                                auth_id.into(),
                                expiration_manager,
                            )
                        });

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
                        tokio::time::sleep(Duration::from_micros(*QUIC_ACCEPT_THROTTLE_TIME)).await;
                    }
                }
            }
        }
    }
    Ok(())
}
