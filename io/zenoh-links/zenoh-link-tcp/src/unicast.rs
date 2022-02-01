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
use async_std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::task;
use async_std::task::JoinHandle;
use async_trait::async_trait;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::net::{IpAddr, Shutdown};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_core::Result as ZResult;
use zenoh_core::{zerror, zread, zwrite};
use zenoh_link_commons::{
    LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, NewLinkChannelSender,
};
use zenoh_protocol_core::{EndPoint, Locator};
use zenoh_sync::Signal;

use super::{
    get_tcp_addr, TCP_ACCEPT_THROTTLE_TIME, TCP_DEFAULT_MTU, TCP_LINGER_TIMEOUT, TCP_LOCATOR_PREFIX,
};

pub struct LinkUnicastTcp {
    // The underlying socket as returned from the async-std library
    socket: TcpStream,
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    src_locator: Locator,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
    dst_locator: Locator,
}

impl LinkUnicastTcp {
    fn new(socket: TcpStream, src_addr: SocketAddr, dst_addr: SocketAddr) -> LinkUnicastTcp {
        // Set the TCP nodelay option
        if let Err(err) = socket.set_nodelay(true) {
            log::warn!(
                "Unable to set NODEALY option on TCP link {} => {}: {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Set the TCP linger option
        if let Err(err) = zenoh_util::net::set_linger(
            &socket,
            Some(Duration::from_secs(
                (*TCP_LINGER_TIMEOUT).try_into().unwrap(),
            )),
        ) {
            log::warn!(
                "Unable to set LINGER option on TCP link {} => {}: {}",
                src_addr,
                dst_addr,
                err
            );
        }

        // Build the Tcp object
        LinkUnicastTcp {
            socket,
            src_addr,
            src_locator: Locator::new(TCP_LOCATOR_PREFIX, &src_addr),
            dst_addr,
            dst_locator: Locator::new(TCP_LOCATOR_PREFIX, &dst_addr),
        }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastTcp {
    async fn close(&self) -> ZResult<()> {
        log::trace!("Closing TCP link: {}", self);
        // Close the underlying TCP socket
        self.socket.shutdown(Shutdown::Both).map_err(|e| {
            let e = zerror!("TCP link shutdown {}: {:?}", self, e);
            log::trace!("{}", e);
            e.into()
        })
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        (&self.socket).write(buffer).await.map_err(|e| {
            let e = zerror!("Write error on TCP link {}: {}", self, e);
            log::trace!("{}", e);
            e.into()
        })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        (&self.socket).write_all(buffer).await.map_err(|e| {
            let e = zerror!("Write error on TCP link {}: {}", self, e);
            log::trace!("{}", e);
            e.into()
        })
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        (&self.socket).read(buffer).await.map_err(|e| {
            let e = zerror!("Read error on TCP link {}: {}", self, e);
            log::trace!("{}", e);
            e.into()
        })
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        (&self.socket).read_exact(buffer).await.map_err(|e| {
            let e = zerror!("Read error on TCP link {}: {}", self, e);
            log::trace!("{}", e);
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
        *TCP_DEFAULT_MTU
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

impl Drop for LinkUnicastTcp {
    fn drop(&mut self) {
        // Close the underlying TCP socket
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

impl fmt::Display for LinkUnicastTcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastTcp {
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
struct ListenerUnicastTcp {
    endpoint: EndPoint,
    active: Arc<AtomicBool>,
    signal: Signal,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastTcp {
    fn new(
        endpoint: EndPoint,
        active: Arc<AtomicBool>,
        signal: Signal,
        handle: JoinHandle<ZResult<()>>,
    ) -> ListenerUnicastTcp {
        ListenerUnicastTcp {
            endpoint,
            active,
            signal,
            handle,
        }
    }
}

pub struct LinkManagerUnicastTcp {
    manager: NewLinkChannelSender,
    listeners: Arc<RwLock<HashMap<SocketAddr, ListenerUnicastTcp>>>,
}

impl LinkManagerUnicastTcp {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastTcp {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let dst_addr = get_tcp_addr(&endpoint.locator).await?;

        let stream = TcpStream::connect(dst_addr)
            .await
            .map_err(|e| zerror!("Can not create a new TCP link bound to {}: {}", dst_addr, e))?;

        let src_addr = stream
            .local_addr()
            .map_err(|e| zerror!("Can not create a new TCP link bound to {}: {}", dst_addr, e))?;

        let dst_addr = stream
            .peer_addr()
            .map_err(|e| zerror!("Can not create a new TCP link bound to {}: {}", dst_addr, e))?;

        let link = Arc::new(LinkUnicastTcp::new(stream, src_addr, dst_addr));

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let addr = get_tcp_addr(&endpoint.locator).await?;

        // Bind the TCP socket
        let socket = TcpListener::bind(addr)
            .await
            .map_err(|e| zerror!("Can not create a new TCP listener on {}: {}", addr, e))?;

        let local_addr = socket
            .local_addr()
            .map_err(|e| zerror!("Can not create a new TCP listener on {}: {}", addr, e))?;

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
            let res = accept_task(socket, c_active, c_signal, c_manager).await;
            zwrite!(c_listeners).remove(&c_addr);
            res
        });

        let locator = endpoint.locator.clone();
        let listener = ListenerUnicastTcp::new(endpoint, active, signal, handle);
        // Update the list of active listeners on the manager
        zwrite!(self.listeners).insert(local_addr, listener);

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let addr = get_tcp_addr(&endpoint.locator).await?;

        // Stop the listener
        let listener = zwrite!(self.listeners).remove(&addr).ok_or_else(|| {
            let e = zerror!(
                "Can not delete the TCP listener because it has not been found: {}",
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
                    IpAddr::V4(addr) => Some(Locator::new(TCP_LOCATOR_PREFIX, &addr)),
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
                    IpAddr::V6(addr) => Some(Locator::new(TCP_LOCATOR_PREFIX, &addr)),
                    IpAddr::V4(_) => None,
                })
                .collect();
        }

        let guard = zread!(self.listeners);
        for (key, value) in guard.iter() {
            let listener_locator = &value.endpoint.locator;
            if key.ip() == default_ipv4 {
                locators.extend(V4_LOCAL_ADDRESSES.iter().map(|l| {
                    let mut l = l.clone();
                    l.metadata = listener_locator.metadata.clone();
                    l
                }))
            } else if key.ip() == default_ipv6 {
                locators.extend(V6_LOCAL_ADDRESSES.iter().map(|l| {
                    let mut l = l.clone();
                    l.metadata = listener_locator.metadata.clone();
                    l
                }))
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
        let e = zerror!("Can not accept TCP connections: {}", e);
        log::warn!("{}", e);
        e
    })?;

    log::trace!("Ready to accept TCP connections on: {:?}", src_addr);
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

        log::debug!("Accepted TCP connection on {:?}: {:?}", src_addr, dst_addr);
        // Create the new link object
        let link = Arc::new(LinkUnicastTcp::new(stream, src_addr, dst_addr));

        // Communicate the new link to the initial transport manager
        if let Err(e) = manager.send_async(LinkUnicast(link)).await {
            log::error!("{}-{}: {}", file!(), line!(), e)
        }
    }

    Ok(())
}
