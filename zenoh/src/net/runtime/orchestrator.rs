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
use super::protocol::core::{whatami, PeerId, WhatAmI};
use super::protocol::io::{RBuf, WBuf};
use super::protocol::link::{Link, Locator};
use super::protocol::proto::{
    Data, Hello, Scout, SessionBody, SessionMessage, ZenohBody, ZenohMessage,
};
use super::protocol::session::{Session, SessionEventDispatcher, SessionManager};
use super::routing::pubsub::route_data;
use super::routing::router::{LinkStateInterceptor, Router};
use async_std::net::UdpSocket;
use async_std::sync::{Arc, RwLock};
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

#[derive(Clone)]
pub struct SessionOrchestrator {
    pub whatami: WhatAmI,
    pub manager: Arc<RwLock<Option<SessionManager>>>,
    pub sub_handler: Arc<Router>,
}

impl SessionOrchestrator {
    pub fn new(whatami: WhatAmI, sub_handler: Arc<Router>) -> SessionOrchestrator {
        SessionOrchestrator {
            whatami,
            manager: Arc::new(RwLock::new(None)),
            sub_handler,
        }
    }

    pub async fn init(
        &mut self,
        manager: SessionManager,
        config: ConfigProperties,
        peers_autoconnect: bool,
    ) -> ZResult<()> {
        *self.manager.write().await = Some(manager);
        match self.whatami {
            whatami::CLIENT => self.init_client(config).await,
            whatami::PEER => self.init_peer(config, peers_autoconnect).await,
            whatami::ROUTER => self.init_broker(config).await,
            _ => {
                log::error!("Unknown mode");
                zerror!(ZErrorKind::Other {
                    descr: "Unknown mode".to_string()
                })
            }
        }
    }

    pub async fn manager(&self) -> SessionManager {
        self.manager.read().await.as_ref().unwrap().clone()
    }

    async fn init_client(&mut self, config: ConfigProperties) -> ZResult<()> {
        let peers = config
            .get_or(&ZN_PEER_KEY, "")
            .split(',')
            .filter_map(|s| match s.trim() {
                "" => None,
                s => Some(s.parse().unwrap()),
            })
            .collect::<Vec<Locator>>();
        let scouting = config
            .get_or(&ZN_MULTICAST_SCOUTING_KEY, ZN_MULTICAST_SCOUTING_DEFAULT)
            .to_lowercase()
            == ZN_TRUE;
        let addr = config
            .get_or(&ZN_MULTICAST_ADDRESS_KEY, ZN_MULTICAST_ADDRESS_DEFAULT)
            .parse()
            .unwrap();
        let iface = config.get_or(&ZN_MULTICAST_INTERFACE_KEY, ZN_MULTICAST_INTERFACE_DEFAULT);
        let timeout = std::time::Duration::from_secs_f64(
            config
                .get_or(&ZN_SCOUTING_TIMEOUT_KEY, ZN_SCOUTING_TIMEOUT_DEFAULT)
                .parse()
                .unwrap(),
        );
        match peers.len() {
            0 => {
                if scouting {
                    log::info!("Scouting for router ...");
                    let iface = SessionOrchestrator::get_interface(iface)?;
                    let socket = SessionOrchestrator::bind_ucast_port(iface).await?;
                    self.connect_first(&socket, whatami::ROUTER, &addr, timeout)
                        .await
                } else {
                    zerror!(ZErrorKind::Other {
                        descr: "No peer specified and multicast scouting desactivated!".to_string()
                    })
                }
            }
            _ => {
                for locator in &peers {
                    match self.manager().await.open_session(&locator).await {
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

    pub async fn init_peer(
        &mut self,
        config: ConfigProperties,
        peers_autoconnect: bool,
    ) -> ZResult<()> {
        let listeners = config
            .get_or(&ZN_LISTENER_KEY, PEER_DEFAULT_LISTENER)
            .split(',')
            .filter_map(|s| match s.trim() {
                "" => None,
                s => Some(s.parse().unwrap()),
            })
            .collect::<Vec<Locator>>();
        let peers = config
            .get_or(&ZN_PEER_KEY, "")
            .split(',')
            .filter_map(|s| match s.trim() {
                "" => None,
                s => Some(s.parse().unwrap()),
            })
            .collect::<Vec<Locator>>();
        let scouting = config
            .get_or(&ZN_MULTICAST_SCOUTING_KEY, ZN_MULTICAST_SCOUTING_DEFAULT)
            .to_lowercase()
            == ZN_TRUE;
        let addr = config
            .get_or(&ZN_MULTICAST_ADDRESS_KEY, ZN_MULTICAST_ADDRESS_DEFAULT)
            .parse()
            .unwrap();
        let iface = config.get_or(&ZN_MULTICAST_INTERFACE_KEY, ZN_MULTICAST_INTERFACE_DEFAULT);
        let delay = std::time::Duration::from_secs_f64(
            config
                .get_or(&ZN_SCOUTING_DELAY_KEY, ZN_SCOUTING_DELAY_DEFAULT)
                .parse()
                .unwrap(),
        );

        self.bind_listeners(&listeners).await?;

        for peer in peers {
            let this = self.clone();
            async_std::task::spawn(async move { this.peer_connector(peer).await });
        }

        if scouting {
            let mcast_socket = SessionOrchestrator::bind_mcast_port(&addr).await?;
            let iface = SessionOrchestrator::get_interface(iface)?;
            let ucast_socket = SessionOrchestrator::bind_ucast_port(iface).await?;
            let this = self.clone();
            async_std::task::spawn(async move {
                async_std::prelude::FutureExt::race(
                    this.responder(&mcast_socket, &ucast_socket),
                    this.connect_all(
                        &ucast_socket,
                        if peers_autoconnect {
                            whatami::PEER | whatami::ROUTER
                        } else {
                            whatami::ROUTER
                        },
                        &addr,
                    ),
                )
                .await;
            });
        }
        async_std::task::sleep(delay).await;
        Ok(())
    }

    pub async fn init_broker(&mut self, config: ConfigProperties) -> ZResult<()> {
        let listeners = config
            .get_or(&ZN_LISTENER_KEY, ROUTER_DEFAULT_LISTENER)
            .split(',')
            .filter_map(|s| match s.trim() {
                "" => None,
                s => Some(s.parse().unwrap()),
            })
            .collect::<Vec<Locator>>();
        let peers = config
            .get_or(&ZN_PEER_KEY, "")
            .split(',')
            .filter_map(|s| match s.trim() {
                "" => None,
                s => Some(s.parse().unwrap()),
            })
            .collect::<Vec<Locator>>();
        let scouting = config
            .get_or(&ZN_MULTICAST_SCOUTING_KEY, ZN_MULTICAST_SCOUTING_DEFAULT)
            .to_lowercase()
            == ZN_TRUE;
        let addr = config
            .get_or(&ZN_MULTICAST_ADDRESS_KEY, ZN_MULTICAST_ADDRESS_DEFAULT)
            .parse()
            .unwrap();
        let iface = config.get_or(&ZN_MULTICAST_INTERFACE_KEY, ZN_MULTICAST_INTERFACE_DEFAULT);

        self.bind_listeners(&listeners).await?;

        for peer in peers {
            let this = self.clone();
            async_std::task::spawn(async move { this.peer_connector(peer).await });
        }

        if scouting {
            let mcast_socket = SessionOrchestrator::bind_mcast_port(&addr).await?;
            let iface = SessionOrchestrator::get_interface(iface)?;
            let ucast_socket = SessionOrchestrator::bind_ucast_port(iface).await?;
            let this = self.clone();
            async_std::task::spawn(async move {
                this.responder(&mcast_socket, &ucast_socket).await;
            });
        }

        Ok(())
    }

    async fn bind_listeners(&self, listeners: &[Locator]) -> ZResult<()> {
        for listener in listeners {
            match self.manager().await.add_listener(&listener).await {
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
        for locator in self.manager().await.get_locators().await {
            log::info!("zenohd can be reached on {}", locator);
        }
        Ok(())
    }

    pub fn get_interface(name: &str) -> ZResult<IpAddr> {
        if name == "auto" {
            match zenoh_util::net::get_default_multicast_interface() {
                Some(addr) => Ok(addr),
                None => {
                    log::warn!(
                        "Unable to find active, non-loopback multicast interface. Will use 0.0.0.0"
                    );
                    Ok(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)))
                }
            }
        } else {
            match name.parse::<IpAddr>() {
                Ok(addr) => Ok(addr),
                Err(_) => match zenoh_util::net::get_interface(name) {
                    Ok(opt_addr) => match opt_addr {
                        Some(addr) => Ok(addr),
                        None => {
                            log::error!("Unable to find interface {}", name);
                            zerror!(ZErrorKind::IoError {
                                descr: format!("Unable to find interface {}", name)
                            })
                        }
                    },
                    Err(err) => {
                        log::error!("Unable to find interface {} : {}", name, err);
                        zerror!(ZErrorKind::IoError {
                            descr: format!("Unable to find interface {} : {}", name, err)
                        })
                    }
                },
            }
        }
    }

    pub async fn bind_mcast_port(sockaddr: &SocketAddr) -> ZResult<UdpSocket> {
        let socket = match Socket::new(Domain::ipv4(), Type::dgram(), None) {
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
        let join_multicast = match sockaddr.ip() {
            IpAddr::V4(addr) => socket.join_multicast_v4(&addr, &Ipv4Addr::new(0, 0, 0, 0)),
            IpAddr::V6(addr) => socket.join_multicast_v6(&addr, 0),
        };
        match join_multicast {
            Ok(()) => log::debug!("Joined multicast group {}", sockaddr.ip()),
            Err(err) => {
                log::error!("Unable to join multicast group {} : {}", sockaddr.ip(), err);
                return zerror!(
                    ZErrorKind::IoError {
                        descr: format!("Unable to join multicast group {}", sockaddr.ip())
                    },
                    err
                );
            }
        }
        log::info!("zenohd listening scout messages on {}", sockaddr);
        Ok(socket.into_udp_socket().into())
    }

    pub async fn bind_ucast_port(addr: IpAddr) -> ZResult<UdpSocket> {
        let socket = match Socket::new(Domain::ipv4(), Type::dgram(), None) {
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
        match socket.bind(&SocketAddr::new(addr, 0).into()) {
            Ok(()) => {
                #[allow(clippy::or_fun_call)]
                let local_addr = socket
                    .local_addr()
                    .or::<std::io::Error>(Ok(SocketAddr::new(addr, 0).into()))
                    .unwrap()
                    .as_std()
                    .or(Some(SocketAddr::new(addr, 0)))
                    .unwrap();
                log::debug!("UDP port bound to {}", local_addr);
            }
            Err(err) => {
                log::error!("Unable to bind udp port {}:0 : {}", addr.to_string(), err);
                return zerror!(
                    ZErrorKind::IoError {
                        descr: format!("Unable to bind udp port {}:0", addr.to_string())
                    },
                    err
                );
            }
        }
        Ok(socket.into_udp_socket().into())
    }

    async fn peer_connector(&self, peer: Locator) {
        let mut delay = CONNECTION_RETRY_INITIAL_PERIOD;
        loop {
            log::trace!("Trying to connect to configured peer {}", peer);
            if let Ok(session) = self.manager().await.open_session(&peer).await {
                log::debug!("Successfully connected to configured peer {}", peer);
                if let SessionEventDispatcher::OrchSession(orch_session) =
                    session.get_callback().await.unwrap().unwrap()
                {
                    *orch_session.locator.write().await = Some(peer);
                }
                break;
            }
            log::debug!(
                "Unable to connect to configured peer {}. Retry in {} ms.",
                peer,
                delay
            );
            async_std::task::sleep(Duration::from_millis(delay)).await;
            if delay * CONNECTION_RETRY_PERIOD_INCREASE_FACTOR <= CONNECTION_RETRY_MAX_PERIOD {
                delay *= CONNECTION_RETRY_PERIOD_INCREASE_FACTOR;
            }
        }
    }

    pub async fn scout<Fut, F>(socket: &UdpSocket, what: WhatAmI, mcast_addr: &SocketAddr, mut f: F)
    where
        F: FnMut(Hello) -> Fut,
        Fut: Future<Output = Loop>,
        Self: Sized,
    {
        let send = async {
            let mut delay = SCOUT_INITIAL_PERIOD;
            let mut wbuf = WBuf::new(SEND_BUF_INITIAL_SIZE, false);
            wbuf.write_session_message(&SessionMessage::make_scout(Some(what), true, None));
            loop {
                log::trace!("Send scout to {}", mcast_addr);
                if let Err(err) = socket
                    .send_to(&RBuf::from(&wbuf).to_vec(), mcast_addr.to_string())
                    .await
                {
                    log::error!("Unable to send scout to {} : {}", mcast_addr, err);
                }
                async_std::task::sleep(Duration::from_millis(delay)).await;
                if delay * SCOUT_PERIOD_INCREASE_FACTOR <= SCOUT_MAX_PERIOD {
                    delay *= SCOUT_PERIOD_INCREASE_FACTOR;
                }
            }
        };
        let recv = async {
            let mut buf = vec![0; RCV_BUF_SIZE];
            loop {
                let (n, _peer) = socket.recv_from(&mut buf).await.unwrap();
                let mut rbuf = RBuf::from(&buf[..n]);
                log::trace!("Received UDP datagram {}", rbuf);
                if let Some(msg) = rbuf.read_session_message() {
                    log::trace!("Received {:?}", msg);
                    if let SessionBody::Hello(hello) = msg.get_body() {
                        let whatami = hello.whatami.or(Some(whatami::ROUTER)).unwrap();
                        if whatami & what != 0 {
                            if let Loop::Break = f(hello.clone()).await {
                                break;
                            }
                        } else {
                            log::warn!("Received unexpected hello : {:?}", msg);
                        }
                    }
                }
            }
        };
        async_std::prelude::FutureExt::race(send, recv).await;
    }

    async fn connect(&self, locators: &[Locator]) -> ZResult<Session> {
        for locator in locators {
            let session = self.manager().await.open_session(locator).await;
            if session.is_ok() {
                return session;
            }
        }
        zerror!(ZErrorKind::Other {
            descr: format!("Unable to connect any of {:?}", locators)
        })
    }

    pub async fn connect_peer(&self, pid: &PeerId, locators: &[Locator]) {
        if pid != &self.manager().await.pid() {
            if self.manager().await.get_session(pid).await.is_none() {
                let session = self.connect(locators).await;
                if session.is_ok() {
                    log::debug!("Successfully connected to newly scouted {}", pid);
                } else {
                    log::warn!("Unable to connect to scouted {}", pid);
                }
            } else {
                log::trace!("Scouted already connected peer : {}", pid);
            }
        }
    }

    async fn connect_first(
        &self,
        socket: &UdpSocket,
        what: WhatAmI,
        addr: &SocketAddr,
        timeout: std::time::Duration,
    ) -> ZResult<()> {
        let scout = async {
            SessionOrchestrator::scout(socket, what, addr, async move |hello| {
                log::info!("Found {:?}", hello);
                if let Some(locators) = &hello.locators {
                    if self.connect(locators).await.is_ok() {
                        log::debug!("Successfully connected to newly scouted {:?}", hello);
                        return Loop::Break;
                    }
                    log::warn!("Unable to connect to scouted {:?}", hello);
                } else {
                    log::warn!("Received hello with no locators : {:?}", hello);
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

    async fn connect_all(&self, ucast_socket: &UdpSocket, what: WhatAmI, addr: &SocketAddr) {
        SessionOrchestrator::scout(ucast_socket, what, addr, async move |hello| {
            match &hello.pid {
                Some(pid) => {
                    if let Some(locators) = &hello.locators {
                        self.connect_peer(pid, locators).await
                    } else {
                        log::warn!("Received hello with no locators : {:?}", hello);
                    }
                }
                None => {
                    log::warn!("Received hello with no pid : {:?}", hello);
                }
            }
            Loop::Continue
        })
        .await
    }

    async fn responder(&self, mcast_socket: &UdpSocket, ucast_socket: &UdpSocket) {
        let mut buf = vec![0; RCV_BUF_SIZE];
        log::debug!("Waiting for UDP datagram...");
        loop {
            let (n, peer) = mcast_socket.recv_from(&mut buf).await.unwrap();
            if let Ok(local_addr) = ucast_socket.local_addr() {
                if local_addr == peer {
                    log::trace!("Ignore UDP datagram from own socket");
                    continue;
                }
            }

            let mut rbuf = RBuf::from(&buf[..n]);
            log::trace!("Received UDP datagram {}", rbuf);
            if let Some(msg) = rbuf.read_session_message() {
                log::trace!("Received {:?}", msg);
                if let SessionBody::Scout(Scout {
                    what, pid_request, ..
                }) = msg.get_body()
                {
                    let what = what.or(Some(whatami::ROUTER)).unwrap();
                    if what & self.whatami != 0 {
                        let mut wbuf = WBuf::new(SEND_BUF_INITIAL_SIZE, false);
                        let pid = if *pid_request {
                            Some(self.manager().await.pid())
                        } else {
                            None
                        };
                        let hello = SessionMessage::make_hello(
                            pid,
                            Some(self.whatami),
                            Some(self.manager().await.get_locators().await.clone()),
                            None,
                        );
                        log::trace!("Send {:?} to {}", hello, peer);
                        wbuf.write_session_message(&hello);
                        if let Err(err) = ucast_socket
                            .send_to(&RBuf::from(&wbuf).to_vec(), peer)
                            .await
                        {
                            log::error!("Unable to send {:?} to {} : {}", hello, peer, err);
                        }
                    }
                }
            }
        }
    }

    pub async fn close(&mut self) -> ZResult<()> {
        log::trace!("SessionOrchestrator::close())");
        for session in &mut self.manager().await.get_sessions().await {
            session.close().await?;
        }
        Ok(())
    }

    pub(crate) async fn new_session(&self, session: Session) -> ZResult<Arc<OrchSession>> {
        Ok(Arc::new(OrchSession {
            orchestrator: self.clone(),
            locator: RwLock::new(None),
            sub_event_handler: self.sub_handler.new_session(session).await.unwrap(),
        }))
    }
}

pub struct OrchSession {
    orchestrator: SessionOrchestrator,
    locator: RwLock<Option<Locator>>,
    sub_event_handler: Arc<LinkStateInterceptor>,
}

impl OrchSession {
    pub(crate) async fn handle_message(&self, msg: ZenohMessage) -> ZResult<()> {
        // critical path shortcut
        if msg.reply_context.is_none() {
            if let ZenohBody::Data(Data {
                key,
                data_info,
                payload,
            }) = msg.body
            {
                let (rid, suffix) = (&key).into();
                let mut tables = self.sub_event_handler.tables.write().await;
                let face = &self.sub_event_handler.demux.primitives.state;
                route_data(
                    &mut tables,
                    face,
                    rid,
                    suffix,
                    msg.congestion_control,
                    data_info,
                    payload,
                    msg.routing_context,
                )
                .await;
                Ok(())
            } else {
                self.sub_event_handler.handle_message(msg).await
            }
        } else {
            self.sub_event_handler.handle_message(msg).await
        }
    }

    pub(crate) async fn new_link(&self, link: Link) {
        self.sub_event_handler.new_link(link).await
    }

    pub(crate) async fn del_link(&self, link: Link) {
        self.sub_event_handler.del_link(link).await
    }

    pub(crate) async fn closing(&self) {
        self.sub_event_handler.closing().await;
        if let Some(locator) = &*self.locator.read().await {
            let locator = locator.clone();
            let orchestrator = self.orchestrator.clone();
            async_std::task::spawn(async move { orchestrator.peer_connector(locator).await });
        }
    }

    pub(crate) async fn closed(&self) {
        self.sub_event_handler.closed().await
    }
}
