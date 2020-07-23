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
use async_std::net::UdpSocket;
use std::time::Duration;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use socket2::{Socket, Domain, Type};
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use zenoh_util::zerror;
use zenoh_protocol::io::{WBuf, RBuf};
use zenoh_protocol::core::{WhatAmI, whatami};
use zenoh_protocol::proto::{SessionMessage, SessionBody, Scout, Hello};
use zenoh_protocol::link::Locator;
use zenoh_protocol::session::SessionManager;
use crate::runtime::Config;

const RCV_BUF_SIZE: usize = 65536;
const SEND_BUF_INITIAL_SIZE: usize = 8;
const CLIENT_SCOUT_INITIAL_PERIOD: u64 = 1000; //ms
const CLIENT_SCOUT_MAX_PERIOD: u64 = 4000; //ms
const CLIENT_SCOUT_PERIOD_INCREASE_FACTOR: u64 = 2;
const PEER_SCOUT_INITIAL_PERIOD: u64 = 1000; //ms
const PEER_SCOUT_MAX_PERIOD: u64 = 8000; //ms
const PEER_SCOUT_PERIOD_INCREASE_FACTOR: u64 = 2;
const DEFAULT_LISTENER: &str = "tcp/0.0.0.0:0";
const MCAST_ADDR: &str = "224.0.0.224";
const MCAST_PORT: &str = "7447";

#[derive(Clone)]
pub struct SessionOrchestrator {
    pub whatami: WhatAmI,
    pub manager: SessionManager,
}

impl SessionOrchestrator {

    pub fn new(manager: SessionManager, whatami: WhatAmI) -> SessionOrchestrator {
        SessionOrchestrator {
            whatami,
            manager,
        }
    }

    pub async fn init(&mut self, config: Config) -> ZResult<()> {
        match self.whatami {
            whatami::CLIENT => self.init_client(config.peers, &config.multicast_interface).await,
            whatami::PEER => self.init_peer(config.listeners, config.peers, &config.multicast_interface, config.scouting_delay).await,
            whatami::BROKER => self.init_broker(config.listeners, config.peers, &config.multicast_interface).await,
            _ => {
                log::error!("Unknown mode");
                zerror!(ZErrorKind::Other{ descr: "Unknown mode".to_string()})
            }
        }
    }

    async fn init_client(&mut self, peers: Vec<Locator>, iface: &str) -> ZResult<()> {
        match peers.len() {
            0 => {
                log::info!("Scouting for router ...");
                let iface = SessionOrchestrator::get_interface(iface)?;
                let socket = SessionOrchestrator::bind_ucast_port(iface).await?;
                self.connect_first(&socket, whatami::BROKER).await
            },
            _ => {
                for locator in &peers {
                    match self.manager.open_session(&locator, &None).await {
                        Ok(_) => {return Ok (())},
                        Err(err) => log::warn!("Unable to connect to {}! {}", locator, err)
                    }
                }
                log::error!("Unable to connect to any of {:?}! ", peers);
                zerror!(ZErrorKind::IOError{ descr: "".to_string()})
            },
        }
    }

    pub async fn init_peer(&mut self, mut listeners: Vec<Locator>, peers: Vec<Locator>, iface: &str, delay: Duration) -> ZResult<()> {

        if listeners.is_empty() {
            listeners.push(DEFAULT_LISTENER.parse().unwrap());
        }
        self.bind_listeners(&listeners).await?;

        let this = self.clone();
        async_std::task::spawn( async move { this.connector(peers).await });

        let mcast_socket = SessionOrchestrator::bind_mcast_port().await?;
        let iface = SessionOrchestrator::get_interface(iface)?;
        let ucast_socket = SessionOrchestrator::bind_ucast_port(iface).await?;
        let this = self.clone();
        async_std::task::spawn( async move {
            async_std::prelude::FutureExt::race(
                this.responder(&mcast_socket, &ucast_socket),
                this.scout(&ucast_socket, whatami::PEER | whatami::BROKER)
            ).await; 
        });
        async_std::task::sleep(delay).await;
        Ok(())
    }

    pub async fn init_broker(&mut self, listeners: Vec<Locator>, peers: Vec<Locator>, iface: &str) -> ZResult<()> {

        self.bind_listeners(&listeners).await?;

        let this = self.clone();
        async_std::task::spawn( async move { this.connector(peers).await });

        let mcast_socket = SessionOrchestrator::bind_mcast_port().await?;
        let iface = SessionOrchestrator::get_interface(iface)?;
        let ucast_socket = SessionOrchestrator::bind_ucast_port(iface).await?;
        let this = self.clone();
        async_std::task::spawn( async move { this.responder(&mcast_socket, &ucast_socket).await;  });
        Ok(()) 
    }

    async fn bind_listeners(&self, listeners: &[Locator]) -> ZResult<()> {
        for locator in listeners {
            match self.manager.add_locator(&locator).await {
                Ok(locator) => log::info!("Listening on {}!", locator),
                Err(err) => {
                    log::error!("Unable to open listener {} : {}", locator, err);
                    return zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)
                },
            }
        }
        Ok(())
    }

    fn get_interface(name: &str) -> ZResult<IpAddr> {
        if name == "auto" {
            match zenoh_util::net::get_default_multicast_interface() {
                Some(addr) => Ok(addr),
                None => {
                    log::warn!("Unable to find active, non-loopback multicast interface. Will use 0.0.0.0");
                    Ok(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)))
                },
            }
        } else {
            match name.parse::<IpAddr>() {
                Ok(addr) => Ok(addr),
                Err(_) => {
                    match zenoh_util::net::get_interface(name) {
                        Ok(opt_addr) => {
                                match opt_addr {
                                Some(addr) => Ok(addr),
                                None => {
                                    log::error!("Unable to find interface {}", name);
                                    zerror!(ZErrorKind::IOError{ descr: format!("Unable to find interface {}", name) })
                                }
                            }
                        },
                        Err(err) => {
                            log::error!("Unable to find interface {} : {}", name, err);
                            zerror!(ZErrorKind::IOError{ descr: format!("Unable to find interface {} : {}", name, err) })
                        },
                    }
                }
            }
        }
    }

    async fn bind_mcast_port() -> ZResult<UdpSocket> {
        let socket = match Socket::new(Domain::ipv4(), Type::dgram(), None) {
            Ok(socket) => {socket},
            Err(err) => {
                log::error!("Unable to create datagram socket : {}", err);
                return zerror!(ZErrorKind::IOError{ descr: "Unable to create datagram socket".to_string()}, err)
            },
        };
        if let Err(err) = socket.set_reuse_address(true) {
            log::error!("Unable to set SO_REUSEADDR option : {}", err);
            return zerror!(ZErrorKind::IOError{ descr: "Unable to set SO_REUSEADDR option".to_string()}, err)
        }
        let addr = {
            #[cfg(unix)] { MCAST_ADDR.parse().unwrap() } // See UNIX Network Programmping p.212
            #[cfg(windows)] { IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)) }
        };
        match socket.bind(&SocketAddr::new(addr, MCAST_PORT.parse().unwrap()).into()) {
            Ok(()) => log::debug!("UDP port bound to {}:{}", addr, MCAST_PORT),
            Err(err) => {
                log::error!("Unable to bind udp port {}:{} : {}", addr, MCAST_PORT, err);
                return zerror!(ZErrorKind::IOError{ descr: format!("Unable to bind udp port {}:{}", addr, MCAST_PORT)}, err)
            },
        }
        match socket.join_multicast_v4(&MCAST_ADDR.parse::<Ipv4Addr>().unwrap(), &Ipv4Addr::new(0, 0, 0, 0)) {
            Ok(()) => log::debug!("Joined multicast group {}", MCAST_ADDR),
            Err(err) => {
                log::error!("Unable to join multicast group {} : {}", MCAST_ADDR, err);
                return zerror!(ZErrorKind::IOError{ descr: format!("Unable to join multicast group {}", MCAST_ADDR)}, err)
            },
        }
        Ok(socket.into_udp_socket().into())
    }

    async fn bind_ucast_port(addr: IpAddr) -> ZResult<UdpSocket> {
        let socket = match Socket::new(Domain::ipv4(), Type::dgram(), None) {
            Ok(socket) => {socket},
            Err(err) => {
                log::error!("Unable to create datagram socket : {}", err);
                return zerror!(ZErrorKind::IOError{ descr: "Unable to create datagram socket".to_string()}, err)
            },
        };
        match socket.bind(&SocketAddr::new(addr, 0).into()) {
            Ok(()) => {
                #[allow(clippy::or_fun_call)]
                let local_addr = socket.local_addr()
                    .or::<std::io::Error>(Ok(SocketAddr::new(addr, 0).into())).unwrap().as_std()
                    .or(Some(SocketAddr::new(addr, 0))).unwrap();
                log::debug!("UDP port bound to {}", local_addr);
            },
            Err(err) => {
                log::error!("Unable to bind udp port {}:0 : {}", addr.to_string(), err);
                return zerror!(ZErrorKind::IOError{ descr: format!("Unable to bind udp port {}:0", addr.to_string())}, err)
            }
        }
        Ok(socket.into_udp_socket().into())
    }

    async fn connect_first(&self, socket: &UdpSocket, what: WhatAmI) -> ZResult<()> {
        let send = async {
            let mut delay = CLIENT_SCOUT_INITIAL_PERIOD;
            let mut wbuf = WBuf::new(SEND_BUF_INITIAL_SIZE, false);
            wbuf.write_session_message(&SessionMessage::make_scout(Some(what), true, false, None));
            loop{
                log::trace!("Send scout to {}:{}", MCAST_ADDR, MCAST_PORT);
                if let Err(err) = socket.send_to(&RBuf::from(&wbuf).to_vec(), [MCAST_ADDR, MCAST_PORT].join(":")).await {
                    log::error!("Unable to send scout to {}:{} : {}", MCAST_ADDR, MCAST_PORT, err);
                    return zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)
                }
                async_std::task::sleep(Duration::from_millis(delay)).await;
                if delay * CLIENT_SCOUT_PERIOD_INCREASE_FACTOR <= CLIENT_SCOUT_MAX_PERIOD {
                    delay *= CLIENT_SCOUT_PERIOD_INCREASE_FACTOR;
                }
            }
        };
        let recv = async {
            let mut buf = vec![0; RCV_BUF_SIZE];
            loop {
                let (n, _peer) = socket.recv_from(&mut buf).await.unwrap();
                let mut rbuf = RBuf::from(&buf[..n]);
                log::trace!("Received UDP datagram {}", rbuf);
                if let Ok(msg) = rbuf.read_session_message() {
                    log::trace!("Received {:?}", msg);
                    if let SessionBody::Hello(Hello{whatami, locators, ..}) = msg.get_body() {
                        let whatami = whatami.or(Some(whatami::BROKER)).unwrap();
                        if whatami & what != 0 {
                            log::info!("Found {:?}", msg);
                            if let Some(locators) = locators {
                                for locator in locators {
                                    if self.manager.open_session(locator, &None).await.is_ok() {
                                        log::debug!("Successfully connected to newly scouted {:?}", msg);
                                        return Ok(())
                                    }
                                }
                                log::warn!("Unable to connect to scouted {:?}", msg);
                            } else { log::warn!("Received hello with no locators : {:?}", msg); }
                        } else {
                            log::warn!("Received unexpected hello : {:?}", msg);
                        }
                    }
                }
            }    
        };
        async_std::prelude::FutureExt::race(send, recv).await
    }

    // @TODO try to reconnect on disconnection
    async fn connector(&self, peers: Vec<Locator>) {
        futures::future::join_all(
            peers.into_iter().map(|peer| { async move {
                loop {
                    log::trace!("Trying to connect to configured peer {}", peer);
                    if self.manager.open_session(&peer, &None).await.is_ok() {
                        log::debug!("Successfully connected to configured peer {}", peer);
                        break;
                    } else {
                        log::warn!("Unable to connect to configured peer {}", peer);
                    }
                    async_std::task::sleep(Duration::new(5, 0)).await;
                }
            }})
        ).await;
    }

    async fn scout(&self, ucast_socket: &UdpSocket, what: WhatAmI) {
        let send = async {
            let mut delay = PEER_SCOUT_INITIAL_PERIOD;
            let mut wbuf = WBuf::new(SEND_BUF_INITIAL_SIZE, false);
            wbuf.write_session_message(&SessionMessage::make_scout(Some(what), true, false, None));
            loop{
                log::trace!("Send scout to {}:{}", MCAST_ADDR, MCAST_PORT);
                if let Err(err) = ucast_socket.send_to(&RBuf::from(&wbuf).to_vec(), [MCAST_ADDR, MCAST_PORT].join(":")).await {
                    log::error!("Unable to send scout to {}:{} : {}", MCAST_ADDR, MCAST_PORT, err);
                }
                async_std::task::sleep(Duration::from_millis(delay)).await;
                if delay * PEER_SCOUT_PERIOD_INCREASE_FACTOR <= PEER_SCOUT_MAX_PERIOD {
                    delay *= PEER_SCOUT_PERIOD_INCREASE_FACTOR;
                }
            }
        };
        let recv = async {
            let mut buf = vec![0; RCV_BUF_SIZE];
            loop {
                let (n, _peer) = ucast_socket.recv_from(&mut buf).await.unwrap();
                let mut rbuf = RBuf::from(&buf[..n]);
                log::trace!("Received UDP datagram {}", rbuf);
                if let Ok(msg) = rbuf.read_session_message() {
                    log::trace!("Received {:?}", msg);
                    if let SessionBody::Hello(Hello{pid, whatami, locators}) = msg.get_body() {
                        let whatami = whatami.or(Some(whatami::BROKER)).unwrap();
                        if whatami & what != 0 {
                            match pid {
                                Some(pid) => {
                                    if pid != &self.manager.pid() {
                                        if self.manager.get_session(pid).await.is_none() {
                                            if let Some(locators) = locators {
                                                let mut success = false;
                                                for locator in locators {
                                                    if self.manager.open_session(locator, &None).await.is_ok() {
                                                        log::debug!("Successfully connected to newly scouted {:?}", msg);
                                                        success = true;
                                                        break;
                                                    }
                                                }
                                                if !success { log::warn!("Unable to connect to scouted {:?}", msg); }
                                            } else { log::warn!("Received hello with no locators : {:?}", msg); }
                                        } else { log::trace!("Scouted already connected peer : {:?}", msg); }
                                    }
                                }
                                None => { log::warn!("Received hello with no pid : {:?}", msg); }
                            }
                        } else { log::warn!("Received unexpected hello : {:?}", msg); }
                    }
                }
            }    
        };
        async_std::prelude::FutureExt::race(send, recv).await;
    }

    #[allow(unreachable_patterns)]
    async fn get_local_locators(&self) -> Vec<Locator> {
        let mut result = vec![];
        for locator in self.manager.get_locators().await {
            match locator {
                Locator::Tcp(addr) => {
                    if addr.ip() == Ipv4Addr::new(0, 0, 0, 0) {
                        match zenoh_util::net::get_local_addresses() {
                            Ok(ipaddrs) => {
                                for ipaddr in ipaddrs {
                                    if ! ipaddr.is_loopback() && ipaddr.is_ipv4() {
                                        result.push(format!("tcp/{}:{}", ipaddr.to_string(), addr.port()).parse().unwrap());
                                    }
                                }
                            },
                            Err(err) => log::error!("Unable to get local addresses : {}", err),
                        }
                    } else {
                        result.push(locator)
                    }
                },
                loc => result.push(loc),
            }
        }
        result
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
            if let Ok(msg) = rbuf.read_session_message() {
                log::trace!("Received {:?}", msg);
                if let SessionBody::Scout(Scout{what, pid_replies, ..}) = msg.get_body() {
                    let what = what.or(Some(whatami::BROKER)).unwrap();
                    if what & self.whatami != 0 {
                        let mut wbuf = WBuf::new(SEND_BUF_INITIAL_SIZE, false);
                        let pid  = if *pid_replies { Some(self.manager.pid()) } else { None };
                        let hello = SessionMessage::make_hello( pid, Some(self.whatami), 
                            Some(self.get_local_locators().await.clone()), None);
                        log::trace!("Send {:?} to {}", hello, peer);
                        wbuf.write_session_message(&hello);
                        if let Err(err) = ucast_socket.send_to(&RBuf::from(&wbuf).to_vec(), peer).await {
                            log::error!("Unable to send {:?} to {} : {}", hello, peer, err);
                        }
                    }
                }
            }
        }
    }

    pub async fn close(&mut self) -> ZResult<()> {
        log::trace!("SessionOrchestrator::close())");
        for session in &mut self.manager.get_sessions().await {
            session.close().await?;
        }
        Ok(())
    }
}