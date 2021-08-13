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
use super::session::SessionManager;
use super::*;
use async_std::net::SocketAddr;
use async_std::prelude::*;
use async_std::sync::Mutex as AsyncMutex;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use quinn::*;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::sync::Signal;
use zenoh_util::{zasynclock, zerror2, zread, zwrite};

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
impl LinkTrait for LinkUnicastQuic {
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
            let e = e.to_string();
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut guard = zasynclock!(self.send);
        guard.write_all(buffer).await.map_err(|e| {
            log::trace!("Write error on QUIC link {}: {}", self, e);
            let e = e.to_string();
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let mut guard = zasynclock!(self.recv);
        guard
            .read(buffer)
            .await
            .map_err(|e| {
                let e = format!("Read error on QUIC link {}: {}", self, e);
                log::trace!("{}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })?
            .ok_or_else(|| {
                let e = format!(
                    "Read error on QUIC link {}: stream {} has been closed",
                    self,
                    guard.id()
                );
                log::trace!("{}", e);
                zerror2!(ZErrorKind::IoError { descr: e })
            })
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let mut guard = zasynclock!(self.recv);
        guard.read_exact(buffer).await.map_err(|e| {
            let e = format!("Read error on QUIC link {}: {}", self, e);
            log::trace!("{}", e);
            zerror2!(ZErrorKind::IoError { descr: e })
        })
    }

    #[inline(always)]
    fn get_src(&self) -> Locator {
        Locator::Quic(LocatorQuic::SocketAddr(self.src_addr))
    }

    #[inline(always)]
    fn get_dst(&self) -> Locator {
        Locator::Quic(LocatorQuic::SocketAddr(
            self.connection.connection.remote_address(),
        ))
    }

    #[inline(always)]
    fn get_mtu(&self) -> usize {
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
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastQuic {
    fn new(
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> ListenerUnicastQuic {
        ListenerUnicastQuic {
            active,
            signal,
            handle,
        }
    }
}

pub struct LinkManagerUnicastQuic {
    manager: SessionManager,
    listeners: Arc<RwLock<HashMap<SocketAddr, ListenerUnicastQuic>>>,
}

impl LinkManagerUnicastQuic {
    pub(crate) fn new(manager: SessionManager) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastQuic {
    async fn new_link(&self, locator: &Locator, ps: Option<&LocatorProperty>) -> ZResult<Link> {
        let domain = get_quic_dns(locator).await?;
        let addr = get_quic_addr(locator).await?;
        let host: &str = domain.as_ref().into();

        // Initialize the QUIC connection
        let mut config = match ps {
            Some(prop) => {
                let quic_prop = get_quic_prop(prop)?;
                match quic_prop.client.as_ref() {
                    Some(conf) => conf.clone(),
                    None => ClientConfigBuilder::default(),
                }
            }
            None => ClientConfigBuilder::default(),
        };
        config.protocols(ALPN_QUIC_HTTP);

        let mut endpoint = Endpoint::builder();
        endpoint.default_client_config(config.build());

        let (endpoint, _) = if addr.is_ipv4() {
            endpoint.bind(&"0.0.0.0:0".parse().unwrap())
        } else {
            endpoint.bind(&"[::]:0".parse().unwrap())
        }
        .map_err(|e| {
            let e = format!("Can not create a new QUIC link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let src_addr = endpoint.local_addr().map_err(|e| {
            let e = format!("Can not create a new QUIC link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let quic_conn = endpoint
            .connect(&addr, host)
            .map_err(|e| {
                let e = format!("Can not create a new QUIC link bound to {}: {}", host, e);
                zerror2!(ZErrorKind::InvalidLink { descr: e })
            })?
            .await
            .map_err(|e| {
                let e = format!("Can not create a new QUIC link bound to {}: {}", host, e);
                zerror2!(ZErrorKind::InvalidLink { descr: e })
            })?;

        let (send, recv) = quic_conn.connection.open_bi().await.map_err(|e| {
            let e = format!("Can not create a new QUIC link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let link = Arc::new(LinkUnicastQuic::new(quic_conn, src_addr, send, recv));

        Ok(Link(link))
    }

    async fn new_listener(
        &self,
        locator: &Locator,
        ps: Option<&LocatorProperty>,
    ) -> ZResult<Locator> {
        let addr = get_quic_addr(locator).await?;

        // Verify there is a valid ServerConfig
        let prop = ps.as_ref().ok_or_else(|| {
            let e = format!(
                "Can not create a new QUIC listener on {}: no ServerConfig provided",
                addr
            );
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;
        let quic_prop = get_quic_prop(prop)?;
        let config = quic_prop.server.as_ref().ok_or_else(|| {
            let e = format!(
                "Can not create a new QUIC listener on {}: no ServerConfig provided",
                addr
            );
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Initialize the Endpoint
        let mut endpoint = Endpoint::builder();
        endpoint.listen(config.clone().build());
        let (endpoint, acceptor) = endpoint.bind(&addr).map_err(|e| {
            let e = format!("Can not create a new QUIC listener on {}: {}", addr, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let local_addr = endpoint.local_addr().map_err(|e| {
            let e = format!("Can not create a new QUIC listener on {}: {}", addr, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

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
            let res = accept_task(endpoint, acceptor, c_active, c_signal, c_manager).await;
            zwrite!(c_listeners).remove(&c_addr);
            res
        });

        // Initialize the QuicAcceptor
        let listener = ListenerUnicastQuic::new(active, signal, handle);
        // Update the list of active listeners on the manager
        zwrite!(self.listeners).insert(local_addr, listener);

        Ok(Locator::Quic(LocatorQuic::SocketAddr(local_addr)))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_quic_addr(locator).await?;

        // Stop the listener
        let listener = zwrite!(self.listeners).remove(&addr).ok_or_else(|| {
            let e = format!(
                "Can not delete the QUIC listener because it has not been found: {}",
                addr
            );
            log::trace!("{}", e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Send the stop signal
        listener.active.store(false, Ordering::Release);
        listener.signal.trigger();
        listener.handle.await
    }

    fn get_listeners(&self) -> Vec<Locator> {
        zread!(self.listeners)
            .keys()
            .map(|x| Locator::Quic(LocatorQuic::SocketAddr(*x)))
            .collect()
    }

    fn get_locators(&self) -> Vec<Locator> {
        let mut locators = vec![];
        for addr in zread!(self.listeners).keys() {
            if addr.ip() == std::net::Ipv4Addr::new(0, 0, 0, 0) {
                match zenoh_util::net::get_local_addresses() {
                    Ok(ipaddrs) => {
                        for ipaddr in ipaddrs {
                            if !ipaddr.is_loopback() && ipaddr.is_ipv4() {
                                locators.push(SocketAddr::new(ipaddr, addr.port()));
                            }
                        }
                    }
                    Err(err) => log::error!("Unable to get local addresses : {}", err),
                }
            } else {
                locators.push(*addr)
            }
        }
        locators
            .into_iter()
            .map(|x| Locator::Quic(LocatorQuic::SocketAddr(x)))
            .collect()
    }
}

async fn accept_task(
    endpoint: Endpoint,
    mut acceptor: Incoming,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: SessionManager,
) -> ZResult<()> {
    enum Action {
        Accept(NewConnection),
        Stop,
    }

    async fn accept(acceptor: &mut Incoming) -> ZResult<Action> {
        let qc = acceptor.next().await.ok_or_else(|| {
            let e = "Can not accept QUIC connections: acceptor closed".to_string();
            zerror2!(ZErrorKind::IoError { descr: e })
        })?;

        let conn = qc.await.map_err(|e| {
            let e = format!("QUIC acceptor failed: {:?}", e);
            log::warn!("{}", e);
            zerror2!(ZErrorKind::IoError { descr: e })
        })?;

        Ok(Action::Accept(conn))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let src_addr = endpoint.local_addr().map_err(|e| {
        let e = format!("Can not accept QUIC connections: {}", e);
        log::warn!("{}", e);
        zerror2!(ZErrorKind::IoError { descr: e })
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

        // Communicate the new link to the initial session manager
        manager.handle_new_link_unicast(Link(link), None).await;
    }

    Ok(())
}
