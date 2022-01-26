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

use crate::{
    config::*, get_quic_addr, get_quic_dns, ALPN_QUIC_HTTP, QUIC_ACCEPT_THROTTLE_TIME,
    QUIC_DEFAULT_MTU, QUIC_LOCATOR_PREFIX,
};
use async_std::fs;
use async_std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use async_std::prelude::*;
use async_std::sync::Mutex as AsyncMutex;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use quinn::Endpoint as QuicEndPoint;
use quinn::*;
use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_core::{bail, Result as ZResult};
use zenoh_core::{zasynclock, zerror, zread, zwrite};
use zenoh_link_commons::{
    LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, NewLinkChannelSender,
};
use zenoh_protocol_core::{endpoint, locator, EndPoint, Locator};
use zenoh_sync::Signal;

pub struct LinkUnicastQuic {
    connection: NewConnection,
    src_addr: SocketAddr,
    src_locator: Locator,
    dst_locator: Locator,
    send: AsyncMutex<SendStream>,
    recv: AsyncMutex<RecvStream>,
}

impl LinkUnicastQuic {
    fn new(
        connection: NewConnection,
        src_addr: SocketAddr,
        dst_locator: Locator,
        send: SendStream,
        recv: RecvStream,
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
        self.connection.connection.close(VarInt::from_u32(0), &[0]);
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
    fn get_src(&self) -> &locator {
        &self.src_locator
    }

    #[inline(always)]
    fn get_dst(&self) -> &locator {
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
        self.connection.connection.close(VarInt::from_u32(0), &[0]);
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
        let (dst_locator, config) = endpoint.split();
        let domain = get_quic_dns(dst_locator).await?;
        let addr = get_quic_addr(dst_locator).await?;
        let host: &str = domain.as_ref().into();

        // Initialize the QUIC connection
        let mut tls_root_ca_certificate = Vec::new();
        for (key, value) in config {
            match key {
                TLS_ROOT_CA_CERTIFICATE_RAW => {
                    tls_root_ca_certificate = value.as_bytes().to_vec();
                    break;
                }
                TLS_ROOT_CA_CERTIFICATE_FILE => {
                    tls_root_ca_certificate = fs::read(value)
                        .await
                        .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?;
                    break;
                }
                _ => {}
            }
        }

        let mut config = if tls_root_ca_certificate.is_empty() {
            ClientConfigBuilder::default()
        } else {
            let ca = Certificate::from_pem(&tls_root_ca_certificate)
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?;

            let mut cc = ClientConfigBuilder::default();
            cc.protocols(ALPN_QUIC_HTTP);
            cc.add_certificate_authority(ca)
                .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?;

            cc
        };

        config.protocols(ALPN_QUIC_HTTP);

        let mut endpoint = Endpoint::builder();
        endpoint.default_client_config(config.build());

        let (endpoint, _) = if addr.is_ipv4() {
            endpoint.bind(&"0.0.0.0:0".parse().unwrap())
        } else {
            endpoint.bind(&"[::]:0".parse().unwrap())
        }
        .map_err(|e| zerror!("Can not create a new QUIC link bound to {}: {}", host, e))?;

        let src_addr = endpoint
            .local_addr()
            .map_err(|e| zerror!("Can not create a new QUIC link bound to {}: {}", host, e))?;

        let quic_conn = endpoint
            .connect(&addr, host)
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
        let (locator, config) = endpoint.split();
        let addr = get_quic_addr(locator).await?;

        let mut tls_server_private_key = Vec::new();
        let mut tls_server_certificate = Vec::new();
        for (key, value) in config {
            match key {
                TLS_SERVER_PRIVATE_KEY_RAW => tls_server_private_key = value.as_bytes().to_vec(),
                TLS_SERVER_PRIVATE_KEY_FILE => {
                    tls_server_private_key = fs::read(value)
                        .await
                        .map_err(|e| zerror!("Invalid QUIC server private key file: {}", e))?
                }
                TLS_SERVER_CERTIFICATE_RAW => {
                    tls_server_certificate = value.as_bytes().to_vec();
                }
                TLS_SERVER_CERTIFICATE_FILE => {
                    tls_server_certificate = fs::read(value)
                        .await
                        .map_err(|e| zerror!("Invalid QUIC server certificate file: {}", e))?;
                }
                _ => {}
            }
        }

        // Configure the server private key
        if tls_server_private_key.is_empty() {
            bail!(
                "Can not create a new QUIC listener on {}: missing server private key",
                addr
            )
        }
        let keys = PrivateKey::from_pem(&tls_server_private_key).unwrap();

        // Configure the server certificate
        let certs = CertificateChain::from_pem(&tls_server_certificate)
            .map_err(|e| zerror!("Invalid TLS server certificate: {}", e))?;

        let mut tc = TransportConfig::default();
        // We do not accept unidireactional streams.
        tc.max_concurrent_uni_streams(0)
            .map_err(|e| zerror!("Invalid QUIC server configuration: {}", e))?;
        // For the time being we only allow one bidirectional stream
        tc.max_concurrent_bidi_streams(1)
            .map_err(|e| zerror!("Invalid QUIC server configuration: {}", e))?;
        let mut sc = ServerConfig::default();
        sc.transport = Arc::new(tc);
        let mut sc = ServerConfigBuilder::new(sc);
        sc.protocols(ALPN_QUIC_HTTP);
        sc.certificate(certs, keys)
            .map_err(|e| zerror!("Invalid TLS server configuration: {}", e))?;

        // Initialize the Endpoint
        let mut quic_endpoint = QuicEndPoint::builder();
        quic_endpoint.listen(sc.build());
        let (quic_endpoint, acceptor) = quic_endpoint
            .bind(&addr)
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
        let locator = endpoint.locator().to_owned();
        let listener = ListenerUnicastQuic::new(endpoint, active, signal, handle);
        // Update the list of active listeners on the manager
        zwrite!(self.listeners).insert(local_addr, listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &endpoint) -> ZResult<()> {
        let addr = get_quic_addr(endpoint.locator()).await?;

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
        lazy_static::lazy_static! {
            static ref V4_LOCAL_ADDRESSES: Vec<Locator> = zenoh_util::net::get_local_addresses()
                .unwrap_or_else(|e| {
                    log::error!("Unable to get local addresses : {}", e);
                    Vec::new()
                })
                .into_iter()
                .filter_map(|addr| match addr {
                    IpAddr::V4(addr) => Some(Locator::new(QUIC_LOCATOR_PREFIX, &addr)),
                    IpAddr::V6(_) => None,
                })
                .collect();
            static ref V6_LOCAL_ADDRESSES: Vec<Locator> = zenoh_util::net::get_local_addresses()
                .unwrap_or_else(|e| {
                    log::error!("Unable to get local addresses : {}", e);
                    Vec::new()
                })
                .into_iter()
                .filter_map(|addr| match addr {
                    IpAddr::V6(addr) => Some(Locator::new(QUIC_LOCATOR_PREFIX, &addr)),
                    IpAddr::V4(_) => None,
                })
                .collect();
        }

        let guard = zread!(self.listeners);
        for (key, value) in guard.iter() {
            if key.ip() == default_ipv4 {
                let meta = value.endpoint.locator().metadata_str();
                locators.extend(V4_LOCAL_ADDRESSES.iter().map(|l| {
                    let mut l = l.clone();
                    unsafe { l.with_metadata_str_unchecked(meta) }
                    l
                }))
            } else if key.ip() == default_ipv6 {
                let meta = value.endpoint.locator().metadata_str();
                locators.extend(V6_LOCAL_ADDRESSES.iter().map(|l| {
                    let mut l = l.clone();
                    unsafe { l.with_metadata_str_unchecked(meta) }
                    l
                }))
            } else {
                locators.push(value.endpoint.locator().to_owned());
            }
        }
        std::mem::drop(guard);

        locators
    }
}

async fn accept_task(
    endpoint: Endpoint,
    mut acceptor: Incoming,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: NewLinkChannelSender,
) -> ZResult<()> {
    enum Action {
        Accept(NewConnection),
        Stop,
    }

    async fn accept(acceptor: &mut Incoming) -> ZResult<Action> {
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

    let src_addr = endpoint.local_addr().map_err(|e| {
        let e = zerror!("Can not accept QUIC connections: {}", e);
        log::warn!("{}", e);
        e
    })?;

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
                log::warn!("{}. Hint: increase the system open file limit.", e);
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
