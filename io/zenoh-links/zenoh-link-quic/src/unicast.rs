// Copyright (c) 2024 ZettaScale Technology
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

use crate::base64_decode;
use crate::{
    config::*, get_quic_addr, verify::WebPkiVerifierAnyServerName, ALPN_QUIC_HTTP,
    QUIC_ACCEPT_THROTTLE_TIME, QUIC_DEFAULT_MTU, QUIC_LOCATOR_PREFIX,
};
use async_trait::async_trait;
use rustls::{Certificate, PrivateKey};
use rustls_pemfile::Item;
use std::fmt;
use std::io::BufReader;
use std::net::IpAddr;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use zenoh_core::zasynclock;
use zenoh_link_commons::{
    get_ip_interface_names, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait,
    ListenersUnicastIP, NewLinkChannelSender,
};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{bail, zerror, ZError, ZResult};

pub struct LinkUnicastQuic {
    connection: quinn::Connection,
    src_addr: SocketAddr,
    src_locator: Locator,
    dst_locator: Locator,
    send: AsyncMutex<quinn::SendStream>,
    recv: AsyncMutex<quinn::RecvStream>,
}

impl LinkUnicastQuic {
    fn new(
        connection: quinn::Connection,
        src_addr: SocketAddr,
        dst_locator: Locator,
        send: quinn::SendStream,
        recv: quinn::RecvStream,
    ) -> LinkUnicastQuic {
        // Build the Quic object
        LinkUnicastQuic {
            connection,
            src_addr,
            src_locator: Locator::new(QUIC_LOCATOR_PREFIX, src_addr.to_string(), "").unwrap(),
            dst_locator,
            send: AsyncMutex::new(send),
            recv: AsyncMutex::new(recv),
        }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastQuic {
    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing QUIC link: {}", self);
        // Flush the QUIC stream
        let mut guard = zasynclock!(self.send);
        if let Err(e) = guard.finish().await {
            tracing::trace!("Error closing QUIC stream {}: {}", self, e);
        }
        self.connection.close(quinn::VarInt::from_u32(0), &[0]);
        Ok(())
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
    fn get_mtu(&self) -> u16 {
        *QUIC_DEFAULT_MTU
    }

    #[inline(always)]
    fn get_interface_names(&self) -> Vec<String> {
        get_ip_interface_names(&self.src_addr)
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
        let host = epaddr
            .as_str()
            .split(':')
            .next()
            .ok_or("Endpoints must be of the form quic/<address>:<port>")?;
        let epconf = endpoint.config();

        let addr = get_quic_addr(&epaddr).await?;

        let server_name_verification: bool = epconf
            .get(TLS_SERVER_NAME_VERIFICATION)
            .unwrap_or(TLS_SERVER_NAME_VERIFICATION_DEFAULT)
            .parse()?;

        if !server_name_verification {
            tracing::warn!("Skipping name verification of servers");
        }

        // Initialize the QUIC connection
        let mut root_cert_store = rustls::RootCertStore::empty();

        // Read the certificates
        let f = if let Some(value) = epconf.get(TLS_ROOT_CA_CERTIFICATE_RAW) {
            value.as_bytes().to_vec()
        } else if let Some(b64_certificate) = epconf.get(TLS_ROOT_CA_CERTIFICATE_BASE64) {
            base64_decode(b64_certificate)?
        } else if let Some(value) = epconf.get(TLS_ROOT_CA_CERTIFICATE_FILE) {
            tokio::fs::read(value)
                .await
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
        } else {
            vec![]
        };

        let certificates = if f.is_empty() {
            rustls_native_certs::load_native_certs()
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
                .drain(..)
                .map(|x| rustls::Certificate(x.to_vec()))
                .collect::<Vec<rustls::Certificate>>()
        } else {
            rustls_pemfile::certs(&mut BufReader::new(f.as_slice()))
                .map(|result| {
                    result
                        .map_err(|err| zerror!("Invalid QUIC CA certificate file: {}", err))
                        .map(|der| Certificate(der.to_vec()))
                })
                .collect::<Result<Vec<rustls::Certificate>, ZError>>()?
        };
        for c in certificates.iter() {
            root_cert_store.add(c).map_err(|e| zerror!("{}", e))?;
        }

        let client_crypto = rustls::ClientConfig::builder().with_safe_defaults();

        let mut client_crypto = if server_name_verification {
            client_crypto
                .with_root_certificates(root_cert_store)
                .with_no_client_auth()
        } else {
            client_crypto
                .with_custom_certificate_verifier(Arc::new(WebPkiVerifierAnyServerName::new(
                    root_cert_store,
                )))
                .with_no_client_auth()
        };

        client_crypto.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

        let ip_addr: IpAddr = if addr.is_ipv4() {
            Ipv4Addr::UNSPECIFIED.into()
        } else {
            Ipv6Addr::UNSPECIFIED.into()
        };
        let mut quic_endpoint = quinn::Endpoint::client(SocketAddr::new(ip_addr, 0))
            .map_err(|e| zerror!("Can not create a new QUIC link bound to {}: {}", host, e))?;
        quic_endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(client_crypto)));

        let src_addr = quic_endpoint
            .local_addr()
            .map_err(|e| zerror!("Can not create a new QUIC link bound to {}: {}", host, e))?;

        let quic_conn = quic_endpoint
            .connect(addr, host)
            .map_err(|e| zerror!("Can not create a new QUIC link bound to {}: {}", host, e))?
            .await
            .map_err(|e| zerror!("Can not create a new QUIC link bound to {}: {}", host, e))?;

        let (send, recv) = quic_conn
            .open_bi()
            .await
            .map_err(|e| zerror!("Can not create a new QUIC link bound to {}: {}", host, e))?;

        let link = Arc::new(LinkUnicastQuic::new(
            quic_conn,
            src_addr,
            endpoint.into(),
            send,
            recv,
        ));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let epaddr = endpoint.address();
        let epconf = endpoint.config();

        if epconf.is_empty() {
            bail!("No QUIC configuration provided");
        };

        let addr = get_quic_addr(&epaddr).await?;

        let f = if let Some(value) = epconf.get(TLS_SERVER_CERTIFICATE_RAW) {
            value.as_bytes().to_vec()
        } else if let Some(b64_certificate) = epconf.get(TLS_SERVER_CERTIFICATE_BASE64) {
            base64_decode(b64_certificate)?
        } else if let Some(value) = epconf.get(TLS_SERVER_CERTIFICATE_FILE) {
            tokio::fs::read(value)
                .await
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
        } else {
            bail!("No QUIC CA certificate has been provided.");
        };
        let certificates = rustls_pemfile::certs(&mut BufReader::new(f.as_slice()))
            .map(|result| {
                result
                    .map_err(|err| zerror!("Invalid QUIC CA certificate file: {}", err))
                    .map(|der| Certificate(der.to_vec()))
            })
            .collect::<Result<Vec<rustls::Certificate>, ZError>>()?;

        // Private keys
        let f = if let Some(value) = epconf.get(TLS_SERVER_PRIVATE_KEY_RAW) {
            value.as_bytes().to_vec()
        } else if let Some(b64_key) = epconf.get(TLS_SERVER_PRIVATE_KEY_BASE64) {
            base64_decode(b64_key)?
        } else if let Some(value) = epconf.get(TLS_SERVER_PRIVATE_KEY_FILE) {
            tokio::fs::read(value)
                .await
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
        } else {
            bail!("No QUIC CA private key has been provided.");
        };
        let items: Vec<Item> = rustls_pemfile::read_all(&mut BufReader::new(f.as_slice()))
            .collect::<Result<_, _>>()
            .map_err(|err| zerror!("Invalid QUIC CA private key file: {}", err))?;

        let private_key = items
            .into_iter()
            .filter_map(|x| match x {
                rustls_pemfile::Item::Pkcs1Key(k) => Some(k.secret_pkcs1_der().to_vec()),
                rustls_pemfile::Item::Pkcs8Key(k) => Some(k.secret_pkcs8_der().to_vec()),
                rustls_pemfile::Item::Sec1Key(k) => Some(k.secret_sec1_der().to_vec()),
                _ => None,
            })
            .take(1)
            .next()
            .ok_or_else(|| zerror!("No QUIC CA private key has been provided."))
            .map(PrivateKey)?;

        // Server config
        let mut server_crypto = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certificates, private_key)?;
        server_crypto.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();
        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));

        // We do not accept unidireactional streams.
        Arc::get_mut(&mut server_config.transport)
            .unwrap()
            .max_concurrent_uni_streams(0_u8.into());
        // For the time being we only allow one bidirectional stream
        Arc::get_mut(&mut server_config.transport)
            .unwrap()
            .max_concurrent_bidi_streams(1_u8.into());

        // Initialize the Endpoint
        let quic_endpoint = quinn::Endpoint::server(server_config, addr)
            .map_err(|e| zerror!("Can not create a new QUIC listener on {}: {}", addr, e))?;

        let local_addr = quic_endpoint
            .local_addr()
            .map_err(|e| zerror!("Can not create a new QUIC listener on {}: {}", addr, e))?;

        // Update the endpoint locator address
        endpoint = EndPoint::new(
            endpoint.protocol(),
            local_addr.to_string(),
            endpoint.metadata(),
            endpoint.config(),
        )?;

        // Spawn the accept loop for the listener
        let token = self.listeners.token.child_token();
        let c_token = token.clone();

        let c_manager = self.manager.clone();

        let task = async move { accept_task(quic_endpoint, c_token, c_manager).await };

        // Initialize the QuicAcceptor
        let locator = endpoint.to_locator();

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
    endpoint: quinn::Endpoint,
    token: CancellationToken,
    manager: NewLinkChannelSender,
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

    let src_addr = endpoint
        .local_addr()
        .map_err(|e| zerror!("Can not accept QUIC connections: {}", e))?;

    // The accept future
    tracing::trace!("Ready to accept QUIC connections on: {:?}", src_addr);

    loop {
        tokio::select! {
            _ = token.cancelled() => break,

            res = accept(endpoint.accept()) => {
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

                        let dst_addr = quic_conn.remote_address();
                        tracing::debug!("Accepted QUIC connection on {:?}: {:?}", src_addr, dst_addr);
                        // Create the new link object
                        let link = Arc::new(LinkUnicastQuic::new(
                            quic_conn,
                            src_addr,
                            Locator::new(QUIC_LOCATOR_PREFIX, dst_addr.to_string(), "")?,
                            send,
                            recv,
                        ));

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
