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
use super::{Runtime, RuntimeSession};
use async_std::net::UdpSocket;
use async_std::prelude::FutureExt;
use futures::prelude::*;
use socket2::{Domain, Socket, Type};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use zenoh_buffers::reader::DidntRead;
use zenoh_buffers::{reader::HasReader, writer::HasWriter};
use zenoh_codec::{RCodec, WCodec, Zenoh060};
use zenoh_config::{unwrap_or_default, EndPoint, ModeDependent};
use zenoh_link::Locator;
use zenoh_protocol::{
    core::{whatami::WhatAmIMatcher, WhatAmI, ZenohId},
    scouting::{Hello, Scout, ScoutingBody, ScoutingMessage},
};
use zenoh_result::{bail, zerror, ZResult};
use zenoh_transport::TransportUnicast;

const RCV_BUF_SIZE: usize = u16::MAX as usize;
const SCOUT_INITIAL_PERIOD: Duration = Duration::from_millis(1_000);
const SCOUT_MAX_PERIOD: Duration = Duration::from_millis(8_000);
const SCOUT_PERIOD_INCREASE_FACTOR: u32 = 2;
const CONNECTION_TIMEOUT: Duration = Duration::from_millis(10_000);
const CONNECTION_RETRY_INITIAL_PERIOD: Duration = Duration::from_millis(1_000);
const CONNECTION_RETRY_MAX_PERIOD: Duration = Duration::from_millis(4_000);
const CONNECTION_RETRY_PERIOD_INCREASE_FACTOR: u32 = 2;
const ROUTER_DEFAULT_LISTENER: &str = "tcp/[::]:7447";
const PEER_DEFAULT_LISTENER: &str = "tcp/[::]:0";

pub enum Loop {
    Continue,
    Break,
}

impl Runtime {
    pub(crate) async fn start(&mut self) -> ZResult<()> {
        match self.whatami {
            WhatAmI::Client => self.start_client().await,
            WhatAmI::Peer => self.start_peer().await,
            WhatAmI::Router => self.start_router().await,
        }
    }

    async fn start_client(&self) -> ZResult<()> {
        let (peers, scouting, addr, ifaces, timeout) = {
            let guard = self.config.lock();
            (
                guard.connect().endpoints().clone(),
                unwrap_or_default!(guard.scouting().multicast().enabled()),
                unwrap_or_default!(guard.scouting().multicast().address()),
                unwrap_or_default!(guard.scouting().multicast().interface()),
                std::time::Duration::from_millis(unwrap_or_default!(guard.scouting().timeout())),
            )
        };
        match peers.len() {
            0 => {
                if scouting {
                    log::info!("Scouting for router ...");
                    let ifaces = Runtime::get_interfaces(&ifaces);
                    if ifaces.is_empty() {
                        bail!("Unable to find multicast interface!")
                    } else {
                        let sockets: Vec<UdpSocket> = ifaces
                            .into_iter()
                            .filter_map(|iface| Runtime::bind_ucast_port(iface).ok())
                            .collect();
                        if sockets.is_empty() {
                            bail!("Unable to bind UDP port to any multicast interface!")
                        } else {
                            self.connect_first(&sockets, WhatAmI::Router.into(), &addr, timeout)
                                .await
                        }
                    }
                } else {
                    bail!("No peer specified and multicast scouting desactivated!")
                }
            }
            _ => {
                for locator in &peers {
                    match self
                        .manager()
                        .open_transport(locator.clone())
                        .timeout(CONNECTION_TIMEOUT)
                        .await
                    {
                        Ok(Ok(_)) => return Ok(()),
                        Ok(Err(e)) => log::warn!("Unable to connect to {}! {}", locator, e),
                        Err(e) => log::warn!("Unable to connect to {}! {}", locator, e),
                    }
                }
                let e = zerror!("Unable to connect to any of {:?}! ", peers);
                log::error!("{}", &e);
                Err(e.into())
            }
        }
    }

    async fn start_peer(&self) -> ZResult<()> {
        let (listeners, peers, scouting, listen, autoconnect, addr, ifaces, delay) = {
            let guard = &self.config.lock();
            let listeners = if guard.listen().endpoints().is_empty() {
                let endpoint: EndPoint = PEER_DEFAULT_LISTENER.parse().unwrap();
                let protocol = endpoint.protocol();
                let mut listeners = vec![];
                if self
                    .manager
                    .config
                    .protocols
                    .iter()
                    .any(|p| p.as_str() == protocol.as_str())
                {
                    listeners.push(endpoint)
                }
                listeners
            } else {
                guard.listen().endpoints().clone()
            };
            (
                listeners,
                guard.connect().endpoints().clone(),
                unwrap_or_default!(guard.scouting().multicast().enabled()),
                *unwrap_or_default!(guard.scouting().multicast().listen().peer()),
                *unwrap_or_default!(guard.scouting().multicast().autoconnect().peer()),
                unwrap_or_default!(guard.scouting().multicast().address()),
                unwrap_or_default!(guard.scouting().multicast().interface()),
                Duration::from_millis(unwrap_or_default!(guard.scouting().delay())),
            )
        };

        self.bind_listeners(&listeners).await?;

        for peer in peers {
            let this = self.clone();
            self.spawn(async move { this.peer_connector(peer).await });
        }

        if scouting {
            self.start_scout(listen, autoconnect, addr, ifaces).await?;
        }
        async_std::task::sleep(delay).await;
        Ok(())
    }

    async fn start_router(&self) -> ZResult<()> {
        let (listeners, peers, scouting, listen, autoconnect, addr, ifaces) = {
            let guard = self.config.lock();
            let listeners = if guard.listen().endpoints().is_empty() {
                let endpoint: EndPoint = ROUTER_DEFAULT_LISTENER.parse().unwrap();
                let protocol = endpoint.protocol();
                let mut listeners = vec![];
                if self
                    .manager
                    .config
                    .protocols
                    .iter()
                    .any(|p| p.as_str() == protocol.as_str())
                {
                    listeners.push(endpoint)
                }
                listeners
            } else {
                guard.listen().endpoints().clone()
            };
            (
                listeners,
                guard.connect().endpoints().clone(),
                unwrap_or_default!(guard.scouting().multicast().enabled()),
                *unwrap_or_default!(guard.scouting().multicast().listen().router()),
                *unwrap_or_default!(guard.scouting().multicast().autoconnect().router()),
                unwrap_or_default!(guard.scouting().multicast().address()),
                unwrap_or_default!(guard.scouting().multicast().interface()),
            )
        };

        self.bind_listeners(&listeners).await?;

        for peer in peers {
            let this = self.clone();
            self.spawn(async move { this.peer_connector(peer).await });
        }

        if scouting {
            self.start_scout(listen, autoconnect, addr, ifaces).await?;
        }

        Ok(())
    }

    async fn start_scout(
        &self,
        listen: bool,
        autoconnect: WhatAmIMatcher,
        addr: SocketAddr,
        ifaces: String,
    ) -> ZResult<()> {
        let ifaces = Runtime::get_interfaces(&ifaces);
        let mcast_socket = Runtime::bind_mcast_port(&addr, &ifaces).await?;
        if !ifaces.is_empty() {
            let sockets: Vec<UdpSocket> = ifaces
                .into_iter()
                .filter_map(|iface| Runtime::bind_ucast_port(iface).ok())
                .collect();
            if !sockets.is_empty() {
                let this = self.clone();
                match (listen, autoconnect.is_empty()) {
                    (true, false) => {
                        self.spawn(async move {
                            async_std::prelude::FutureExt::race(
                                this.responder(&mcast_socket, &sockets),
                                this.connect_all(&sockets, autoconnect, &addr),
                            )
                            .await;
                        });
                    }
                    (true, true) => {
                        self.spawn(async move {
                            this.responder(&mcast_socket, &sockets).await;
                        });
                    }
                    (false, false) => {
                        self.spawn(
                            async move { this.connect_all(&sockets, autoconnect, &addr).await },
                        );
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn update_peers(&self) -> ZResult<()> {
        let peers = { self.config.lock().connect().endpoints().clone() };
        let tranports = self.manager().get_transports();

        if self.whatami == WhatAmI::Client {
            for transport in tranports {
                let should_close = if let Some(orch_transport) = transport
                    .get_callback()
                    .unwrap()
                    .unwrap()
                    .as_any()
                    .downcast_ref::<super::RuntimeSession>()
                {
                    if let Some(endpoint) = &*zread!(orch_transport.endpoint) {
                        !peers.contains(endpoint)
                    } else {
                        true
                    }
                } else {
                    false
                };
                if should_close {
                    transport.close().await?;
                }
            }
        } else {
            for peer in peers {
                if !tranports.iter().any(|transport| {
                    if let Some(orch_transport) = transport
                        .get_callback()
                        .unwrap()
                        .unwrap()
                        .as_any()
                        .downcast_ref::<super::RuntimeSession>()
                    {
                        if let Some(endpoint) = &*zread!(orch_transport.endpoint) {
                            return *endpoint == peer;
                        }
                    }
                    false
                }) {
                    let this = self.clone();
                    self.spawn(async move { this.peer_connector(peer).await });
                }
            }
        }

        Ok(())
    }

    async fn bind_listeners(&self, listeners: &[EndPoint]) -> ZResult<()> {
        for listener in listeners {
            let endpoint = listener.clone();
            match self.manager().add_listener(endpoint).await {
                Ok(listener) => log::debug!("Listener added: {}", listener),
                Err(err) => {
                    log::error!("Unable to open listener {}: {}", listener, err);
                    return Err(err);
                }
            }
        }

        let mut locators = self.locators.write().unwrap();
        *locators = self.manager().get_locators();
        for locator in &*locators {
            log::info!("Zenoh can be reached at: {}", locator);
        }
        Ok(())
    }

    pub fn get_interfaces(names: &str) -> Vec<IpAddr> {
        if names == "auto" {
            let ifaces = zenoh_util::net::get_multicast_interfaces();
            if ifaces.is_empty() {
                log::warn!(
                    "Unable to find active, non-loopback multicast interface. Will use [::]."
                );
                vec![Ipv6Addr::UNSPECIFIED.into()]
            } else {
                ifaces
            }
        } else {
            names
                .split(',')
                .filter_map(|name| match name.trim().parse::<IpAddr>() {
                    Ok(addr) => Some(addr),
                    Err(_) => match zenoh_util::net::get_interface(name.trim()) {
                        Ok(opt_addr) => match opt_addr {
                            Some(addr) => Some(addr),
                            None => {
                                log::error!("Unable to find interface {}", name);
                                None
                            }
                        },
                        Err(err) => {
                            log::error!("Unable to find interface {}: {}", name, err);
                            None
                        }
                    },
                })
                .collect()
        }
    }

    pub async fn bind_mcast_port(sockaddr: &SocketAddr, ifaces: &[IpAddr]) -> ZResult<UdpSocket> {
        let socket = match Socket::new(Domain::IPV4, Type::DGRAM, None) {
            Ok(socket) => socket,
            Err(err) => {
                log::error!("Unable to create datagram socket: {}", err);
                bail!(err => "Unable to create datagram socket");
            }
        };
        if let Err(err) = socket.set_reuse_address(true) {
            log::error!("Unable to set SO_REUSEADDR option: {}", err);
            bail!(err => "Unable to set SO_REUSEADDR option");
        }
        let addr: IpAddr = {
            #[cfg(unix)]
            {
                sockaddr.ip()
            } // See UNIX Network Programmping p.212
            #[cfg(windows)]
            {
                std::net::Ipv4Addr::UNSPECIFIED.into()
            }
        };
        match socket.bind(&SocketAddr::new(addr, sockaddr.port()).into()) {
            Ok(()) => log::debug!("UDP port bound to {}", sockaddr),
            Err(err) => {
                log::error!("Unable to bind UDP port {}: {}", sockaddr, err);
                bail!(err => "Unable to bind UDP port {}", sockaddr);
            }
        }

        match sockaddr.ip() {
            IpAddr::V6(addr) => match socket.join_multicast_v6(&addr, 0) {
                Ok(()) => log::debug!("Joined multicast group {} on interface 0", sockaddr.ip()),
                Err(err) => {
                    log::error!(
                        "Unable to join multicast group {} on interface 0: {}",
                        sockaddr.ip(),
                        err
                    );
                    bail!(err =>
                        "Unable to join multicast group {} on interface 0",
                        sockaddr.ip()
                    )
                }
            },
            IpAddr::V4(addr) => {
                for iface in ifaces {
                    if let IpAddr::V4(iface_addr) = iface {
                        match socket.join_multicast_v4(&addr, iface_addr) {
                            Ok(()) => log::debug!(
                                "Joined multicast group {} on interface {}",
                                sockaddr.ip(),
                                iface_addr,
                            ),
                            Err(err) => log::warn!(
                                "Unable to join multicast group {} on interface {}: {}",
                                sockaddr.ip(),
                                iface_addr,
                                err,
                            ),
                        }
                    } else {
                        log::warn!(
                            "Cannot join IpV4 multicast group {} on IpV6 iface {}",
                            sockaddr.ip(),
                            iface
                        );
                    }
                }
            }
        }
        log::info!("zenohd listening scout messages on {}", sockaddr);
        Ok(std::net::UdpSocket::from(socket).into())
    }

    pub fn bind_ucast_port(addr: IpAddr) -> ZResult<UdpSocket> {
        let socket = match Socket::new(Domain::IPV4, Type::DGRAM, None) {
            Ok(socket) => socket,
            Err(err) => {
                log::warn!("Unable to create datagram socket: {}", err);
                bail!(err=> "Unable to create datagram socket");
            }
        };
        match socket.bind(&SocketAddr::new(addr, 0).into()) {
            Ok(()) => {
                #[allow(clippy::or_fun_call)]
                let local_addr = socket
                    .local_addr()
                    .unwrap_or(SocketAddr::new(addr, 0).into())
                    .as_socket()
                    .unwrap_or(SocketAddr::new(addr, 0));
                log::debug!("UDP port bound to {}", local_addr);
            }
            Err(err) => {
                log::warn!("Unable to bind udp port {}:0: {}", addr, err);
                bail!(err => "Unable to bind udp port {}:0", addr);
            }
        }
        Ok(std::net::UdpSocket::from(socket).into())
    }

    async fn peer_connector(&self, peer: EndPoint) {
        let mut delay = CONNECTION_RETRY_INITIAL_PERIOD;
        loop {
            log::trace!("Trying to connect to configured peer {}", peer);
            let endpoint = peer.clone();
            match self
                .manager()
                .open_transport(endpoint)
                .timeout(CONNECTION_TIMEOUT)
                .await
            {
                Ok(Ok(transport)) => {
                    log::debug!("Successfully connected to configured peer {}", peer);
                    if let Some(orch_transport) = transport
                        .get_callback()
                        .unwrap()
                        .unwrap()
                        .as_any()
                        .downcast_ref::<super::RuntimeSession>()
                    {
                        *zwrite!(orch_transport.endpoint) = Some(peer);
                    }
                    break;
                }
                Ok(Err(e)) => {
                    log::debug!(
                        "Unable to connect to configured peer {}! {}. Retry in {:?}.",
                        peer,
                        e,
                        delay
                    );
                }
                Err(e) => {
                    log::debug!(
                        "Unable to connect to configured peer {}! {}. Retry in {:?}.",
                        peer,
                        e,
                        delay
                    );
                }
            }
            async_std::task::sleep(delay).await;
            delay *= CONNECTION_RETRY_PERIOD_INCREASE_FACTOR;
            if delay > CONNECTION_RETRY_MAX_PERIOD {
                delay = CONNECTION_RETRY_MAX_PERIOD;
            }
        }
    }

    pub async fn scout<Fut, F>(
        sockets: &[UdpSocket],
        matcher: WhatAmIMatcher,
        mcast_addr: &SocketAddr,
        f: F,
    ) where
        F: Fn(Hello) -> Fut + std::marker::Send + std::marker::Sync + Clone,
        Fut: Future<Output = Loop> + std::marker::Send,
        Self: Sized,
    {
        let send = async {
            let mut delay = SCOUT_INITIAL_PERIOD;

            let scout = ScoutingMessage::make_scout(Some(matcher), true, None);
            let mut wbuf = vec![];
            let mut writer = wbuf.writer();
            let codec = Zenoh060::default();
            codec.write(&mut writer, &scout).unwrap();

            loop {
                for socket in sockets {
                    log::trace!(
                        "Send {:?} to {} on interface {}",
                        scout.body,
                        mcast_addr,
                        socket
                            .local_addr()
                            .map_or("unknown".to_string(), |addr| addr.ip().to_string())
                    );
                    if let Err(err) = socket
                        .send_to(wbuf.as_slice(), mcast_addr.to_string())
                        .await
                    {
                        log::debug!(
                            "Unable to send {:?} to {} on interface {}: {}",
                            scout.body,
                            mcast_addr,
                            socket
                                .local_addr()
                                .map_or("unknown".to_string(), |addr| addr.ip().to_string()),
                            err
                        );
                    }
                }
                async_std::task::sleep(delay).await;
                if delay * SCOUT_PERIOD_INCREASE_FACTOR <= SCOUT_MAX_PERIOD {
                    delay *= SCOUT_PERIOD_INCREASE_FACTOR;
                }
            }
        };
        let recvs = futures::future::select_all(sockets.iter().map(move |socket| {
            let f = f.clone();
            async move {
                let mut buf = vec![0; RCV_BUF_SIZE];
                loop {
                    match socket.recv_from(&mut buf).await {
                        Ok((n, peer)) => {
                            let mut reader = buf.as_slice()[..n].reader();
                            let codec = Zenoh060::default();
                            let res: Result<ScoutingMessage, DidntRead> = codec.read(&mut reader);
                            if let Ok(msg) = res {
                                log::trace!("Received {:?} from {}", msg.body, peer);
                                if let ScoutingBody::Hello(hello) = &msg.body {
                                    if matcher.matches(hello.whatami) {
                                        if let Loop::Break = f(hello.clone()).await {
                                            break;
                                        }
                                    } else {
                                        log::warn!("Received unexpected Hello: {:?}", msg.body);
                                    }
                                }
                            } else {
                                log::trace!(
                                    "Received unexpected UDP datagram from {}: {:?}",
                                    peer,
                                    &buf.as_slice()[..n]
                                );
                            }
                        }
                        Err(e) => log::debug!("Error receiving UDP datagram: {}", e),
                    }
                }
            }
            .boxed()
        }));
        async_std::prelude::FutureExt::race(send, recvs).await;
    }

    async fn connect(&self, locators: &[Locator]) -> Option<TransportUnicast> {
        for locator in locators {
            let endpoint = locator.clone().into();
            match self
                .manager()
                .open_transport(endpoint)
                .timeout(CONNECTION_TIMEOUT)
                .await
            {
                Ok(Ok(transport)) => return Some(transport),
                Ok(Err(e)) => log::trace!("Unable to connect to {}! {}", locator, e),
                Err(e) => log::trace!("Unable to connect to {}! {}", locator, e),
            }
        }
        None
    }

    pub async fn connect_peer(&self, zid: &ZenohId, locators: &[Locator]) {
        if zid != &self.manager().zid() {
            if self.manager().get_transport(zid).is_none() {
                log::debug!("Try to connect to peer {} via any of {:?}", zid, locators);
                if let Some(transport) = self.connect(locators).await {
                    log::debug!(
                        "Successfully connected to newly scouted peer {} via {:?}",
                        zid,
                        transport
                    );
                } else {
                    log::warn!(
                        "Unable to connect any locator of scouted peer {}: {:?}",
                        zid,
                        locators
                    );
                }
            } else {
                log::trace!("Already connected scouted peer: {}", zid);
            }
        }
    }

    async fn connect_first(
        &self,
        sockets: &[UdpSocket],
        what: WhatAmIMatcher,
        addr: &SocketAddr,
        timeout: std::time::Duration,
    ) -> ZResult<()> {
        let scout = async {
            Runtime::scout(sockets, what, addr, move |hello| async move {
                log::info!("Found {:?}", hello);
                if !hello.locators.is_empty() {
                    if let Some(transport) = self.connect(&hello.locators).await {
                        log::debug!(
                            "Successfully connected to newly scouted {:?} via {:?}",
                            hello,
                            transport
                        );
                        return Loop::Break;
                    }
                    log::warn!("Unable to connect to scouted {:?}", hello);
                } else {
                    log::warn!("Received Hello with no locators: {:?}", hello);
                }
                Loop::Continue
            })
            .await;
            Ok(())
        };
        let timeout = async {
            async_std::task::sleep(timeout).await;
            bail!("timeout")
        };
        async_std::prelude::FutureExt::race(scout, timeout).await
    }

    async fn connect_all(
        &self,
        ucast_sockets: &[UdpSocket],
        what: WhatAmIMatcher,
        addr: &SocketAddr,
    ) {
        Runtime::scout(ucast_sockets, what, addr, move |hello| async move {
            match &hello.zid {
                Some(zid) => {
                    if !hello.locators.is_empty() {
                        self.connect_peer(zid, &hello.locators).await
                    } else {
                        log::warn!("Received Hello with no locators: {:?}", hello);
                    }
                }
                None => {
                    log::warn!("Received Hello with no zid: {:?}", hello);
                }
            }
            Loop::Continue
        })
        .await
    }

    async fn responder(&self, mcast_socket: &UdpSocket, ucast_sockets: &[UdpSocket]) {
        fn get_best_match<'a>(addr: &IpAddr, sockets: &'a [UdpSocket]) -> Option<&'a UdpSocket> {
            fn octets(addr: &IpAddr) -> Vec<u8> {
                match addr {
                    IpAddr::V4(addr) => addr.octets().to_vec(),
                    IpAddr::V6(addr) => addr.octets().to_vec(),
                }
            }
            fn matching_octets(addr: &IpAddr, sock: &UdpSocket) -> usize {
                octets(addr)
                    .iter()
                    .zip(octets(&sock.local_addr().unwrap().ip()))
                    .map(|(x, y)| x.cmp(&y))
                    .position(|ord| ord != std::cmp::Ordering::Equal)
                    .unwrap_or_else(|| octets(addr).len())
            }
            sockets
                .iter()
                .filter(|sock| sock.local_addr().is_ok())
                .max_by(|sock1, sock2| {
                    matching_octets(addr, sock1).cmp(&matching_octets(addr, sock2))
                })
        }

        let mut buf = vec![0; RCV_BUF_SIZE];
        let local_addrs: Vec<SocketAddr> = ucast_sockets
            .iter()
            .filter_map(|sock| sock.local_addr().ok())
            .collect();
        log::debug!("Waiting for UDP datagram...");
        loop {
            let (n, peer) = mcast_socket.recv_from(&mut buf).await.unwrap();
            if local_addrs.iter().any(|addr| *addr == peer) {
                log::trace!("Ignore UDP datagram from own socket");
                continue;
            }

            let mut reader = buf.as_slice()[..n].reader();
            let codec = Zenoh060::default();
            let res: Result<ScoutingMessage, DidntRead> = codec.read(&mut reader);
            if let Ok(msg) = res {
                log::trace!("Received {:?} from {}", msg.body, peer);
                if let ScoutingBody::Scout(Scout {
                    what, zid_request, ..
                }) = &msg.body
                {
                    let what = what.or(Some(WhatAmI::Router.into())).unwrap();
                    if what.matches(self.whatami) {
                        let mut wbuf = vec![];
                        let mut writer = wbuf.writer();
                        let codec = Zenoh060::default();

                        let zid = if *zid_request {
                            Some(self.manager().zid())
                        } else {
                            None
                        };
                        let hello = ScoutingMessage::make_hello(
                            zid,
                            Some(self.whatami),
                            Some(self.get_locators()),
                            None,
                        );
                        let socket = get_best_match(&peer.ip(), ucast_sockets).unwrap();
                        log::trace!(
                            "Send {:?} to {} on interface {}",
                            hello.body,
                            peer,
                            socket
                                .local_addr()
                                .map_or("unknown".to_string(), |addr| addr.ip().to_string())
                        );
                        codec.write(&mut writer, &hello).unwrap();

                        if let Err(err) = socket.send_to(wbuf.as_slice(), peer).await {
                            log::error!("Unable to send {:?} to {}: {}", hello.body, peer, err);
                        }
                    }
                }
            } else {
                log::trace!(
                    "Received unexpected UDP datagram from {}: {:?}",
                    peer,
                    &buf.as_slice()[..n]
                );
            }
        }
    }

    pub(super) fn closing_session(session: &RuntimeSession) {
        match session.runtime.whatami {
            WhatAmI::Client => {
                let runtime = session.runtime.clone();
                session.runtime.spawn(async move {
                    let mut delay = CONNECTION_RETRY_INITIAL_PERIOD;
                    while runtime.start_client().await.is_err() {
                        async_std::task::sleep(delay).await;
                        delay *= CONNECTION_RETRY_PERIOD_INCREASE_FACTOR;
                        if delay > CONNECTION_RETRY_MAX_PERIOD {
                            delay = CONNECTION_RETRY_MAX_PERIOD;
                        }
                    }
                });
            }
            _ => {
                if let Some(endpoint) = &*zread!(session.endpoint) {
                    let peers = { session.runtime.config.lock().connect().endpoints().clone() };
                    if peers.contains(endpoint) {
                        let endpoint = endpoint.clone();
                        let runtime = session.runtime.clone();
                        session
                            .runtime
                            .spawn(async move { runtime.peer_connector(endpoint).await });
                    }
                }
            }
        }
    }
}
