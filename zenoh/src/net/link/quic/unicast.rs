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
use super::EndPoint as ZEndPoint;
use super::*;
use crate::net::transport::TransportManager;
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_core::Result as ZResult;
use zenoh_core::{zasynclock, zerror, zread, zwrite};
use zenoh_sync::Signal;

pub struct LinkUnicastQuic {
    connection: NewConnection,
    src_addr: SocketAddr,
    send: AsyncMutex<SendStream>,
    recv: AsyncMutex<RecvStream>,
}

impl LinkUnicastQuic {
    fn new(
        connection: NewConnection,
        src_addr: SocketAddr,
        send: SendStream,
        recv: RecvStream,
    ) -> LinkUnicastQuic {
        // Build the Quic object
        LinkUnicastQuic {
            connection,
            src_addr,
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
    fn get_src(&self) -> Locator {
        Locator {
            address: LocatorAddress::Quic(LocatorQuic::SocketAddr(self.src_addr)),
            metadata: None,
        }
    }

    #[inline(always)]
    fn get_dst(&self) -> Locator {
        Locator {
            address: LocatorAddress::Quic(LocatorQuic::SocketAddr(
                self.connection.connection.remote_address(),
            )),
            metadata: None,
        }
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
    manager: TransportManager,
    listeners: Arc<RwLock<HashMap<SocketAddr, ListenerUnicastQuic>>>,
}

impl LinkManagerUnicastQuic {
    pub(crate) fn new(manager: TransportManager) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastQuic {
    async fn new_link(&self, endpoint: ZEndPoint) -> ZResult<LinkUnicast> {
        let domain = get_quic_dns(&endpoint.locator.address).await?;
        let addr = get_quic_addr(&endpoint.locator.address).await?;
        let host: &str = domain.as_ref().into();

        // Initialize the QUIC connection
        let bytes = match endpoint.config.as_ref() {
            Some(config) => match config.get(TLS_ROOT_CA_CERTIFICATE_RAW) {
                Some(tls_ca_certificate) => tls_ca_certificate.as_bytes().to_vec(),
                None => match config.get(TLS_ROOT_CA_CERTIFICATE_FILE) {
                    Some(tls_ca_certificate) => fs::read(tls_ca_certificate)
                        .await
                        .map_err(|e| zerror!("Invalid QUIC CA certificate file: {}", e))?,
                    None => vec![],
                },
            },
            None => vec![],
        };

        let mut config = if bytes.is_empty() {
            ClientConfigBuilder::default()
        } else {
            let ca = Certificate::from_pem(&bytes)
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

        let link = Arc::new(LinkUnicastQuic::new(quic_conn, src_addr, send, recv));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let addr = get_quic_addr(&endpoint.locator.address).await?;

        // Verify there is a valid ServerConfig
        let config = endpoint.config.as_ref().ok_or_else(|| {
            zerror!(
                "Can not create a new QUIC listener on {}: no ServerConfig provided",
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
                None => bail!(
                    "Can not create a new QUIC listener on {}. ServerConfig not provided: {}.",
                    addr,
                    TLS_SERVER_PRIVATE_KEY_FILE
                ),
            },
        };
        let keys = PrivateKey::from_pem(bytes.as_slice()).unwrap();

        // Configure the server certificate
        let bytes = match config.get(TLS_SERVER_CERTIFICATE_RAW) {
            Some(tls_server_certificate) => tls_server_certificate.as_bytes().to_vec(),
            None => match config.get(TLS_SERVER_CERTIFICATE_FILE) {
                Some(tls_server_certificate) => fs::read(tls_server_certificate)
                    .await
                    .map_err(|e| zerror!("Invalid TLS server certificate file: {}", e))?,
                None => {
                    bail!(
                        "Can not create a new QUIC listener on {}. ServerConfig not provided: {}.",
                        addr,
                        TLS_SERVER_CERTIFICATE_FILE
                    )
                }
            },
        };
        let certs = CertificateChain::from_pem(bytes.as_slice())
            .map_err(|e| zerror!("Invalid TLS server certificate file: {}", e))?;

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
        endpoint.locator.address = LocatorAddress::Quic(LocatorQuic::SocketAddr(local_addr));

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
        let addr = get_quic_addr(&endpoint.locator.address).await?;

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
        let default_ipv4 = Ipv4Addr::new(0, 0, 0, 0);
        let default_ipv6 = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0);

        for (key, value) in zread!(self.listeners).iter() {
            if key.ip() == default_ipv4 {
                match zenoh_util::net::get_local_addresses() {
                    Ok(ipaddrs) => {
                        for ipaddr in ipaddrs {
                            if !ipaddr.is_loopback() && !ipaddr.is_multicast() && ipaddr.is_ipv4() {
                                locators.push((
                                    SocketAddr::new(ipaddr, key.port()),
                                    value.endpoint.locator.metadata.clone(),
                                ));
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
                                locators.push((
                                    SocketAddr::new(ipaddr, key.port()),
                                    value.endpoint.locator.metadata.clone(),
                                ));
                            }
                        }
                    }
                    Err(err) => log::error!("Unable to get local addresses : {}", err),
                }
            } else {
                locators.push((*key, value.endpoint.locator.metadata.clone()));
            }
        }

        locators
            .into_iter()
            .map(|(addr, metadata)| Locator {
                address: LocatorAddress::Quic(LocatorQuic::SocketAddr(addr)),
                metadata,
            })
            .collect()
    }
}

async fn accept_task(
    endpoint: Endpoint,
    mut acceptor: Incoming,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: TransportManager,
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
        let link = Arc::new(LinkUnicastQuic::new(quic_conn, src_addr, send, recv));

        // Communicate the new link to the initial transport manager
        manager.handle_new_link_unicast(LinkUnicast(link)).await;
    }

    Ok(())
}
