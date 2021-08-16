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
pub use async_rustls::rustls::*;
pub use async_rustls::webpki::*;
use async_rustls::{TlsAcceptor, TlsConnector, TlsStream};
use async_std::net::{SocketAddr, TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::sync::Mutex as AsyncMutex;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::net::Shutdown;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::sync::Signal;
use zenoh_util::{zerror2, zread, zwrite};

pub struct LinkUnicastTls {
    // The underlying socket as returned from the async-rustls library
    // NOTE: TlsStream requires &mut for read and write operations. This means
    //       that concurrent reads and writes are not possible. To achieve that,
    //       we use an UnsafeCell for interior mutability. Using an UnsafeCell
    //       is safe in our case since the transmission and reception logic
    //       already ensures that no concurrent reads or writes can happen on
    //       the same stream: there is only one task at the time that writes on
    //       the stream and only one task at the time that reads from the stream.
    inner: UnsafeCell<TlsStream<TcpStream>>,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    // The destination socket address of this link (address used on the local host)
    dst_addr: SocketAddr,
    // Make sure there are no concurrent read or writes
    write_mtx: AsyncMutex<()>,
    read_mtx: AsyncMutex<()>,
}

unsafe impl Send for LinkUnicastTls {}
unsafe impl Sync for LinkUnicastTls {}

impl LinkUnicastTls {
    fn new(
        socket: TlsStream<TcpStream>,
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
    ) -> LinkUnicastTls {
        let (tcp_stream, _) = socket.get_ref();
        // Set the TLS nodelay option
        if let Err(err) = tcp_stream.set_nodelay(true) {
            log::warn!(
                "Unable to set NODEALY option on TLS link {} => {} : {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Set the TLS linger option
        if let Err(err) = zenoh_util::net::set_linger(
            tcp_stream,
            Some(Duration::from_secs(
                (*TLS_LINGER_TIMEOUT).try_into().unwrap(),
            )),
        ) {
            log::warn!(
                "Unable to set LINGER option on TLS link {} => {} : {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Build the Tls object
        LinkUnicastTls {
            inner: UnsafeCell::new(socket),
            src_addr,
            dst_addr,
            write_mtx: AsyncMutex::new(()),
            read_mtx: AsyncMutex::new(()),
        }
    }

    // NOTE: It is safe to suppress Clippy warning since no concurrent reads
    //       or concurrent writes will ever happen. The read_mtx and write_mtx
    //       are respectively acquired in any read and write operation.
    #[allow(clippy::mut_from_ref)]
    fn get_sock_mut(&self) -> &mut TlsStream<TcpStream> {
        unsafe { &mut *self.inner.get() }
    }
}

#[async_trait]
impl LinkTrait for LinkUnicastTls {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing TLS link: {}", self);
        // Flush the TLS stream
        let _guard = zasynclock!(self.write_mtx);
        let tls_stream = self.get_sock_mut();
        let res = tls_stream.flush().await;
        log::trace!("TLS link flush {}: {:?}", self, res);
        // Close the underlying TCP stream
        let (tcp_stream, _) = tls_stream.get_ref();
        let res = tcp_stream.shutdown(Shutdown::Both);
        log::trace!("TLS link shutdown {}: {:?}", self, res);
        res.map_err(|e| {
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string(),
            })
        })
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        let _guard = zasynclock!(self.write_mtx);
        self.get_sock_mut().write(buffer).await.map_err(|e| {
            log::trace!("Write error on TLS link {}: {}", self, e);
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let _guard = zasynclock!(self.write_mtx);
        self.get_sock_mut().write_all(buffer).await.map_err(|e| {
            log::trace!("Write error on TLS link {}: {}", self, e);
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let _guard = zasynclock!(self.read_mtx);
        self.get_sock_mut().read(buffer).await.map_err(|e| {
            log::trace!("Read error on TLS link {}: {}", self, e);
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let _guard = zasynclock!(self.read_mtx);
        self.get_sock_mut().read_exact(buffer).await.map_err(|e| {
            log::trace!("Read error on TLS link {}: {}", self, e);
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })
    }

    #[inline(always)]
    fn get_src(&self) -> Locator {
        Locator::Tls(LocatorTls::SocketAddr(self.src_addr))
    }

    #[inline(always)]
    fn get_dst(&self) -> Locator {
        Locator::Tls(LocatorTls::SocketAddr(self.dst_addr))
    }

    #[inline(always)]
    fn get_mtu(&self) -> usize {
        *TLS_DEFAULT_MTU
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

impl Drop for LinkUnicastTls {
    fn drop(&mut self) {
        // Close the underlying TCP stream
        let (tcp_stream, _) = self.get_sock_mut().get_ref();
        let _ = tcp_stream.shutdown(Shutdown::Both);
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

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerUnicastTls {
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastTls {
    fn new(
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> ListenerUnicastTls {
        ListenerUnicastTls {
            active,
            signal,
            handle,
        }
    }
}

pub struct LinkManagerUnicastTls {
    manager: SessionManager,
    listeners: Arc<RwLock<HashMap<SocketAddr, ListenerUnicastTls>>>,
}

impl LinkManagerUnicastTls {
    pub(crate) fn new(manager: SessionManager) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastTls {
    async fn new_link(&self, locator: &Locator, ps: Option<&LocatorProperty>) -> ZResult<Link> {
        let domain = get_tls_dns(locator).await?;
        let addr = get_tls_addr(locator).await?;
        let host: &str = domain.as_ref().into();

        // Initialize the TcpStream
        let tcp_stream = TcpStream::connect(addr).await.map_err(|e| {
            let e = format!("Can not create a new TLS link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::Other { descr: e })
        })?;

        let src_addr = tcp_stream.local_addr().map_err(|e| {
            let e = format!("Can not create a new TLS link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let dst_addr = tcp_stream.peer_addr().map_err(|e| {
            let e = format!("Can not create a new TLS link bound to {}: {}", host, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Initialize the TLS stream
        let config = match ps {
            Some(prop) => {
                let tls_prop = get_tls_prop(prop)?;
                match tls_prop.client.as_ref() {
                    Some(conf) => conf.clone(),
                    None => Arc::new(ClientConfig::new()),
                }
            }
            None => Arc::new(ClientConfig::new()),
        };
        let connector = TlsConnector::from(config);
        let tls_stream = connector
            .connect(domain.as_ref(), tcp_stream)
            .await
            .map_err(|e| {
                let e = format!("Can not create a new TLS link bound to {}: {}", host, e);
                zerror2!(ZErrorKind::InvalidLink { descr: e })
            })?;
        let tls_stream = TlsStream::Client(tls_stream);

        let link = Arc::new(LinkUnicastTls::new(tls_stream, src_addr, dst_addr));

        Ok(Link(link))
    }

    async fn new_listener(
        &self,
        locator: &Locator,
        ps: Option<&LocatorProperty>,
    ) -> ZResult<Locator> {
        let addr = get_tls_addr(locator).await?;

        // Verify there is a valid ServerConfig
        let prop = ps.as_ref().ok_or_else(|| {
            let e = format!(
                "Can not create a new TLS listener on {}: no ServerConfig provided",
                addr
            );
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;
        let tls_prop = get_tls_prop(prop)?;
        let config = tls_prop.server.as_ref().ok_or_else(|| {
            let e = format!(
                "Can not create a new TLS listener on {}: no ServerConfig provided",
                addr
            );
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Initialize the TcpListener
        let socket = TcpListener::bind(addr).await.map_err(|e| {
            let e = format!("Can not create a new TLS listener on {}: {}", addr, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        let local_addr = socket.local_addr().map_err(|e| {
            let e = format!("Can not create a new TLS listener on {}: {}", addr, e);
            zerror2!(ZErrorKind::InvalidLink { descr: e })
        })?;

        // Initialize the TlsAcceptor
        let acceptor = TlsAcceptor::from(config.clone());
        let active = Arc::new(AtomicBool::new(true));
        let signal = Signal::new();

        // Spawn the accept loop for the listener
        let c_active = active.clone();
        let c_signal = signal.clone();
        let c_manager = self.manager.clone();
        let c_listeners = self.listeners.clone();
        let c_addr = local_addr;
        let handle = task::spawn(async move {
            // Wait for the accept loop to terminate
            let res = accept_task(socket, acceptor, c_active, c_signal, c_manager).await;
            zwrite!(c_listeners).remove(&c_addr);
            res
        });

        let listener = ListenerUnicastTls::new(active, signal, handle);
        // Update the list of active listeners on the manager
        zwrite!(self.listeners).insert(local_addr, listener);

        Ok(Locator::Tls(LocatorTls::SocketAddr(local_addr)))
    }

    async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let addr = get_tls_addr(locator).await?;

        // Stop the listener
        let listener = zwrite!(self.listeners).remove(&addr).ok_or_else(|| {
            let e = format!(
                "Can not delete the TLS listener because it has not been found: {}",
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
            .map(|x| Locator::Tls(LocatorTls::SocketAddr(*x)))
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
            .map(|x| Locator::Tls(LocatorTls::SocketAddr(x)))
            .collect()
    }
}

async fn accept_task(
    socket: TcpListener,
    acceptor: TlsAcceptor,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: SessionManager,
) -> ZResult<()> {
    enum Action {
        Accept((TcpStream, SocketAddr)),
        Stop,
    }

    async fn accept(socket: &TcpListener) -> ZResult<Action> {
        let res = socket.accept().await.map_err(|e| {
            zerror2!(ZErrorKind::IoError {
                descr: e.to_string()
            })
        })?;
        Ok(Action::Accept(res))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let src_addr = socket.local_addr().map_err(|e| {
        let e = format!("Can not accept TLS connections: {}", e);
        log::warn!("{}", e);
        zerror2!(ZErrorKind::IoError { descr: e })
    })?;

    log::trace!("Ready to accept TLS connections on: {:?}", src_addr);
    while active.load(Ordering::Acquire) {
        // Wait for incoming connections
        let (tcp_stream, dst_addr) = match accept(&socket).race(stop(signal.clone())).await {
            Ok(action) => match action {
                Action::Accept((tcp_stream, dst_addr)) => (tcp_stream, dst_addr),
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
                task::sleep(Duration::from_micros(*TLS_ACCEPT_THROTTLE_TIME)).await;
                continue;
            }
        };
        // Accept the TLS connection
        let tls_stream = match acceptor.accept(tcp_stream).await {
            Ok(stream) => TlsStream::Server(stream),
            Err(e) => {
                let e = format!("Can not accept TLS connection: {}", e);
                log::warn!("{}", e);
                continue;
            }
        };

        log::debug!("Accepted TLS connection on {:?}: {:?}", src_addr, dst_addr);
        // Create the new link object
        let link = Arc::new(LinkUnicastTls::new(tls_stream, src_addr, dst_addr));

        // Communicate the new link to the initial session manager
        manager.handle_new_link_unicast(Link(link), None).await;
    }

    Ok(())
}
