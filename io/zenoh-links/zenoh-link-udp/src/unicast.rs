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
use std::{
    collections::HashMap,
    fmt,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use async_trait::async_trait;
use tokio::{net::UdpSocket, sync::Mutex as AsyncMutex};
use tokio_util::sync::CancellationToken;
use zenoh_core::{zasynclock, zlock};
use zenoh_link_commons::{
    get_ip_interface_names, parse_dscp, set_dscp, ConstructibleLinkManagerUnicast, LinkAuthId,
    LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, ListenersUnicastIP,
    NewLinkChannelSender, BIND_INTERFACE, BIND_SOCKET,
};
use zenoh_protocol::{
    core::{Address, EndPoint, Locator},
    transport::BatchSize,
};
use zenoh_result::{bail, zerror, Error as ZError, ZResult};
use zenoh_sync::Mvar;

use super::{
    get_udp_addrs, socket_addr_to_udp_locator, UDP_ACCEPT_THROTTLE_TIME, UDP_DEFAULT_MTU,
    UDP_MAX_MTU,
};
use crate::pktinfo;

type LinkHashMap = Arc<Mutex<HashMap<(SocketAddr, SocketAddr), Weak<LinkUnicastUdpUnconnected>>>>;
type LinkInput = (Vec<u8>, usize);
type LinkLeftOver = (Vec<u8>, usize, usize);

struct LinkUnicastUdpConnected {
    socket: Arc<UdpSocket>,
}

impl LinkUnicastUdpConnected {
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        self.socket
            .recv(buffer)
            .await
            .map_err(|e| zerror!(e).into())
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        self.socket
            .send(buffer)
            .await
            .map_err(|e| zerror!(e).into())
    }

    async fn close(&self) -> ZResult<()> {
        Ok(())
    }
}

struct LinkUnicastUdpUnconnected {
    socket: Weak<UdpSocket>,
    links: LinkHashMap,
    input: Mvar<LinkInput>,
    leftover: AsyncMutex<Option<LinkLeftOver>>,
}

impl LinkUnicastUdpUnconnected {
    async fn received(&self, buffer: Vec<u8>, len: usize) {
        self.input.put((buffer, len)).await;
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        let mut guard = zasynclock!(self.leftover);
        let (slice, start, len) = match guard.take() {
            Some(tuple) => tuple,
            None => {
                let (slice, len) = self.input.take().await;
                (slice, 0, len)
            }
        };
        // Copy the read bytes into the target buffer
        let len_min = (len - start).min(buffer.len());
        let end = start + len_min;
        buffer[0..len_min].copy_from_slice(&slice[start..end]);
        if end < len {
            // Store the leftover
            *guard = Some((slice, end, len));
        } else {
            // Recycle the buffer
            drop(slice);
        }
        // Return the amount read
        Ok(len_min)
    }

    async fn write(&self, buffer: &[u8], dst_addr: SocketAddr) -> ZResult<usize> {
        match self.socket.upgrade() {
            Some(socket) => socket
                .send_to(buffer, &dst_addr)
                .await
                .map_err(|e| zerror!(e).into()),
            None => bail!("UDP listener has been dropped"),
        }
    }

    async fn close(&self, src_addr: SocketAddr, dst_addr: SocketAddr) -> ZResult<()> {
        // Delete the link from the list of links
        zlock!(self.links).remove(&(src_addr, dst_addr));
        Ok(())
    }
}

enum LinkUnicastUdpVariant {
    Connected(LinkUnicastUdpConnected),
    Unconnected(Arc<LinkUnicastUdpUnconnected>),
}

pub struct LinkUnicastUdp {
    // The source socket address of this link (address used on the local host)
    src_addr: SocketAddr,
    src_locator: Locator,
    // The destination socket address of this link (address used on the remote host)
    dst_addr: SocketAddr,
    dst_locator: Locator,
    // The UDP socket is connected to the peer
    variant: LinkUnicastUdpVariant,
}

impl LinkUnicastUdp {
    fn new(
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
        variant: LinkUnicastUdpVariant,
    ) -> LinkUnicastUdp {
        LinkUnicastUdp {
            src_locator: socket_addr_to_udp_locator(&src_addr),
            dst_locator: socket_addr_to_udp_locator(&dst_addr),
            src_addr,
            dst_addr,
            variant,
        }
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastUdp {
    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing UDP link: {}", self);
        match &self.variant {
            LinkUnicastUdpVariant::Connected(link) => link.close().await,
            LinkUnicastUdpVariant::Unconnected(link) => {
                link.close(self.src_addr, self.dst_addr).await
            }
        }
    }

    async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
        match &self.variant {
            LinkUnicastUdpVariant::Connected(link) => link.write(buffer).await,
            LinkUnicastUdpVariant::Unconnected(link) => link.write(buffer, self.dst_addr).await,
        }
    }

    async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
        let mut written: usize = 0;
        while written < buffer.len() {
            written += self.write(&buffer[written..]).await?;
        }
        Ok(())
    }

    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize> {
        match &self.variant {
            LinkUnicastUdpVariant::Connected(link) => link.read(buffer).await,
            LinkUnicastUdpVariant::Unconnected(link) => link.read(buffer).await,
        }
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
    fn get_mtu(&self) -> BatchSize {
        *UDP_DEFAULT_MTU
    }

    #[inline(always)]
    fn get_interface_names(&self) -> Vec<String> {
        get_ip_interface_names(&self.src_addr)
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        super::IS_RELIABLE
    }

    #[inline(always)]
    fn is_streamed(&self) -> bool {
        false
    }

    #[inline(always)]
    fn get_auth_id(&self) -> &LinkAuthId {
        &LinkAuthId::Udp
    }
}

impl fmt::Display for LinkUnicastUdp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src_addr, self.dst_addr)?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastUdp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Udp")
            .field("src", &self.src_addr)
            .field("dst", &self.dst_addr)
            .finish()
    }
}

pub struct LinkManagerUnicastUdp {
    manager: NewLinkChannelSender,
    listeners: ListenersUnicastIP,
}

impl LinkManagerUnicastUdp {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: ListenersUnicastIP::new(),
        }
    }
}
impl ConstructibleLinkManagerUnicast<()> for LinkManagerUnicastUdp {
    fn new(new_link_sender: NewLinkChannelSender, _: ()) -> ZResult<Self> {
        Ok(Self::new(new_link_sender))
    }
}

impl LinkManagerUnicastUdp {
    async fn new_link_inner(
        &self,
        dst_addr: &SocketAddr,
        iface: Option<&str>,
        bind_socket: Option<&str>,
        dscp: Option<u32>,
    ) -> ZResult<(UdpSocket, SocketAddr, SocketAddr)> {
        let src_socket_addr = if let Some(bind_socket) = bind_socket {
            let address = Address::from(bind_socket);
            get_udp_addrs(address)
                .await?
                .next()
                .ok_or_else(|| zerror!("No UDP socket addr found bound to {}", address))?
        } else if dst_addr.is_ipv4() {
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)
        } else {
            SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 0)
        };

        // Establish a UDP socket
        let socket = UdpSocket::bind(src_socket_addr).await.map_err(|e| {
            let e = zerror!(
                "Can not create a new UDP link bound to {}: {}",
                src_socket_addr,
                e
            );
            tracing::warn!("{}", e);
            e
        })?;

        if let Some(iface) = iface {
            zenoh_util::net::set_bind_to_device_udp_socket(&socket, iface)?;
        }

        if let Some(dscp) = dscp {
            set_dscp(&socket, *dst_addr, dscp)?;
        }

        // Connect the socket to the remote address
        socket.connect(dst_addr).await.map_err(|e| {
            let e = zerror!("Can not connect a new UDP link to {}: {}", dst_addr, e);
            tracing::warn!("{}", e);
            e
        })?;

        // Get source and destination UDP addresses
        let src_addr = socket.local_addr().map_err(|e| {
            let e = zerror!(
                "Can not get local_addr for UDP link bound src {}: to :{} : {}",
                src_socket_addr,
                dst_addr,
                e
            );
            tracing::warn!("{}", e);
            e
        })?;

        let dst_addr = socket.peer_addr().map_err(|e| {
            let e = zerror!(
                "Can not get peer_addr for UDP link bound src {}: to :{} : {}",
                src_socket_addr,
                dst_addr,
                e
            );
            tracing::warn!("{}", e);
            e
        })?;

        Ok((socket, src_addr, dst_addr))
    }

    async fn new_listener_inner(
        &self,
        addr: &SocketAddr,
        iface: Option<&str>,
        dscp: Option<u32>,
    ) -> ZResult<(UdpSocket, SocketAddr)> {
        // Bind the UDP socket
        let socket = UdpSocket::bind(addr).await.map_err(|e| {
            let e = zerror!("Can not create a new UDP listener on {}: {}", addr, e);
            tracing::warn!("{}", e);
            e
        })?;

        if let Some(iface) = iface {
            zenoh_util::net::set_bind_to_device_udp_socket(&socket, iface)?;
        }

        if let Some(dscp) = dscp {
            set_dscp(&socket, *addr, dscp)?;
        }

        let local_addr = socket.local_addr().map_err(|e| {
            let e = zerror!("Can not create a new UDP listener on {}: {}", addr, e);
            tracing::warn!("{}", e);
            e
        })?;

        Ok((socket, local_addr))
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastUdp {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let dst_addrs = get_udp_addrs(endpoint.address())
            .await?
            .filter(|a| !a.ip().is_multicast());
        let config = endpoint.config();
        let iface = config.get(BIND_INTERFACE);

        if let (Some(_), Some(_)) = (config.get(BIND_INTERFACE), config.get(BIND_SOCKET)) {
            bail!(
                "Using Config options `iface` and `bind` in conjunction is unsupported at this time {} {:?}",
                BIND_INTERFACE,
                BIND_SOCKET
            )
        }

        let bind_socket = config.get(BIND_SOCKET);
        let dscp = parse_dscp(&config)?;

        let mut errs: Vec<ZError> = vec![];
        for da in dst_addrs {
            match self.new_link_inner(&da, iface, bind_socket, dscp).await {
                Ok((socket, src_addr, dst_addr)) => {
                    // Create UDP link
                    let link = Arc::new(LinkUnicastUdp::new(
                        src_addr,
                        dst_addr,
                        LinkUnicastUdpVariant::Connected(LinkUnicastUdpConnected {
                            socket: Arc::new(socket),
                        }),
                    ));

                    return Ok(LinkUnicast(link));
                }
                Err(e) => {
                    errs.push(e);
                }
            }
        }

        if errs.is_empty() {
            errs.push(zerror!("No UDP unicast addresses available").into());
        }

        bail!(
            "Can not create a new UDP link bound to {}: {:?}",
            endpoint,
            errs
        )
    }

    async fn new_listener(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let addrs = get_udp_addrs(endpoint.address())
            .await?
            .filter(|a| !a.ip().is_multicast());
        let config = endpoint.config();
        let iface = config.get(BIND_INTERFACE);
        let dscp = parse_dscp(&config)?;

        let mut errs: Vec<ZError> = vec![];
        for da in addrs {
            match self.new_listener_inner(&da, iface, dscp).await {
                Ok((socket, local_addr)) => {
                    // Update the endpoint locator address
                    endpoint = EndPoint::new(
                        endpoint.protocol(),
                        local_addr.to_string(),
                        endpoint.metadata(),
                        endpoint.config(),
                    )?;

                    let token = self.listeners.token.child_token();

                    let task = {
                        let token = token.clone();
                        let manager = self.manager.clone();

                        async move { accept_read_task(socket, token, manager).await }
                    };

                    let locator = endpoint.to_locator();
                    self.listeners
                        .add_listener(endpoint, local_addr, task, token)
                        .await?;

                    return Ok(locator);
                }
                Err(e) => {
                    errs.push(e);
                }
            }
        }

        if errs.is_empty() {
            errs.push(zerror!("No UDP unicast addresses available").into());
        }

        bail!(
            "Can not create a new UDP listener bound to {}: {:?}",
            endpoint,
            errs
        )
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let addrs = get_udp_addrs(endpoint.address())
            .await?
            .filter(|x| !x.ip().is_multicast());

        // Stop the listener
        let mut errs: Vec<ZError> = vec![];
        let mut failed = true;
        for a in addrs {
            match self.listeners.del_listener(a).await {
                Ok(_) => {
                    failed = false;
                    break;
                }
                Err(err) => {
                    errs.push(zerror!("{}", err).into());
                }
            }
        }

        if failed {
            bail!(
                "Can not delete the TCP listener bound to {}: {:?}",
                endpoint,
                errs
            )
        }
        Ok(())
    }

    async fn get_listeners(&self) -> Vec<EndPoint> {
        self.listeners.get_endpoints()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        self.listeners.get_locators()
    }
}

async fn accept_read_task(
    socket: UdpSocket,
    token: CancellationToken,
    manager: NewLinkChannelSender,
) -> ZResult<()> {
    let socket = Arc::new(socket);
    let links: LinkHashMap = Arc::new(Mutex::new(HashMap::new()));

    macro_rules! zaddlink {
        ($src:expr, $dst:expr, $link:expr) => {
            zlock!(links).insert(($src, $dst), $link);
        };
    }

    macro_rules! zdellink {
        ($src:expr, $dst:expr) => {
            zlock!(links).remove(&($src, $dst));
        };
    }

    macro_rules! zgetlink {
        ($src:expr, $dst:expr) => {
            zlock!(links).get(&($src, $dst)).map(|link| link.clone())
        };
    }

    async fn receive(
        socket: Arc<pktinfo::PktInfoUdpSocket>,
        buffer: &mut [u8],
    ) -> ZResult<(usize, SocketAddr, SocketAddr)> {
        let res = socket.receive(buffer).await.map_err(|e| zerror!(e))?;
        Ok(res)
    }

    let local_src_addr = socket.local_addr().map_err(|e| {
        let e = zerror!("Can not accept UDP connections: {}", e);
        tracing::warn!("{}", e);
        e
    })?;
    let socket = Arc::new(pktinfo::PktInfoUdpSocket::new(socket).map_err(|e| {
        let e = zerror!("Failed to enable IP_PKTINFO: {}", e);
        tracing::warn!("{}", e);
        e
    })?);

    tracing::trace!("Ready to accept UDP connections on: {:?}", local_src_addr);

    loop {
        // Buffers for deserialization
        let mut buff = zenoh_buffers::vec::uninit(UDP_MAX_MTU as usize);

        tokio::select! {
            _ = token.cancelled() => break,

            res = receive(socket.clone(), &mut buff) => {
                match res {
                    Ok((n, dst_addr, src_addr)) => {
                        let link = loop {
                            let res = zgetlink!(src_addr, dst_addr);
                            match res {
                                Some(link) => break link.upgrade(),
                                None => {
                                    // A new peers has sent data to this socket
                                    tracing::debug!("Accepted UDP connection on {}: {}", src_addr, dst_addr);
                                    let unconnected = Arc::new(LinkUnicastUdpUnconnected {
                                        socket: Arc::downgrade(&socket.socket),
                                        links: links.clone(),
                                        input: Mvar::new(),
                                        leftover: AsyncMutex::new(None),
                                    });
                                    zaddlink!(src_addr, dst_addr, Arc::downgrade(&unconnected));
                                    // Create the new link object
                                    let link = Arc::new(LinkUnicastUdp::new(
                                        src_addr,
                                        dst_addr,
                                        LinkUnicastUdpVariant::Unconnected(unconnected),
                                    ));
                                    // Add the new link to the set of connected peers
                                    if let Err(e) = manager.send_async(LinkUnicast(link)).await {
                                        tracing::error!("{}-{}: {}", file!(), line!(), e)
                                    }
                                }
                            }
                        };

                        match link {
                            Some(link) => {
                                link.received(buff, n).await;
                            }
                            None => {
                                zdellink!(src_addr, dst_addr);
                            }
                        }
                    }

                    Err(e) => {
                        tracing::warn!("{}. Hint: increase the system open file limit.", e);
                        // Throttle the accept loop upon an error
                        // NOTE: This might be due to various factors. However, the most common case is that
                        //       the process has reached the maximum number of open files in the system. On
                        //       Linux systems this limit can be changed by using the "ulimit" command line
                        //       tool. In case of systemd-based systems, this can be changed by using the
                        //       "sysctl" command line tool.
                        tokio::time::sleep(Duration::from_micros(*UDP_ACCEPT_THROTTLE_TIME)).await;
                    }
                }
            }
        }
    }

    Ok(())
}
