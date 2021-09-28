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
use super::link::{EndPoint, Locator};
use super::protocol::core::{PeerId, WhatAmI};
use super::protocol::io::{WBuf, ZBuf};
use super::protocol::proto::{Hello, Scout, TransportBody, TransportMessage};
use super::transport::TransportUnicast;
use super::{Runtime, RuntimeSession};
use crate::net::protocol::core::whatami::WhatAmIMatcher;
use async_std::net::UdpSocket;
use futures::prelude::*;
use socket2::{Domain, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::*;
use zenoh_util::zerror;

const RCV_BUF_SIZE: usize = 65536;
const SEND_BUF_INITIAL_SIZE: usize = 8;
const SCOUT_INITIAL_PERIOD: u64 = 1000; //ms
const SCOUT_MAX_PERIOD: u64 = 8000; //ms
const SCOUT_PERIOD_INCREASE_FACTOR: u64 = 2;
const CONNECTION_RETRY_INITIAL_PERIOD: u64 = 1000; //ms
const CONNECTION_RETRY_MAX_PERIOD: u64 = 4000; //ms
const CONNECTION_RETRY_PERIOD_INCREASE_FACTOR: u64 = 2;
const ROUTER_DEFAULT_LISTENER: &str = "tcp/0.0.0.0:7447";
const PEER_DEFAULT_LISTENER: &str = "tcp/0.0.0.0:0";

pub enum Loop {
    Continue,
    Break,
}

impl Runtime {
    pub async fn start(&mut self) -> ZResult<()> {
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
                guard.peers().clone(),
                guard.scouting().multicast().enabled().unwrap_or(true),
                guard
                    .scouting()
                    .multicast()
                    .address()
                    .unwrap_or_else(|| "224.0.0.224:7447".parse().unwrap()),
                guard
                    .scouting()
                    .multicast()
                    .interface()
                    .as_ref()
                    .map(AsRef::as_ref)
                    .unwrap_or("auto")
                    .to_owned(),
                std::time::Duration::from_secs_f64(guard.scouting().timeout().unwrap_or(3.)),
            )
        };
        match peers.len() {
            0 => {
                if scouting {
                    log::info!("Scouting for router ...");
                    let ifaces = Runtime::get_interfaces(&ifaces);
                    if ifaces.is_empty() {
                        zerror!(ZErrorKind::IoError {
                            descr: "Unable to find multicast interface!".to_string()
                        })
                    } else {
                        let sockets: Vec<UdpSocket> = ifaces
                            .into_iter()
                            .filter_map(|iface| Runtime::bind_ucast_port(iface).ok())
                            .collect();
                        if sockets.is_empty() {
                            zerror!(ZErrorKind::IoError {
                                descr: "Unable to bind UDP port to any multicast interface!"
                                    .to_string()
                            })
                        } else {
                            self.connect_first(&sockets, WhatAmI::Router, &addr, timeout)
                                .await
                        }
                    }
                } else {
                    zerror!(ZErrorKind::Other {
                        descr: "No peer specified and multicast scouting desactivated!".to_string()
                    })
                }
            }
            _ => {
                for locator in &peers {
                    let endpoint = EndPoint {
                        locator: locator.clone(),
                        config: None,
                    };
                    match self.manager().open_transport(endpoint).await {
                        Ok(_) => return Ok(()),
                        Err(err) => log::warn!("Unable to connect to {}! {}", locator, err),
                    }
                }
                log::error!("Unable to connect to any of {:?}! ", peers);
                zerror!(ZErrorKind::IoError {
                    descr: "".to_string()
                })
            }
        }
    }

    async fn start_peer(&self) -> ZResult<()> {
        let (listeners, peers, scouting, peers_autoconnect, addr, ifaces, delay) = {
            let guard = &self.config.lock();
            let listeners = if guard.listeners().is_empty() {
                vec![PEER_DEFAULT_LISTENER.parse().unwrap()]
            } else {
                guard.listeners().clone()
            };
            (
                listeners,
                guard.peers().clone(),
                guard.scouting().multicast().enabled().unwrap_or(true),
                guard
                    .scouting()
                    .multicast()
                    .autoconnect()
                    .map(|m| m.matches(WhatAmI::Peer))
                    .unwrap_or(true),
                guard
                    .scouting()
                    .multicast()
                    .address()
                    .unwrap_or_else(|| ZN_MULTICAST_IPV4_ADDRESS_DEFAULT.parse().unwrap()),
                guard
                    .scouting()
                    .multicast()
                    .interface()
                    .as_ref()
                    .map(AsRef::as_ref)
                    .unwrap_or_else(|| ZN_MULTICAST_INTERFACE_DEFAULT)
                    .to_string(),
                std::time::Duration::from_secs_f64(guard.scouting().delay().unwrap_or(0.2)),
            )
        };

        self.bind_listeners(&listeners).await?;

        for peer in peers {
            let this = self.clone();
            async_std::task::spawn(async move { this.peer_connector(peer).await });
        }

        if scouting {
            let ifaces = Runtime::get_interfaces(&ifaces);
            let mcast_socket = Runtime::bind_mcast_port(&addr, &ifaces).await?;
            if !ifaces.is_empty() {
                let sockets: Vec<UdpSocket> = ifaces
                    .into_iter()
                    .filter_map(|iface| Runtime::bind_ucast_port(iface).ok())
                    .collect();
                if !sockets.is_empty() {
                    let this = self.clone();
                    async_std::task::spawn(async move {
                        async_std::prelude::FutureExt::race(
                            this.responder(&mcast_socket, &sockets),
                            this.connect_all(
                                &sockets,
                                if peers_autoconnect {
                                    WhatAmI::Peer | WhatAmI::Router
                                } else {
                                    WhatAmI::Router.into()
                                },
                                &addr,
                            ),
                        )
                        .await;
                    });
                }
            }
        }
        async_std::task::sleep(delay).await;
        Ok(())
    }

    async fn start_router(&self) -> ZResult<()> {
        let (listeners, peers, scouting, routers_autoconnect_multicast, addr, ifaces) = {
            let guard = self.config.lock();
            let listeners = if guard.listeners().is_empty() {
                vec![ROUTER_DEFAULT_LISTENER.parse().unwrap()]
            } else {
                guard.listeners().clone()
            };
            (
                listeners,
                guard.peers().clone(),
                guard.scouting().multicast().enabled().unwrap_or(true),
                guard
                    .scouting()
                    .multicast()
                    .autoconnect()
                    .map(|m| m.matches(WhatAmI::Router))
                    .unwrap_or(false),
                guard
                    .scouting()
                    .multicast()
                    .address()
                    .unwrap_or_else(|| ZN_MULTICAST_IPV4_ADDRESS_DEFAULT.parse().unwrap()),
                guard
                    .scouting()
                    .multicast()
                    .interface()
                    .as_ref()
                    .map(AsRef::as_ref)
                    .unwrap_or(ZN_MULTICAST_INTERFACE_DEFAULT)
                    .to_string(),
            )
        };

        self.bind_listeners(&listeners).await?;

        for peer in peers {
            let this = self.clone();
            async_std::task::spawn(async move { this.peer_connector(peer).await });
        }

        if scouting {
            let ifaces = Runtime::get_interfaces(&ifaces);
            let mcast_socket = Runtime::bind_mcast_port(&addr, &ifaces).await?;
            if !ifaces.is_empty() {
                let sockets: Vec<UdpSocket> = ifaces
                    .into_iter()
                    .filter_map(|iface| Runtime::bind_ucast_port(iface).ok())
                    .collect();
                if !sockets.is_empty() {
                    let this = self.clone();
                    if routers_autoconnect_multicast {
                        async_std::task::spawn(async move {
                            async_std::prelude::FutureExt::race(
                                this.responder(&mcast_socket, &sockets),
                                this.connect_all(&sockets, WhatAmI::Router, &addr),
                            )
                            .await;
                        });
                    } else {
                        async_std::task::spawn(async move {
                            this.responder(&mcast_socket, &sockets).await;
                        });
                    }
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn update_peers(&self) -> ZResult<()> {
        let peers = self.config.lock().peers().clone();
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
                    if let Some(locator) = &*zread!(orch_transport.locator) {
                        !peers.contains(locator)
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
                        if let Some(locator) = &*zread!(orch_transport.locator) {
                            return *locator == peer;
                        }
                    }
                    false
                }) {
                    let this = self.clone();
                    async_std::task::spawn(async move { this.peer_connector(peer).await });
                }
            }
        }

        Ok(())
    }

    async fn bind_listeners(&self, listeners: &[Locator]) -> ZResult<()> {
        for listener in listeners {
            let endpoint = EndPoint {
                locator: listener.clone(),
                config: None,
            };
            match self.manager().add_listener(endpoint).await {
                Ok(listener) => log::debug!("Listener {} added", listener),
                Err(err) => {
                    log::error!("Unable to open listener {} : {}", listener, err);
                    return zerror!(
                        ZErrorKind::IoError {
                            descr: "".to_string()
                        },
                        err
                    );
                }
            }
        }
        for locator in self.manager().get_locators() {
            log::info!("zenohd can be reached on {}", locator);
        }
        Ok(())
    }

    pub fn get_interfaces(names: &str) -> Vec<IpAddr> {
        if names == "auto" {
            let ifaces = zenoh_util::net::get_multicast_interfaces();
            if ifaces.is_empty() {
                log::warn!(
                    "Unable to find active, non-loopback multicast interface. Will use 0.0.0.0"
                );
                vec![IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))]
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
                            log::error!("Unable to find interface {} : {}", name, err);
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
                log::error!("Unable to create datagram socket : {}", err);
                return zerror!(
                    ZErrorKind::IoError {
                        descr: "Unable to create datagram socket".to_string()
                    },
                    err
                );
            }
        };
        if let Err(err) = socket.set_reuse_address(true) {
            log::error!("Unable to set SO_REUSEADDR option : {}", err);
            return zerror!(
                ZErrorKind::IoError {
                    descr: "Unable to set SO_REUSEADDR option".to_string()
                },
                err
            );
        }
        let addr = {
            #[cfg(unix)]
            {
                sockaddr.ip()
            } // See UNIX Network Programmping p.212
            #[cfg(windows)]
            {
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            }
        };
        match socket.bind(&SocketAddr::new(addr, sockaddr.port()).into()) {
            Ok(()) => log::debug!("UDP port bound to {}", sockaddr),
            Err(err) => {
                log::error!("Unable to bind udp port {} : {}", sockaddr, err);
                return zerror!(
                    ZErrorKind::IoError {
                        descr: format!("Unable to bind udp port {}", sockaddr)
                    },
                    err
                );
            }
        }

        match sockaddr.ip() {
            IpAddr::V6(addr) => match socket.join_multicast_v6(&addr, 0) {
                Ok(()) => log::debug!("Joined multicast group {} on interface 0", sockaddr.ip()),
                Err(err) => {
                    log::error!(
                        "Unable to join multicast group {} on interface 0 : {}",
                        sockaddr.ip(),
                        err
                    );
                    return zerror!(
                        ZErrorKind::IoError {
                            descr: format!(
                                "Unable to join multicast group {} on interface 0",
                                sockaddr.ip()
                            )
                        },
                        err
                    );
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
                                "Unable to join multicast group {} on interface {} : {}",
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
                log::warn!("Unable to create datagram socket : {}", err);
                return zerror!(
                    ZErrorKind::IoError {
                        descr: "Unable to create datagram socket".to_string()
                    },
                    err
                );
            }
        };
        match socket.bind(&SocketAddr::new(addr, 0).into()) {
            Ok(()) => {
                #[allow(clippy::or_fun_call)]
                let local_addr = socket
                    .local_addr()
                    .or::<std::io::Error>(Ok(SocketAddr::new(addr, 0).into()))
                    .unwrap()
                    .as_socket()
                    .or(Some(SocketAddr::new(addr, 0)))
                    .unwrap();
                log::debug!("UDP port bound to {}", local_addr);
            }
            Err(err) => {
                log::warn!("Unable to bind udp port {}:0 : {}", addr.to_string(), err);
                return zerror!(
                    ZErrorKind::IoError {
                        descr: format!("Unable to bind udp port {}:0", addr.to_string())
                    },
                    err
                );
            }
        }
        Ok(std::net::UdpSocket::from(socket).into())
    }

    async fn peer_connector(&self, peer: Locator) {
        let mut delay = CONNECTION_RETRY_INITIAL_PERIOD;
        loop {
            log::trace!("Trying to connect to configured peer {}", peer);
            let endpoint = EndPoint {
                locator: peer.clone(),
                config: None,
            };
            if let Ok(transport) = self.manager().open_transport(endpoint).await {
                log::debug!("Successfully connected to configured peer {}", peer);
                if let Some(orch_transport) = transport
                    .get_callback()
                    .unwrap()
                    .unwrap()
                    .as_any()
                    .downcast_ref::<super::RuntimeSession>()
                {
                    *zwrite!(orch_transport.locator) = Some(peer);
                }
                break;
            }
            log::debug!(
                "Unable to connect to configured peer {}. Retry in {} ms.",
                peer,
                delay
            );
            async_std::task::sleep(Duration::from_millis(delay)).await;
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
        mut f: F,
    ) where
        F: FnMut(Hello) -> Fut + std::marker::Send + Copy,
        Fut: Future<Output = Loop> + std::marker::Send,
        Self: Sized,
    {
        let send = async {
            let mut delay = SCOUT_INITIAL_PERIOD;
            let mut wbuf = WBuf::new(SEND_BUF_INITIAL_SIZE, false);
            let scout = TransportMessage::make_scout(Some(matcher), true, None);
            wbuf.write_transport_message(&scout);
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
                        .send_to(&ZBuf::from(&wbuf).contiguous(), mcast_addr.to_string())
                        .await
                    {
                        log::warn!(
                            "Unable to send {:?} to {} on interface {} : {}",
                            scout.body,
                            mcast_addr,
                            socket
                                .local_addr()
                                .map_or("unknown".to_string(), |addr| addr.ip().to_string()),
                            err
                        );
                    }
                }
                async_std::task::sleep(Duration::from_millis(delay)).await;
                if delay * SCOUT_PERIOD_INCREASE_FACTOR <= SCOUT_MAX_PERIOD {
                    delay *= SCOUT_PERIOD_INCREASE_FACTOR;
                }
            }
        };
        let recvs = futures::future::select_all(sockets.iter().map(move |socket| {
            async move {
                let mut buf = vec![0; RCV_BUF_SIZE];
                loop {
                    let (n, peer) = socket.recv_from(&mut buf).await.unwrap();
                    let mut zbuf = ZBuf::from(&buf[..n]);
                    if let Some(msg) = zbuf.read_transport_message() {
                        log::trace!("Received {:?} from {}", msg.body, peer);
                        if let TransportBody::Hello(hello) = &msg.body {
                            let whatami = hello.whatami.or(Some(WhatAmI::Router)).unwrap();
                            if matcher.matches(whatami) {
                                if let Loop::Break = f(hello.clone()).await {
                                    break;
                                }
                            } else {
                                log::warn!("Received unexpected Hello : {:?}", msg.body);
                            }
                        }
                    } else {
                        log::trace!("Received unexpected UDP datagram from {} : {}", peer, zbuf);
                    }
                }
            }
            .boxed()
        }));
        async_std::prelude::FutureExt::race(send, recvs).await;
    }

    async fn connect(&self, locators: &[Locator]) -> ZResult<TransportUnicast> {
        for locator in locators {
            let endpoint = EndPoint {
                locator: locator.clone(),
                config: None,
            };
            let transport = self.manager().open_transport(endpoint).await;
            if transport.is_ok() {
                return transport;
            }
        }
        zerror!(ZErrorKind::Other {
            descr: format!("Unable to connect any of {:?}", locators)
        })
    }

    pub async fn connect_peer(&self, pid: &PeerId, locators: &[Locator]) {
        if pid != &self.manager().pid() {
            if self.manager().get_transport(pid).is_none() {
                let transport = self.connect(locators).await;
                if transport.is_ok() {
                    log::debug!("Successfully connected to newly scouted {}", pid);
                } else {
                    log::warn!("Unable to connect to scouted {}", pid);
                }
            } else {
                log::trace!("Scouted already connected peer : {}", pid);
            }
        }
    }

    async fn connect_first<I: Into<WhatAmIMatcher>>(
        &self,
        sockets: &[UdpSocket],
        what: I,
        addr: &SocketAddr,
        timeout: std::time::Duration,
    ) -> ZResult<()> {
        let scout = async {
            Runtime::scout(sockets, what.into(), addr, move |hello| async move {
                log::info!("Found {:?}", hello);
                if let Some(locators) = &hello.locators {
                    if self.connect(locators).await.is_ok() {
                        log::debug!("Successfully connected to newly scouted {:?}", hello);
                        return Loop::Break;
                    }
                    log::warn!("Unable to connect to scouted {:?}", hello);
                } else {
                    log::warn!("Received Hello with no locators : {:?}", hello);
                }
                Loop::Continue
            })
            .await;
            Ok(())
        };
        let timeout = async {
            async_std::task::sleep(timeout).await;
            zerror!(ZErrorKind::Timeout {})
        };
        async_std::prelude::FutureExt::race(scout, timeout).await
    }

    async fn connect_all<I: Into<WhatAmIMatcher>>(
        &self,
        ucast_sockets: &[UdpSocket],
        what: I,
        addr: &SocketAddr,
    ) {
        Runtime::scout(ucast_sockets, what.into(), addr, move |hello| async move {
            match &hello.pid {
                Some(pid) => {
                    if let Some(locators) = &hello.locators {
                        self.connect_peer(pid, locators).await
                    } else {
                        log::warn!("Received Hello with no locators : {:?}", hello);
                    }
                }
                None => {
                    log::warn!("Received Hello with no pid : {:?}", hello);
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
            sockets
                .iter()
                .filter(|sock| sock.local_addr().is_ok())
                .max_by(|sock1, sock2| {
                    octets(addr)
                        .iter()
                        .zip(octets(&sock1.local_addr().unwrap().ip()))
                        .map(|(x, y)| x.cmp(&y))
                        .position(|ord| ord != std::cmp::Ordering::Equal)
                        .cmp(
                            &octets(addr)
                                .iter()
                                .zip(octets(&sock2.local_addr().unwrap().ip()))
                                .map(|(x, y)| x.cmp(&y))
                                .position(|ord| ord != std::cmp::Ordering::Equal),
                        )
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

            let mut zbuf = ZBuf::from(&buf[..n]);
            if let Some(msg) = zbuf.read_transport_message() {
                log::trace!("Received {:?} from {}", msg.body, peer);
                if let TransportBody::Scout(Scout {
                    what, pid_request, ..
                }) = &msg.body
                {
                    let what = what.or(Some(WhatAmI::Router.into())).unwrap();
                    if what.matches(self.whatami) {
                        let mut wbuf = WBuf::new(SEND_BUF_INITIAL_SIZE, false);
                        let pid = if *pid_request {
                            Some(self.manager().pid())
                        } else {
                            None
                        };
                        let hello = TransportMessage::make_hello(
                            pid,
                            Some(self.whatami),
                            Some(self.manager().get_locators().clone()),
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
                        wbuf.write_transport_message(&hello);
                        if let Err(err) =
                            socket.send_to(&ZBuf::from(&wbuf).contiguous(), peer).await
                        {
                            log::error!("Unable to send {:?} to {} : {}", hello.body, peer, err);
                        }
                    }
                }
            } else {
                log::trace!("Received unexpected UDP datagram from {} : {}", peer, zbuf);
            }
        }
    }

    pub(super) fn closing_session(session: &RuntimeSession) {
        match session.runtime.whatami {
            WhatAmI::Client => {
                let runtime = session.runtime.clone();
                async_std::task::spawn(async move {
                    let mut delay = CONNECTION_RETRY_INITIAL_PERIOD;
                    while runtime.start_client().await.is_err() {
                        async_std::task::sleep(std::time::Duration::from_millis(delay)).await;
                        delay *= CONNECTION_RETRY_PERIOD_INCREASE_FACTOR;
                        if delay > CONNECTION_RETRY_MAX_PERIOD {
                            delay = CONNECTION_RETRY_MAX_PERIOD;
                        }
                    }
                });
            }
            _ => {
                if let Some(locator) = &*zread!(session.locator) {
                    let peers = session.runtime.config.lock().peers().clone();
                    if peers.contains(locator) {
                        let locator = locator.clone();
                        let runtime = session.runtime.clone();
                        async_std::task::spawn(
                            async move { runtime.peer_connector(locator).await },
                        );
                    }
                }
            }
        }
    }
}
