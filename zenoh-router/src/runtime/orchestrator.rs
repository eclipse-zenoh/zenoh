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
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use zenoh_util::zerror;
use zenoh_protocol::io::{WBuf, RBuf};
use zenoh_protocol::proto::{WhatAmI, whatami, SessionMessage, SessionBody};
use zenoh_protocol::link::Locator;
use zenoh_protocol::session::SessionManager;

const MCAST_ADDR: &str = "239.255.0.1";
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

    pub async fn init(&mut self, listeners: Vec<Locator>, peers: Vec<Locator>, iface: &str, delay: Duration) -> ZResult<()> {
        match self.whatami {
            whatami::CLIENT => self.init_client(peers, iface).await,
            whatami::PEER => self.init_peer(listeners, peers, iface, delay).await,
            whatami::BROKER => self.init_broker(listeners, peers, iface).await,
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
                let iface = match SessionOrchestrator::get_interface(iface) {
                    Ok(iface) => iface,
                    Err(err) => {return zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)},
                };
                match SessionOrchestrator::bind_ucast_port(iface).await {
                    Ok(socket) => {
                        self.connect_first(&socket, whatami::BROKER).await
                    }, 
                    Err(err) => {zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)},
                }
            },
            _ => {
                for locator in &peers {
                    match self.manager.open_session(&locator, &None).await {
                        Ok(_) => {return Ok (())},
                        Err(err) => log::warn!("Unable to connect to {}! {:?}", locator, err)
                    }
                }
                log::error!("Unable to connect to any of {:?}! ", peers);
                zerror!(ZErrorKind::IOError{ descr: "".to_string()})
            },
        }
    }

    pub async fn init_peer(&mut self, mut listeners: Vec<Locator>, peers: Vec<Locator>, iface: &str, delay: Duration) -> ZResult<()> {

        if listeners.is_empty() {
            listeners.push("tcp/0.0.0.0:0".parse().unwrap());
        }

        for locator in &listeners {
            match self.manager.add_locator(&locator).await {
                Ok(locator) => log::info!("Listening on {}!", locator),
                Err(err) => {
                    log::error!("Unable to open listener {} : {:?}", locator, err);
                    return zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)
                },
            }
        }

        {
            let this = self.clone();
            async_std::task::spawn(async move { this.connector(peers).await });
        }

        let res = match SessionOrchestrator::bind_mcast_port().await {
            Ok(mcast_socket) => {
                let iface = match SessionOrchestrator::get_interface(iface) {
                    Ok(iface) => iface,
                    Err(err) => {return zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)},
                };
                match SessionOrchestrator::bind_ucast_port(iface).await {
                    Ok(ucast_socket) => {
                        let this = self.clone();
                        async_std::task::spawn( async move {
                            async_std::prelude::FutureExt::race(
                                this.responder(&mcast_socket, &ucast_socket),
                                this.scout(&ucast_socket, whatami::PEER | whatami::BROKER)
                            ).await; 
                        });
                        Ok(()) 
                    }, 
                    Err(err) => {zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)},
                }
            },
            Err(err) => {zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)},
        };
        async_std::task::sleep(delay).await;
        res
    }

    pub async fn init_broker(&mut self, listeners: Vec<Locator>, peers: Vec<Locator>, iface: &str) -> ZResult<()> {
        for locator in &listeners {
            match self.manager.add_locator(&locator).await {
                Ok(locator) => log::info!("Listening on {}!", locator),
                Err(err) => {
                    log::error!("Unable to open listener {} : {:?}", locator, err);
                    return zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)
                },
            }
        }

        {
            let this = self.clone();
            async_std::task::spawn(async move { this.connector(peers).await });
        }

        match SessionOrchestrator::bind_mcast_port().await {
            Ok(mcast_socket) => {
                let iface = match SessionOrchestrator::get_interface(iface) {
                    Ok(iface) => iface,
                    Err(err) => {return zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)},
                };
                match SessionOrchestrator::bind_ucast_port(iface).await {
                    Ok(ucast_socket) => {
                        let this = self.clone();
                        async_std::task::spawn( async move {
                            this.responder(&mcast_socket, &ucast_socket).await; 
                        });
                        Ok(()) 
                    }, 
                    Err(err) => {zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)},
                }
            },
            Err(err) => {zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)},
        }
    }

    fn get_interface(name: &str) -> ZResult<IpAddr> {
        if name == "auto" {
            for iface in pnet::datalink::interfaces() {
                if !iface.is_loopback() && iface.is_multicast() {
                    for ip in iface.ips {
                        if ip.is_ipv4() { return Ok(ip.ip()) }
                    }
                }
            }
            log::warn!("Unable to find non-loopback multicast interface. Will use 0.0.0.0");
            Ok(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)))
        } else {
            for iface in pnet::datalink::interfaces() {
                if iface.name == name {
                    for ip in &iface.ips {
                        if ip.is_ipv4() { return Ok(ip.ip()) }
                    }
                }
                for ip in &iface.ips {
                    if ip.ip().to_string() == name { return Ok(ip.ip()) }
                }
            }
            log::error!("Unable to find interface : {}", name);
            zerror!(ZErrorKind::IOError{ descr: format!("Unable to find interface : {}", name)})
        }
    }

    async fn bind_mcast_port() -> async_std::io::Result<UdpSocket> {
        unsafe {
            let options = [(libc::SO_REUSEADDR, &1 as *const _ as *const libc::c_void)].to_vec();
            match zenoh_util::net::bind_udp([MCAST_ADDR, MCAST_PORT].join(":"), options).await {
                Ok(socket) => {
                    match socket.join_multicast_v4(MCAST_ADDR.parse().unwrap(), std::net::Ipv4Addr::new(0, 0, 0, 0)) {
                        Ok(()) => {Ok(socket)},
                        Err(err) => {Err(err)}
                    }
                },
                err => {err}
            }
        }
    }

    async fn bind_ucast_port(addr: IpAddr) -> async_std::io::Result<UdpSocket> {
        unsafe {
            match zenoh_util::net::bind_udp(SocketAddr::new(addr, 0), vec![]).await {
                Ok(socket) => {Ok(socket)},
                err => {err}
            }
        }
    }

    async fn connect_first(&self, socket: &UdpSocket, what: WhatAmI) -> ZResult<()> {
        let send = async {
            let mut wbuf = WBuf::new(8, false);
            wbuf.write_session_message(&SessionMessage::make_scout(Some(what), true, false, None));
            loop{
                log::trace!("Send scout to {}:{}", MCAST_ADDR, MCAST_PORT);
                if let Err(err) = socket.send_to(&RBuf::from(&wbuf).to_vec(), [MCAST_ADDR, MCAST_PORT].join(":")).await {
                    log::error!("Unable to send scout to {}:{} : {}", MCAST_ADDR, MCAST_PORT, err);
                    return zerror!(ZErrorKind::IOError{ descr: "".to_string()}, err)
                }
                async_std::task::sleep(std::time::Duration::new(5, 0)).await;
            }
        };
        let recv = async {
            let mut buf = vec![0; 65536];
            loop {
                let (n, _peer) = socket.recv_from(&mut buf).await.unwrap();
                let mut rbuf = RBuf::from(&buf[..n]);
                log::trace!("Received UDP datagram {}", rbuf);
                if let Ok(msg) = rbuf.read_session_message() {
                    log::trace!("Received {:?}", msg);
                    if let SessionBody::Hello{whatami, locators, ..} = msg.get_body() {
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
            let mut wbuf = WBuf::new(8, false);
            wbuf.write_session_message(&SessionMessage::make_scout(Some(what), true, false, None));
            loop{
                log::trace!("Send scout to {}:{}", MCAST_ADDR, MCAST_PORT);
                if let Err(err) = ucast_socket.send_to(&RBuf::from(&wbuf).to_vec(), [MCAST_ADDR, MCAST_PORT].join(":")).await {
                    log::error!("Unable to send scout to {}:{} : {}", MCAST_ADDR, MCAST_PORT, err);
                }
                async_std::task::sleep(std::time::Duration::new(5, 0)).await;
            }
        };
        let recv = async {
            let mut buf = vec![0; 65536];
            loop {
                let (n, _peer) = ucast_socket.recv_from(&mut buf).await.unwrap();
                let mut rbuf = RBuf::from(&buf[..n]);
                log::trace!("Received UDP datagram {}", rbuf);
                if let Ok(msg) = rbuf.read_session_message() {
                    log::trace!("Received {:?}", msg);
                    if let SessionBody::Hello{pid, whatami, locators} = msg.get_body() {
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
                    if addr.ip() == std::net::Ipv4Addr::new(0, 0, 0, 0) {
                        for iface in pnet::datalink::interfaces() {
                            if !iface.is_loopback() {
                                for ip in iface.ips {
                                    if ip.ip().is_ipv4() {
                                        result.push(format!("tcp/{}:{}", ip.ip().to_string(), addr.port()).parse().unwrap());
                                    }
                                }
                            }
                        }
                    }
                },
                loc => result.push(loc),
            }
        }
        result
    }

    async fn responder(&self, mcast_socket: &UdpSocket, ucast_socket: &UdpSocket) {
        let mut buf = vec![0; 65536];
        log::debug!("Waiting for UDP datagram...");
        loop {
            let (n, peer) = mcast_socket.recv_from(&mut buf).await.unwrap();
            let mut rbuf = RBuf::from(&buf[..n]);
            log::trace!("Received UDP datagram {}", rbuf);
            if let Ok(msg) = rbuf.read_session_message() {
                log::debug!("Received {:?}", msg);
                if let SessionBody::Scout{what, pid_replies, ..} = msg.get_body() {
                    let what = what.or(Some(whatami::BROKER)).unwrap();
                    if what & self.whatami != 0 {
                        let mut wbuf = WBuf::new(8, false);
                        let pid  = if *pid_replies { Some(self.manager.pid()) } else { None };
                        let hello = SessionMessage::make_hello( pid, Some(self.whatami), 
                            Some(self.get_local_locators().await.clone()), None);
                        log::debug!("Send {:?} to {}", hello, peer);
                        wbuf.write_session_message(&hello);
                        if let Err(err) = ucast_socket.send_to(&RBuf::from(&wbuf).to_vec(), peer).await {
                            log::error!("Unable to send {:?} to {} : {:?}", hello, peer, err);
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