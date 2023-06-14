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

use async_std::prelude::*;
use async_std::sync::Mutex as AsyncMutex;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use futures_util::stream::SplitSink;
use futures_util::stream::SplitStream;
use futures_util::SinkExt;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use zenoh_core::{zasynclock, zread, zwrite};
use zenoh_link_commons::{
    LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, NewLinkChannelSender,
};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{bail, zerror, ZResult};
use zenoh_sync::Signal;

use super::{get_ws_addr, get_ws_url, TCP_ACCEPT_THROTTLE_TIME, WS_DEFAULT_MTU, WS_LOCATOR_PREFIX};

pub struct LinkUnicastWs {
    // The inbound message stream as returned from the futures_util::stream::StreamExt::split method
    recv: AsyncMutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
    // // The outbound message stream as returned from the futures_util::stream::StreamExt::split method
    send: AsyncMutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    src_locator: Locator,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
    dst_locator: Locator,
    // The leftovers if reading less than what available on the web socket.
    leftovers: AsyncMutex<Option<(Vec<u8>, usize, usize)>>,
}

impl LinkUnicastWs {
    fn new(
        socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
    ) -> LinkUnicastWs {
        // Set the TCP nodelay option
        if let Err(err) = get_stream(&socket).set_nodelay(true) {
            log::warn!(
                "Unable to set NODEALY option on TCP link {} => {}: {}",
                src_addr,
                dst_addr,
                err
            );
        }

        let (send, recv) = socket.split();
        let send = AsyncMutex::new(send);
        let recv = AsyncMutex::new(recv);

        // Build the LinkUnicastWs object
        LinkUnicastWs {
            // socket,
            recv,
            send,
            src_addr,
            src_locator: Locator::new(WS_LOCATOR_PREFIX, src_addr.to_string(), "").unwrap(),
            dst_addr,
            dst_locator: Locator::new(WS_LOCATOR_PREFIX, dst_addr.to_string(), "").unwrap(),
            leftovers: AsyncMutex::new(None),
        }
    }

    async fn recv(&self) -> ZResult<Vec<u8>> {
        let mut guard = zasynclock!(self.recv);

        match guard.next().await {
            Some(msg) => match msg {
                Ok(msg) => match msg {
                    Message::Binary(ws_bytes) => Ok(ws_bytes),
                    Message::Ping(_) => bail!(
                        "Received wrong message type (Ping) from WebSocket link {}",
                        self
                    ),
                    Message::Pong(_) => bail!(
                        "Received wrong message type (Pong) from WebSocket link {}",
                        self
                    ),
                    Message::Text(_) => bail!(
                        "Received wrong message type (Text) from WebSocket link {}",
                        self
                    ),
                    Message::Frame(_) => bail!(
                        "Received wrong message type (Frame) from WebSocket link {}",
                        self
                    ),
                    Message::Close(_) => {
                        bail!("Receiving from an already closed WS Link: {}", self)
                    }
                },
                Err(e) => bail!("Error when receiving from WebSocket link {}: {}", self, e),
            },
            None => bail!("Error when receiving from WebSocket link {}: None", self),
        }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastWs {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing WebSocket link: {}", self);
        let mut guard = zasynclock!(self.send);
        // Close the underlying TCP socket
        guard.close().await.map_err(|e| {
            let e = zerror!("WebSocket link shutdown {}: {:?}", self, e);
            log::trace!("{}", e);
            e.into()
        })
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        let mut guard = zasynclock!(self.send);
        let msg = buffer.into();

        guard.send(msg).await.map_err(|e| {
            let e = zerror!("Write error on WebSocket link {}: {}", self, e);
            log::trace!("{}", e);
            e
        })?;

        Ok(buffer.len())
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut written: usize = 0;
        while written < buffer.len() {
            written += self.write(&buffer[written..]).await?;
        }
        Ok(())
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let mut leftovers_guard = zasynclock!(self.leftovers);

        let (slice, start, len) = match leftovers_guard.take() {
            Some(tuple) => tuple,
            None => {
                let ws_bytes = self.recv().await?;
                let ws_size = ws_bytes.len();
                (ws_bytes, 0usize, ws_size)
            }
        };

        // Copy the read bytes into the target buffer
        let len_min = (len - start).min(buffer.len());
        let end = start + len_min;
        buffer[0..len_min].copy_from_slice(&slice[start..end]);
        if end < len {
            // Store the leftover
            *leftovers_guard = Some((slice, end, len));
        } else {
            // Remove any leftover
            *leftovers_guard = None;
        }
        Ok(len_min)
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let mut read: usize = 0;
        while read < buffer.len() {
            let n = self.read(&mut buffer[read..]).await?;
            read += n;
        }
        Ok(())
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
        *WS_DEFAULT_MTU
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        true
    }

    #[inline(always)]
    fn is_streamed(&self) -> bool {
        false
    }
}

impl Drop for LinkUnicastWs {
    fn drop(&mut self) {
        task::block_on(async {
            let mut guard = zasynclock!(self.send);
            // Close the underlying TCP socket
            guard.close().await.unwrap_or_else(|e| {
                log::warn!("`LinkUnicastWs::Drop` error when closing WebSocket {}", e)
            });
        })
    }
}

impl fmt::Display for LinkUnicastWs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastWs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tcp")
            .field("src", &self.src_addr)
            .field("dst", &self.dst_addr)
            .finish()
    }
}

/*************************************/
/*          LISTENER                 */
/*************************************/
struct ListenerUnicastWs {
    endpoint: EndPoint,
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastWs {
    fn new(
        endpoint: EndPoint,
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> ListenerUnicastWs {
        ListenerUnicastWs {
            endpoint,
            active,
            signal,
            handle,
        }
    }
}

pub struct LinkManagerUnicastWs {
    manager: NewLinkChannelSender,
    listeners: Arc<RwLock<HashMap<SocketAddr, ListenerUnicastWs>>>,
}

impl LinkManagerUnicastWs {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastWs {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let dst_url = get_ws_url(endpoint.address()).await?;

        let (stream, _) = tokio_tungstenite::connect_async(&dst_url)
            .await
            .map_err(|e| {
                zerror!(
                    "Can not create a new WebSocket link bound to {}: {}",
                    dst_url,
                    e
                )
            })?;

        let src_addr = get_stream(&stream).local_addr().map_err(|e| {
            zerror!(
                "Can not create a new WebSocket link bound to {}: {}",
                dst_url,
                e
            )
        })?;

        let dst_addr = get_stream(&stream).peer_addr().map_err(|e| {
            zerror!(
                "Can not create a new WebSocket link bound to {}: {}",
                dst_url,
                e
            )
        })?;

        let link = Arc::new(LinkUnicastWs::new(stream, src_addr, dst_addr));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let addr = get_ws_addr(endpoint.address()).await?;

        // Bind the TCP socket
        let socket = TcpListener::bind(addr).await.map_err(|e| {
            zerror!(
                "Can not create a new TCP (WebSocket) listener on {}: {}",
                addr,
                e
            )
        })?;

        let local_addr = socket.local_addr().map_err(|e| {
            zerror!(
                "Can not create a new TCP (WebSocket) listener on {}: {}",
                addr,
                e
            )
        })?;

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

        let c_active = active.clone();
        let c_signal = signal.clone();
        let c_manager = self.manager.clone();
        let c_listeners = self.listeners.clone();
        let c_addr = local_addr;
        let handle = task::spawn(async move {
            // Wait for the accept loop to terminate
            let res = accept_task(socket, c_active, c_signal, c_manager).await;
            zwrite!(c_listeners).remove(&c_addr);
            res
        });

        let locator = endpoint.to_locator();
        let listener = ListenerUnicastWs::new(endpoint, active, signal, handle);
        // Update the list of active listeners on the manager
        zwrite!(self.listeners).insert(local_addr, listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let addr = get_ws_addr(endpoint.address()).await?;

        // Stop the listener
        let listener = zwrite!(self.listeners).remove(&addr).ok_or_else(|| {
            let e = zerror!(
                "Can not delete the TCP (WebSocket) listener because it has not been found: {}",
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
            .map(|l| l.endpoint.clone())
            .collect()
    }

    fn get_locators(&self) -> Vec<Locator> {
        let mut locators = Vec::new();
        let default_ipv4 = Ipv4Addr::UNSPECIFIED;
        let default_ipv6 = Ipv6Addr::UNSPECIFIED;

        let guard = zread!(self.listeners);
        for (key, value) in guard.iter() {
            let listener_locator = value.endpoint.to_locator();
            if key.ip() == default_ipv4 {
                match zenoh_util::net::get_local_addresses() {
                    Ok(ipaddrs) => {
                        for ipaddr in ipaddrs {
                            if !ipaddr.is_loopback() && !ipaddr.is_multicast() && ipaddr.is_ipv4() {
                                let l = Locator::new(
                                    WS_LOCATOR_PREFIX,
                                    SocketAddr::new(ipaddr, key.port()).to_string(),
                                    value.endpoint.metadata(),
                                )
                                .unwrap();
                                locators.push(l);
                            }
                        }
                    }
                    Err(err) => log::error!("Unable to get local addresses: {}", err),
                }
            } else if key.ip() == default_ipv6 {
                match zenoh_util::net::get_local_addresses() {
                    Ok(ipaddrs) => {
                        for ipaddr in ipaddrs {
                            if !ipaddr.is_loopback() && !ipaddr.is_multicast() && ipaddr.is_ipv6() {
                                let l = Locator::new(
                                    WS_LOCATOR_PREFIX,
                                    SocketAddr::new(ipaddr, key.port()).to_string(),
                                    value.endpoint.metadata(),
                                )
                                .unwrap();
                                locators.push(l);
                            }
                        }
                    }
                    Err(err) => log::error!("Unable to get local addresses: {}", err),
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
    socket: TcpListener,
    active: Arc<AtomicBool>,
    signal: Signal,
    manager: NewLinkChannelSender,
) -> ZResult<()> {
    enum Action {
        Accept((TcpStream, SocketAddr)),
        Stop,
    }

    async fn accept(socket: &TcpListener) -> ZResult<Action> {
        let res = socket.accept().await.map_err(|e| zerror!(e))?;
        Ok(Action::Accept(res))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let src_addr = socket.local_addr().map_err(|e| {
        let e = zerror!("Can not accept TCP (WebSocket) connections: {}", e);
        log::warn!("{}", e);
        e
    })?;

    log::trace!(
        "Ready to accept TCP (WebSocket) connections on: {:?}",
        src_addr
    );
    while active.load(Ordering::Acquire) {
        // Wait for incoming connections
        let (stream, dst_addr) = match accept(&socket).race(stop(signal.clone())).await {
            Ok(action) => match action {
                Action::Accept((stream, addr)) => (stream, addr),
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
                task::sleep(Duration::from_micros(*TCP_ACCEPT_THROTTLE_TIME)).await;
                continue;
            }
        };

        log::debug!(
            "Accepted TCP (WebSocket) connection on {:?}: {:?}",
            src_addr,
            dst_addr
        );

        let stream = accept_async(MaybeTlsStream::Plain(stream))
            .await
            .map_err(|e| {
                let e = zerror!("Error when creating the WebSocket session: {}", e);
                log::trace!("{}", e);
                e
            })?;
        // Create the new link object
        let link = Arc::new(LinkUnicastWs::new(stream, src_addr, dst_addr));

        // Communicate the new link to the initial transport manager
        if let Err(e) = manager.send_async(LinkUnicast(link)).await {
            log::error!("{}-{}: {}", file!(), line!(), e)
        }
    }

    Ok(())
}

fn get_stream(ws_stream: &WebSocketStream<MaybeTlsStream<TcpStream>>) -> &TcpStream {
    match ws_stream.get_ref() {
        MaybeTlsStream::Plain(s) => s,
        // This two are available only if the TLS features are enabled for
        // tokio_tungstenite.
        // MaybeTlsStream::NativeTls(s) => s.get_ref().get_ref(),
        // MaybeTlsStream::Rustls(s) => s.get_ref().0
        _ => panic!(),
    }
}
