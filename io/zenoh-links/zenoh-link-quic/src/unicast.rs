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
    config::*, get_quic_addr, ALPN_QUIC_HTTP, QUIC_ACCEPT_THROTTLE_TIME, QUIC_DEFAULT_MTU,
    QUIC_LOCATOR_PREFIX,
};
use async_std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use async_std::prelude::FutureExt;
use async_std::sync::Mutex as AsyncMutex;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::io::BufReader;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_core::{zasynclock, zread, zwrite};
use zenoh_link_commons::{
    LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, NewLinkChannelSender,
};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{bail, zerror, ZResult};
use zenoh_sync::Signal;

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
        log::trace!("Closing QUIC link: {}", self);
        // Flush the QUIC stream
        let mut guard = zasynclock!(self.send);
        if let Err(e) = guard.finish().await {
            log::trace!("Error closing QUIC stream {}: {}", self, e);
        }
        self.connection.close(quinn::VarInt::from_u32(0), &[0]);
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
        let epaddr = endpoint.address();
        let host = epaddr
            .as_str()
            .split(':')
            .next()
            .ok_or("Endpoints must be of the form quic/<address>:<port>")?;
        let epconf = endpoint.config();

        let addr = get_quic_addr(&epaddr).await?;

        // Initialize the QUIC connection
        let mut root_cert_store = rustls::RootCertStore::empty();

        // Read the certificates
        let f = if let Some(value) = epconf.get(TLS_ROOT_CA_CERTIFICATE_RAW) {
            value.as_bytes().to_vec()
        } else if let Some(value) = epconf.get(TLS_ROOT_CA_CERTIFICATE_FILE) {
            async_std::fs::read(value)
                .await
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?
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
        } else if let Some(value) = epconf.get(TLS_SERVER_CERTIFICATE_FILE) {
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
        let f = if let Some(value) = epconf.get(TLS_SERVER_PRIVATE_KEY_RAW) {
            value.as_bytes().to_vec()
        } else if let Some(value) = epconf.get(TLS_SERVER_PRIVATE_KEY_FILE) {
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
        let active = Arc::new(AtomicBool::new(true));
        let signal = Signal::new();
        let mut listeners = zwrite!(self.listeners);

        let c_active = active.clone();
        let c_signal = signal.clone();
        let c_manager = self.manager.clone();
        let c_listeners = self.listeners.clone();
        let c_addr = local_addr;
        let handle = task::spawn(async move {
            // Wait for the accept loop to terminate
            let res = accept_task(quic_endpoint, c_active, c_signal, c_manager).await;
            zwrite!(c_listeners).remove(&c_addr);
            res
        });

        // Initialize the QuicAcceptor
        let locator = endpoint.to_locator();
        let listener = ListenerUnicastQuic::new(endpoint, active, signal, handle);
        // Update the list of active listeners on the manager
        listeners.insert(local_addr, listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let epaddr = endpoint.address();

        let addr = get_quic_addr(&epaddr).await?;

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
    endpoint: quinn::Endpoint,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: NewLinkChannelSender,
) -> ZResult<()> {
    enum Action {
        Accept(quinn::Connection),
        Stop,
    }

    async fn accept(acceptor: quinn::Accept<'_>) -> ZResult<Action> {
        let qc = acceptor
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
        let quic_conn = match accept(endpoint.accept()).race(stop(signal.clone())).await {
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
        let (send, recv) = match quic_conn.accept_bi().await {
            Ok(stream) => stream,
            Err(e) => {
                log::warn!("QUIC connection has no streams: {:?}", e);
                continue;
            }
        };

        let dst_addr = quic_conn.remote_address();
        log::debug!("Accepted QUIC connection on {:?}: {:?}", src_addr, dst_addr);
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
            log::error!("{}-{}: {}", file!(), line!(), e)
        }
    }

    Ok(())
}
