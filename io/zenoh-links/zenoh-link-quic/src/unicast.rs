//
// Copyright (c) 2022 ZettaScale Technology
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
    config::*, get_quic_addr, get_quic_dns, ALPN_QUIC_HTTP, QUIC_ACCEPT_THROTTLE_TIME,
    QUIC_DEFAULT_MTU, QUIC_LOCATOR_PREFIX,
};
use async_std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use async_std::prelude::FutureExt;
use async_std::sync::Mutex as AsyncMutex;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use futures::stream::StreamExt;
use std::collections::HashMap;
use std::fmt;
use std::io::BufReader;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_core::{bail, Result as ZResult};
use zenoh_core::{zasynclock, zerror, zread, zwrite};
use zenoh_link_commons::{
    LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, NewLinkChannelSender,
};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_sync::Signal;

pub struct LinkUnicastQuic {
    connection: quinn::NewConnection,
    src_addr: SocketAddr,
    src_locator: Locator,
    dst_locator: Locator,
    send: AsyncMutex<quinn::SendStream>,
    recv: AsyncMutex<quinn::RecvStream>,
}

impl LinkUnicastQuic {
    fn new(
        connection: quinn::NewConnection,
        src_addr: SocketAddr,
        dst_locator: Locator,
        send: quinn::SendStream,
        recv: quinn::RecvStream,
    ) -> LinkUnicastQuic {
        // Build the Quic object
        LinkUnicastQuic {
            connection,
            src_addr,
            src_locator: Locator::new(QUIC_LOCATOR_PREFIX, &src_addr),
            dst_locator,
            send: AsyncMutex::new(send),
            recv: AsyncMutex::new(recv),
        }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastQuic {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing QUIC link: {}", self);
        // Flush the QUIC stream
        let mut guard = zasynclock!(self.send);
        if let Err(e) = guard.finish().await {
            log::trace!("Error closing QUIC stream {}: {}", self, e);
        }
        self.connection
            .connection
            .close(quinn::VarInt::from_u32(0), &[0]);
        Ok(())
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        let mut guard = zasynclock!(self.send);
        guard.write(buffer).await.map_err(|e| {
            log::trace!("Write error on QUIC link {}: {}", self, e);
            zerror!(e).into()
        })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut guard = zasynclock!(self.send);
        guard.write_all(buffer).await.map_err(|e| {
            log::trace!("Write error on QUIC link {}: {}", self, e);
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
                log::trace!("{}", &e);
                e
            })?
            .ok_or_else(|| {
                let e = zerror!(
                    "Read error on QUIC link {}: stream {} has been closed",
                    self,
                    guard.id()
                );
                log::trace!("{}", &e);
                e.into()
            })
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let mut guard = zasynclock!(self.recv);
        guard.read_exact(buffer).await.map_err(|e| {
            let e = zerror!("Read error on QUIC link {}: {}", self, e);
            log::trace!("{}", &e);
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
        self.connection
            .connection
            .close(quinn::VarInt::from_u32(0), &[0]);
    }
}

impl fmt::Display for LinkUnicastQuic {
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

impl fmt::Debug for LinkUnicastQuic {
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
struct ListenerUnicastQuic {
    endpoint: EndPoint,
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastQuic {
    fn new(
        endpoint: EndPoint,
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> ListenerUnicastQuic {
        ListenerUnicastQuic {
            endpoint,
            active,
            signal,
            handle,
        }
    }
}

pub struct LinkManagerUnicastQuic {
    manager: NewLinkChannelSender,
    listeners: Arc<RwLock<HashMap<SocketAddr, ListenerUnicastQuic>>>,
}

impl LinkManagerUnicastQuic {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastQuic {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let dst_locator = &endpoint.locator;
        let domain = get_quic_dns(dst_locator).await?;
        let addr = get_quic_addr(dst_locator).await?;
        let host: &str = domain.as_ref().into();
        let config = &endpoint.config;

        // Initialize the QUIC connection
        let mut root_cert_store = rustls::RootCertStore::empty();

        // Read the certificates
        let f = if let Some(config) = config {
            if let Some(value) = config.get(TLS_ROOT_CA_CERTIFICATE_RAW) {
                value.as_bytes().to_vec()
            } else if let Some(value) = config.get(TLS_ROOT_CA_CERTIFICATE_FILE) {
                async_std::fs::read(value)
                    .await
                    .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        let certificates = if f.is_empty() {
            rustls_native_certs::load_native_certs()
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
                .drain(..)
                .map(|x| rustls::Certificate(x.0))
                .collect::<Vec<rustls::Certificate>>()
        } else {
            rustls_pemfile::certs(&mut BufReader::new(f.as_slice()))
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
                .drain(..)
                .map(rustls::Certificate)
                .collect::<Vec<rustls::Certificate>>()
        };
        for c in certificates.iter() {
            root_cert_store.add(c).map_err(|e| zerror!("{}", e))?;
        }

        let mut client_crypto = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();
        client_crypto.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

        let ip_addr: IpAddr = if addr.is_ipv4() {
            Ipv4Addr::new(0, 0, 0, 0).into()
        } else {
            Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0).into()
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
            .connection
            .open_bi()
            .await
            .map_err(|e| zerror!("Can not create a new QUIC link bound to {}: {}", host, e))?;

        let link = Arc::new(LinkUnicastQuic::new(
            quic_conn,
            src_addr,
            dst_locator.to_owned(),
            send,
            recv,
        ));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let config = match endpoint.config.as_ref() {
            Some(c) => c,
            None => bail!("No QUIC configuration provided"),
        };

        let locator = &endpoint.locator;
        let addr = get_quic_addr(locator).await?;

        let f = if let Some(value) = config.get(TLS_SERVER_CERTIFICATE_RAW) {
            value.as_bytes().to_vec()
        } else if let Some(value) = config.get(TLS_SERVER_CERTIFICATE_FILE) {
            async_std::fs::read(value)
                .await
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
        } else {
            bail!("No QUIC CA certificate has been provided.");
        };
        let certificates = rustls_pemfile::certs(&mut BufReader::new(f.as_slice()))
            .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
            .drain(..)
            .map(rustls::Certificate)
            .collect();

        // Private keys
        let f = if let Some(value) = config.get(TLS_SERVER_PRIVATE_KEY_RAW) {
            value.as_bytes().to_vec()
        } else if let Some(value) = config.get(TLS_SERVER_PRIVATE_KEY_FILE) {
            async_std::fs::read(value)
                .await
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
        } else {
            bail!("No QUIC CA private key has been provided.");
        };
        let private_key = rustls::PrivateKey(
            rustls_pemfile::read_all(&mut BufReader::new(f.as_slice()))
                .map_err(|e| zerror!("Invalid QUIC CA private key file: {}", e))?
                .iter()
                .filter_map(|x| match x {
                    rustls_pemfile::Item::RSAKey(k)
                    | rustls_pemfile::Item::PKCS8Key(k)
                    | rustls_pemfile::Item::ECKey(k) => Some(k.to_vec()),
                    _ => None,
                })
                .take(1)
                .next()
                .ok_or_else(|| zerror!("No QUIC CA private key has been provided."))?,
        );

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
        let (quic_endpoint, acceptor) = quinn::Endpoint::server(server_config, addr)
            .map_err(|e| zerror!("Can not create a new QUIC listener on {}: {}", addr, e))?;

        let local_addr = quic_endpoint
            .local_addr()
            .map_err(|e| zerror!("Can not create a new QUIC listener on {}: {}", addr, e))?;

        // Update the endpoint locator address
        assert!(endpoint.set_addr(&format!("{}", local_addr)));

        // Spawn the accept loop for the listener
        let active = Arc::new(AtomicBool::new(true));
        let signal = Signal::new();

        let c_active = active.clone();
        let c_signal = signal.clone();
        let c_manager = self.manager.clone();
        let c_listeners = self.listeners.clone();
        let c_addr = local_addr;
        let handle = task::spawn(async move {
            // Wait for the accept loop to terminate
            let res = accept_task(quic_endpoint, acceptor, c_active, c_signal, c_manager).await;
            zwrite!(c_listeners).remove(&c_addr);
            res
        });

        // Initialize the QuicAcceptor
        let locator = endpoint.locator.clone();
        let listener = ListenerUnicastQuic::new(endpoint, active, signal, handle);
        // Update the list of active listeners on the manager
        zwrite!(self.listeners).insert(local_addr, listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let addr = get_quic_addr(&endpoint.locator).await?;

        // Stop the listener
        let listener = zwrite!(self.listeners).remove(&addr).ok_or_else(|| {
            let e = zerror!(
                "Can not delete the QUIC listener because it has not been found: {}",
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
        let mut locators = Vec::new();
        let default_ipv4 = Ipv4Addr::new(0, 0, 0, 0);
        let default_ipv6 = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0);

        let guard = zread!(self.listeners);
        for (key, value) in guard.iter() {
            let listener_locator = &value.endpoint.locator;
            if key.ip() == default_ipv4 {
                match zenoh_util::net::get_local_addresses() {
                    Ok(ipaddrs) => {
                        for ipaddr in ipaddrs {
                            if !ipaddr.is_loopback() && !ipaddr.is_multicast() && ipaddr.is_ipv4() {
                                let mut l = Locator::new(
                                    QUIC_LOCATOR_PREFIX,
                                    &SocketAddr::new(ipaddr, key.port()),
                                );
                                l.metadata = value.endpoint.locator.metadata.clone();
                                locators.push(l);
                            }
                        }
                    }
                    Err(err) => log::error!("Unable to get local addresses : {}", err),
                }
            } else if key.ip() == default_ipv6 {
                match zenoh_util::net::get_local_addresses() {
                    Ok(ipaddrs) => {
                        for ipaddr in ipaddrs {
                            if !ipaddr.is_loopback() && !ipaddr.is_multicast() && ipaddr.is_ipv6() {
                                let mut l = Locator::new(
                                    QUIC_LOCATOR_PREFIX,
                                    &SocketAddr::new(ipaddr, key.port()),
                                );
                                l.metadata = value.endpoint.locator.metadata.clone();
                                locators.push(l);
                            }
                        }
                    }
                    Err(err) => log::error!("Unable to get local addresses : {}", err),
                }
            } else {
                locators.push(listener_locator.clone());
            }
        }
        std::mem::drop(guard);

        locators
    }
}

async fn accept_task(
    endpoint: quinn::Endpoint,
    mut acceptor: quinn::Incoming,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: NewLinkChannelSender,
) -> ZResult<()> {
    enum Action {
        Accept(quinn::NewConnection),
        Stop,
    }

    async fn accept(acceptor: &mut quinn::Incoming) -> ZResult<Action> {
        let qc = acceptor
            .next()
            .await
            .ok_or_else(|| zerror!("Can not accept QUIC connections: acceptor closed"))?;

        let conn = qc.await.map_err(|e| {
            let e = zerror!("QUIC acceptor failed: {:?}", e);
            log::warn!("{}", e);
            e
        })?;

        Ok(Action::Accept(conn))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let src_addr = endpoint
        .local_addr()
        .map_err(|e| zerror!("Can not accept QUIC connections: {}", e))?;

    // The accept future
    log::trace!("Ready to accept QUIC connections on: {:?}", src_addr);
    while active.load(Ordering::Acquire) {
        // Wait for incoming connections
        let mut quic_conn = match accept(&mut acceptor).race(stop(signal.clone())).await {
            Ok(action) => match action {
                Action::Accept(qc) => qc,
                Action::Stop => break,
            },
            Err(e) => {
                log::warn!("{} Hint: increase the system open file limit.", e);
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

        let dst_addr = quic_conn.connection.remote_address();
        log::debug!("Accepted QUIC connection on {:?}: {:?}", src_addr, dst_addr);
        // Create the new link object
        let link = Arc::new(LinkUnicastQuic::new(
            quic_conn,
            src_addr,
            Locator::new(QUIC_LOCATOR_PREFIX, &dst_addr),
            send,
            recv,
        ));

        // Communicate the new link to the initial transport manager
        if let Err(e) = manager.send_async(LinkUnicast(link)).await {
            log::error!("{}-{}: {}", file!(), line!(), e)
        }
    }

    Ok(())
}
