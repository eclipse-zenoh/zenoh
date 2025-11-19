//
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
//

use std::{cell::UnsafeCell, collections::HashMap, fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use libc::VMADDR_PORT_ANY;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::RwLock as AsyncRwLock,
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tokio_vsock::{
    VsockAddr, VsockListener, VsockStream, VMADDR_CID_ANY, VMADDR_CID_HOST, VMADDR_CID_HYPERVISOR,
    VMADDR_CID_LOCAL,
};
use zenoh_core::{zasyncread, zasyncwrite};
use zenoh_link_commons::{
    LinkAuthId, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, NewLinkChannelSender,
};
use zenoh_protocol::{
    core::{endpoint::Address, EndPoint, Locator},
    transport::BatchSize,
};
use zenoh_result::{bail, zerror, ZResult};

use super::{VSOCK_ACCEPT_THROTTLE_TIME, VSOCK_DEFAULT_MTU, VSOCK_LOCATOR_PREFIX};

pub const VSOCK_VMADDR_CID_ANY: &str = "VMADDR_CID_ANY";
pub const VSOCK_VMADDR_CID_HYPERVISOR: &str = "VMADDR_CID_HYPERVISOR";
pub const VSOCK_VMADDR_CID_LOCAL: &str = "VMADDR_CID_LOCAL";
pub const VSOCK_VMADDR_CID_HOST: &str = "VMADDR_CID_HOST";

pub const VSOCK_VMADDR_PORT_ANY: &str = "VMADDR_PORT_ANY";

pub fn get_vsock_addr(address: Address<'_>) -> ZResult<VsockAddr> {
    let parts: Vec<&str> = address.as_str().split(':').collect();

    if parts.len() != 2 {
        bail!("Incorrect vsock address: {:?}", address);
    }

    let cid = match parts[0].to_uppercase().as_str() {
        VSOCK_VMADDR_CID_HYPERVISOR => VMADDR_CID_HYPERVISOR,
        VSOCK_VMADDR_CID_HOST => VMADDR_CID_HOST,
        VSOCK_VMADDR_CID_LOCAL => VMADDR_CID_LOCAL,
        VSOCK_VMADDR_CID_ANY => VMADDR_CID_ANY,
        "-1" => VMADDR_CID_ANY,
        _ => {
            if let Ok(cid) = parts[0].parse::<u32>() {
                cid
            } else {
                bail!("Incorrect vsock cid: {:?}", parts[0]);
            }
        }
    };

    let port = match parts[1].to_uppercase().as_str() {
        VSOCK_VMADDR_PORT_ANY => VMADDR_PORT_ANY,
        "-1" => VMADDR_PORT_ANY,
        _ => {
            if let Ok(cid) = parts[1].parse::<u32>() {
                cid
            } else {
                bail!("Incorrect vsock port: {:?}", parts[1]);
            }
        }
    };

    Ok(VsockAddr::new(cid, port))
}

pub struct LinkUnicastVsock {
    // The underlying socket as returned from the tokio library
    socket: UnsafeCell<VsockStream>,
    // The source socket address of this link (address used on the local host)
    src_addr: VsockAddr,
    src_locator: Locator,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: VsockAddr,
    dst_locator: Locator,
}

unsafe impl Sync for LinkUnicastVsock {}

impl LinkUnicastVsock {
    fn new(socket: VsockStream, src_addr: VsockAddr, dst_addr: VsockAddr) -> LinkUnicastVsock {
        // Build the vsock object
        LinkUnicastVsock {
            socket: UnsafeCell::new(socket),
            src_addr,
            src_locator: Locator::new(VSOCK_LOCATOR_PREFIX, src_addr.to_string(), "").unwrap(),
            dst_addr,
            dst_locator: Locator::new(VSOCK_LOCATOR_PREFIX, dst_addr.to_string(), "").unwrap(),
        }
    }
    #[allow(clippy::mut_from_ref)]
    fn get_mut_socket(&self) -> &mut VsockStream {
        unsafe { &mut *self.socket.get() }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastVsock {
    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing vsock link: {}", self);
        self.get_mut_socket().shutdown().await.map_err(|e| {
            let e = zerror!("vsock link shutdown {}: {:?}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        self.get_mut_socket().write(buffer).await.map_err(|e| {
            let e = zerror!("Write error on vsock link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        self.get_mut_socket().write_all(buffer).await.map_err(|e| {
            let e = zerror!("Write error on vsock link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        self.get_mut_socket().read(buffer).await.map_err(|e| {
            let e = zerror!("Read error on vsock link {}: {}", self, e);
            tracing::trace!("{}", e);
            e.into()
        })
    }

    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()> {
        let _ = self
            .get_mut_socket()
            .read_exact(buffer)
            .await
            .map_err(|e| {
                let e = zerror!("Read error on vsock link {}: {}", self, e);
                tracing::trace!("{}", e);
                e
            })?;
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
    fn get_mtu(&self) -> BatchSize {
        *VSOCK_DEFAULT_MTU
    }

    #[inline(always)]
    fn get_interface_names(&self) -> Vec<String> {
        vec!["vsock".to_string()]
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
        &LinkAuthId::Vsock
    }
}

impl fmt::Display for LinkUnicastVsock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastVsock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vsock")
            .field("src", &self.src_addr)
            .field("dst", &self.dst_addr)
            .finish()
    }
}

struct ListenerUnicastVsock {
    endpoint: EndPoint,
    token: CancellationToken,
    handle: JoinHandle<ZResult<()>>,
}

impl ListenerUnicastVsock {
    fn new(endpoint: EndPoint, token: CancellationToken, handle: JoinHandle<ZResult<()>>) -> Self {
        Self {
            endpoint,
            token,
            handle,
        }
    }

    async fn stop(&self) {
        self.token.cancel();
    }
}

pub struct LinkManagerUnicastVsock {
    manager: NewLinkChannelSender,
    listeners: Arc<AsyncRwLock<HashMap<VsockAddr, ListenerUnicastVsock>>>,
}

impl LinkManagerUnicastVsock {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: Arc::new(AsyncRwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastVsock {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let addr = get_vsock_addr(endpoint.address())?;
        if let Ok(stream) = VsockStream::connect(addr).await {
            let local_addr = stream.local_addr()?;
            let peer_addr = stream.peer_addr()?;
            let link = Arc::new(LinkUnicastVsock::new(stream, local_addr, peer_addr));
            return Ok(LinkUnicast(link));
        }

        bail!("Can not create a new vsock link bound to {}", endpoint)
    }

    async fn new_listener(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let addr = get_vsock_addr(endpoint.address())?;
        if let Ok(listener) = VsockListener::bind(addr) {
            let local_addr = listener.local_addr()?;
            // Update the endpoint locator address
            endpoint = EndPoint::new(
                endpoint.protocol(),
                format!("{local_addr}"),
                endpoint.metadata(),
                endpoint.config(),
            )?;
            let token = CancellationToken::new();

            let locator = endpoint.to_locator();

            let mut listeners = zasyncwrite!(self.listeners);

            let task = {
                let token = token.clone();
                let manager = self.manager.clone();
                let listeners = self.listeners.clone();

                async move {
                    // Wait for the accept loop to terminate
                    let res = accept_task(listener, token, manager).await;
                    zasyncwrite!(listeners).remove(&addr);
                    res
                }
            };
            let handle = zenoh_runtime::ZRuntime::Acceptor.spawn(task);

            let listener = ListenerUnicastVsock::new(endpoint, token, handle);
            // Update the list of active listeners on the manager
            listeners.insert(addr, listener);
            return Ok(locator);
        }

        bail!("Can not create a new vsock listener bound to {}", endpoint)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let addr = get_vsock_addr(endpoint.address())?;

        let listener = zasyncwrite!(self.listeners).remove(&addr).ok_or_else(|| {
            zerror!(
                "Can not delete the listener because it has not been found: {}",
                addr
            )
        })?;

        // Send the stop signal
        listener.stop().await;
        listener.handle.await?
    }

    async fn get_listeners(&self) -> Vec<EndPoint> {
        zasyncread!(self.listeners)
            .values()
            .map(|x| x.endpoint.clone())
            .collect()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        zasyncread!(self.listeners)
            .values()
            .map(|x| x.endpoint.to_locator())
            .collect()
    }
}

async fn accept_task(
    mut socket: VsockListener,
    token: CancellationToken,
    manager: NewLinkChannelSender,
) -> ZResult<()> {
    async fn accept(socket: &mut VsockListener) -> ZResult<(VsockStream, VsockAddr)> {
        let res = socket.accept().await.map_err(|e| zerror!(e))?;
        Ok(res)
    }

    let src_addr = socket.local_addr().map_err(|e| {
        let e = zerror!("Can not accept vsock connections: {}", e);
        tracing::warn!("{}", e);
        e
    })?;

    tracing::trace!("Ready to accept vsock connections on: {:?}", src_addr);
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            res = accept(&mut socket) => {
                match res {
                    Ok((stream, dst_addr)) => {
                        tracing::debug!("Accepted vsock connection on {:?}: {:?}", src_addr, dst_addr);
                        // Create the new link object
                        let link = Arc::new(LinkUnicastVsock::new(stream, src_addr, dst_addr));

                        // Communicate the new link to the initial transport manager
                        if let Err(e) = manager.send_async(LinkUnicast(link)).await {
                            tracing::error!("{}-{}: {}", file!(), line!(), e)
                        }
                    },
                    Err(e) => {
                        tracing::warn!("{}. Hint: increase the system open file limit.", e);
                        // Throttle the accept loop upon an error
                        // NOTE: This might be due to various factors. However, the most common case is that
                        //       the process has reached the maximum number of open files in the system. On
                        //       Linux systems this limit can be changed by using the "ulimit" command line
                        //       tool. In case of systemd-based systems, this can be changed by using the
                        //       "sysctl" command line tool.
                        tokio::time::sleep(Duration::from_micros(*VSOCK_ACCEPT_THROTTLE_TIME)).await;
                    }

                }
            }
        };
    }

    Ok(())
}
