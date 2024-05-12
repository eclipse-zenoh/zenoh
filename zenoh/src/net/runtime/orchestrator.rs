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

// filetag{rust.runtime}

use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use futures::prelude::*;
use socket2::{Domain, Socket, Type};
use tokio::net::UdpSocket;
use zenoh_buffers::{
    reader::{DidntRead, HasReader},
    writer::HasWriter,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_config::{
    get_global_connect_timeout, get_global_listener_timeout, unwrap_or_default, ModeDependent,
};
use zenoh_link::{Locator, LocatorInspector};
use zenoh_protocol::{
    core::{whatami::WhatAmIMatcher, EndPoint, WhatAmI, ZenohId},
    scouting::{Hello, Scout, ScoutingBody, ScoutingMessage},
};
use zenoh_result::{bail, zerror, ZResult};

use super::{Runtime, RuntimeSession};

const RCV_BUF_SIZE: usize = u16::MAX as usize;
const SCOUT_INITIAL_PERIOD: Duration = Duration::from_millis(1_000);
const SCOUT_MAX_PERIOD: Duration = Duration::from_millis(8_000);
const SCOUT_PERIOD_INCREASE_FACTOR: u32 = 2;
const ROUTER_DEFAULT_LISTENER: &str = "tcp/[::]:7447";
const PEER_DEFAULT_LISTENER: &str = "tcp/[::]:0";

pub(crate) enum Loop {
    Continue,
    Break,
}

impl Runtime {
    pub async fn start(&mut self) -> ZResult<()> {
        match self.whatami() {
            WhatAmI::Client => self.start_client().await,
            WhatAmI::Peer => self.start_peer().await,
            WhatAmI::Router => self.start_router().await,
        }
    }

    async fn start_client(&self) -> ZResult<()> {
        let (peers, scouting, addr, ifaces, timeout) = {
            let guard = self.state.config.lock();
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
                    tracing::info!("Scouting for router ...");
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
            _ => self.connect_peers(&peers, true).await,
        }
    }

    async fn start_peer(&self) -> ZResult<()> {
        let (listeners, peers, scouting, listen, autoconnect, addr, ifaces, delay) = {
            let guard = &self.state.config.lock();
            let listeners = if guard.listen().endpoints().is_empty() {
                let endpoint: EndPoint = PEER_DEFAULT_LISTENER.parse().unwrap();
                let protocol = endpoint.protocol();
                let mut listeners = vec![];
                if self
                    .state
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

        self.connect_peers(&peers, false).await?;

        if scouting {
            self.start_scout(listen, autoconnect, addr, ifaces).await?;
        }
        tokio::time::sleep(delay).await;
        Ok(())
    }

    async fn start_router(&self) -> ZResult<()> {
        let (listeners, peers, scouting, listen, autoconnect, addr, ifaces) = {
            let guard = self.state.config.lock();
            let listeners = if guard.listen().endpoints().is_empty() {
                let endpoint: EndPoint = ROUTER_DEFAULT_LISTENER.parse().unwrap();
                let protocol = endpoint.protocol();
                let mut listeners = vec![];
                if self
                    .state
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

        self.connect_peers(&peers, false).await?;

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
                        self.spawn_abortable(async move {
                            tokio::select! {
                                _ = this.responder(&mcast_socket, &sockets) => {},
                                _ = this.connect_all(&sockets, autoconnect, &addr) => {},
                            }
                        });
                    }
                    (true, true) => {
                        self.spawn_abortable(async move {
                            this.responder(&mcast_socket, &sockets).await;
                        });
                    }
                    (false, false) => {
                        self.spawn_abortable(async move {
                            this.connect_all(&sockets, autoconnect, &addr).await
                        });
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    async fn connect_peers(&self, peers: &[EndPoint], single_link: bool) -> ZResult<()> {
        let timeout = self.get_global_connect_timeout();
        if timeout.is_zero() {
            self.connect_peers_impl(peers, single_link).await
        } else {
            let res = tokio::time::timeout(timeout, async {
                self.connect_peers_impl(peers, single_link).await.ok()
            })
            .await;
            match res {
                Ok(_) => Ok(()),
                Err(_) => {
                    let e = zerror!(
                        "{:?} Unable to connect to any of {:?}! ",
                        self.manager().get_locators(),
                        peers
                    );
                    tracing::error!("{}", &e);
                    Err(e.into())
                }
            }
        }
    }

    async fn connect_peers_impl(&self, peers: &[EndPoint], single_link: bool) -> ZResult<()> {
        if single_link {
            self.connect_peers_single_link(peers).await
        } else {
            self.connect_peers_multiply_links(peers).await
        }
    }

    async fn connect_peers_single_link(&self, peers: &[EndPoint]) -> ZResult<()> {
        for peer in peers {
            let endpoint = peer.clone();
            let retry_config = self.get_connect_retry_config(&endpoint);
            tracing::debug!(
                "Try to connect: {:?}: global timeout: {:?}, retry: {:?}",
                endpoint,
                self.get_global_connect_timeout(),
                retry_config
            );
            if retry_config.timeout().is_zero() || self.get_global_connect_timeout().is_zero() {
                // try to connect and exit immediately without retry
                if self
                    .peer_connector(endpoint, retry_config.timeout())
                    .await
                    .is_ok()
                {
                    return Ok(());
                }
            } else {
                // try to connect with retry waiting
                self.peer_connector_retry(endpoint).await;
                return Ok(());
            }
        }
        let e = zerror!(
            "{:?} Unable to connect to any of {:?}! ",
            self.manager().get_locators(),
            peers
        );
        tracing::error!("{}", &e);
        Err(e.into())
    }

    async fn connect_peers_multiply_links(&self, peers: &[EndPoint]) -> ZResult<()> {
        for peer in peers {
            let endpoint = peer.clone();
            let retry_config = self.get_connect_retry_config(&endpoint);
            tracing::debug!(
                "Try to connect: {:?}: global timeout: {:?}, retry: {:?}",
                endpoint,
                self.get_global_connect_timeout(),
                retry_config
            );
            if retry_config.timeout().is_zero() || self.get_global_connect_timeout().is_zero() {
                // try to connect and exit immediately without retry
                if let Err(e) = self.peer_connector(endpoint, retry_config.timeout()).await {
                    if retry_config.exit_on_failure {
                        return Err(e);
                    }
                }
            } else if retry_config.exit_on_failure {
                // try to connect with retry waiting
                self.peer_connector_retry(endpoint).await;
            } else {
                // try to connect in background
                self.spawn_peer_connector(endpoint).await?
            }
        }
        Ok(())
    }

    async fn peer_connector(&self, peer: EndPoint, timeout: std::time::Duration) -> ZResult<()> {
        match tokio::time::timeout(timeout, self.manager().open_transport_unicast(peer.clone()))
            .await
        {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                tracing::warn!("Unable to connect to {}! {}", peer, e);
                Err(e)
            }
            Err(e) => {
                tracing::warn!("Unable to connect to {}! {}", peer, e);
                Err(e.into())
            }
        }
    }

    pub(crate) async fn update_peers(&self) -> ZResult<()> {
        let peers = { self.state.config.lock().connect().endpoints().clone() };
        let tranports = self.manager().get_transports_unicast().await;

        if self.state.whatami == WhatAmI::Client {
            for transport in tranports {
                let should_close = if let Ok(Some(orch_transport)) = transport.get_callback() {
                    if let Some(orch_transport) = orch_transport
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
                    if let Ok(Some(orch_transport)) = transport.get_callback() {
                        if let Some(orch_transport) = orch_transport
                            .as_any()
                            .downcast_ref::<super::RuntimeSession>()
                        {
                            if let Some(endpoint) = &*zread!(orch_transport.endpoint) {
                                return *endpoint == peer;
                            }
                        }
                    }
                    false
                }) {
                    self.spawn_peer_connector(peer).await?;
                }
            }
        }

        Ok(())
    }

    fn get_listen_retry_config(&self, endpoint: &EndPoint) -> zenoh_config::ConnectionRetryConf {
        let guard = &self.state.config.lock();
        zenoh_config::get_retry_config(guard, Some(endpoint), true)
    }

    fn get_connect_retry_config(&self, endpoint: &EndPoint) -> zenoh_config::ConnectionRetryConf {
        let guard = &self.state.config.lock();
        zenoh_config::get_retry_config(guard, Some(endpoint), false)
    }

    fn get_global_connect_retry_config(&self) -> zenoh_config::ConnectionRetryConf {
        let guard = &self.state.config.lock();
        zenoh_config::get_retry_config(guard, None, false)
    }

    fn get_global_listener_timeout(&self) -> std::time::Duration {
        let guard = &self.state.config.lock();
        get_global_listener_timeout(guard)
    }

    fn get_global_connect_timeout(&self) -> std::time::Duration {
        let guard = &self.state.config.lock();
        get_global_connect_timeout(guard)
    }

    async fn bind_listeners(&self, listeners: &[EndPoint]) -> ZResult<()> {
        let timeout = self.get_global_listener_timeout();
        if timeout.is_zero() {
            self.bind_listeners_impl(listeners).await
        } else {
            let res = tokio::time::timeout(timeout, async {
                self.bind_listeners_impl(listeners).await.ok()
            })
            .await;
            match res {
                Ok(_) => Ok(()),
                Err(e) => {
                    tracing::error!("Unable to open listeners: {}", e);
                    Err(Box::new(e))
                }
            }
        }
    }

    async fn bind_listeners_impl(&self, listeners: &[EndPoint]) -> ZResult<()> {
        for listener in listeners {
            let endpoint = listener.clone();
            let retry_config = self.get_listen_retry_config(&endpoint);
            tracing::debug!("Try to add listener: {:?}: {:?}", endpoint, retry_config);
            if retry_config.timeout().is_zero() || self.get_global_listener_timeout().is_zero() {
                // try to add listener and exit immediately without retry
                if let Err(e) = self.add_listener(endpoint).await {
                    if retry_config.exit_on_failure {
                        return Err(e);
                    }
                };
            } else if retry_config.exit_on_failure {
                // try to add listener with retry waiting
                self.add_listener_retry(endpoint, retry_config).await
            } else {
                // try to add listener in background
                self.spawn_add_listener(endpoint, retry_config).await
            }
        }
        self.print_locators();
        Ok(())
    }

    async fn spawn_add_listener(
        &self,
        listener: EndPoint,
        retry_config: zenoh_config::ConnectionRetryConf,
    ) {
        let this = self.clone();
        self.spawn(async move {
            this.add_listener_retry(listener, retry_config).await;
            this.print_locators();
        });
    }

    async fn add_listener_retry(
        &self,
        listener: EndPoint,
        retry_config: zenoh_config::ConnectionRetryConf,
    ) {
        let mut period = retry_config.period();
        loop {
            if self.add_listener(listener.clone()).await.is_ok() {
                break;
            }
            tokio::time::sleep(period.next_duration()).await;
        }
    }

    async fn add_listener(&self, listener: EndPoint) -> ZResult<()> {
        let endpoint = listener.clone();
        match self.manager().add_listener(endpoint).await {
            Ok(listener) => tracing::debug!("Listener added: {}", listener),
            Err(err) => {
                tracing::warn!("Unable to open listener {}: {}", listener, err);
                return Err(err);
            }
        }
        Ok(())
    }

    fn print_locators(&self) {
        let mut locators = self.state.locators.write().unwrap();
        *locators = self.manager().get_locators();
        for locator in &*locators {
            tracing::info!("Zenoh can be reached at: {}", locator);
        }
    }

    pub fn get_interfaces(names: &str) -> Vec<IpAddr> {
        if names == "auto" {
            let ifaces = zenoh_util::net::get_multicast_interfaces();
            if ifaces.is_empty() {
                tracing::warn!(
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
                                tracing::error!("Unable to find interface {}", name);
                                None
                            }
                        },
                        Err(err) => {
                            tracing::error!("Unable to find interface {}: {}", name, err);
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
                tracing::error!("Unable to create datagram socket: {}", err);
                bail!(err => "Unable to create datagram socket");
            }
        };
        if let Err(err) = socket.set_reuse_address(true) {
            tracing::error!("Unable to set SO_REUSEADDR option: {}", err);
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
            Ok(()) => tracing::debug!("UDP port bound to {}", sockaddr),
            Err(err) => {
                tracing::error!("Unable to bind UDP port {}: {}", sockaddr, err);
                bail!(err => "Unable to bind UDP port {}", sockaddr);
            }
        }

        match sockaddr.ip() {
            IpAddr::V6(addr) => match socket.join_multicast_v6(&addr, 0) {
                Ok(()) => {
                    tracing::debug!("Joined multicast group {} on interface 0", sockaddr.ip())
                }
                Err(err) => {
                    tracing::error!(
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
                            Ok(()) => tracing::debug!(
                                "Joined multicast group {} on interface {}",
                                sockaddr.ip(),
                                iface_addr,
                            ),
                            Err(err) => tracing::warn!(
                                "Unable to join multicast group {} on interface {}: {}",
                                sockaddr.ip(),
                                iface_addr,
                                err,
                            ),
                        }
                    } else {
                        tracing::warn!(
                            "Cannot join IpV4 multicast group {} on IpV6 iface {}",
                            sockaddr.ip(),
                            iface
                        );
                    }
                }
            }
        }
        tracing::info!("zenohd listening scout messages on {}", sockaddr);

        // Must set to nonblocking according to the doc of tokio
        // https://docs.rs/tokio/latest/tokio/net/struct.UdpSocket.html#notes
        socket.set_nonblocking(true)?;

        // UdpSocket::from_std requires a runtime even though it's a sync function
        let udp_socket = zenoh_runtime::ZRuntime::Net
            .block_in_place(async { UdpSocket::from_std(socket.into()) })?;
        Ok(udp_socket)
    }

    pub fn bind_ucast_port(addr: IpAddr) -> ZResult<UdpSocket> {
        let socket = match Socket::new(Domain::IPV4, Type::DGRAM, None) {
            Ok(socket) => socket,
            Err(err) => {
                tracing::warn!("Unable to create datagram socket: {}", err);
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
                tracing::debug!("UDP port bound to {}", local_addr);
            }
            Err(err) => {
                tracing::warn!("Unable to bind udp port {}:0: {}", addr, err);
                bail!(err => "Unable to bind udp port {}:0", addr);
            }
        }

        // Must set to nonblocking according to the doc of tokio
        // https://docs.rs/tokio/latest/tokio/net/struct.UdpSocket.html#notes
        socket.set_nonblocking(true)?;

        // UdpSocket::from_std requires a runtime even though it's a sync function
        let udp_socket = zenoh_runtime::ZRuntime::Net
            .block_in_place(async { UdpSocket::from_std(socket.into()) })?;
        Ok(udp_socket)
    }

    async fn spawn_peer_connector(&self, peer: EndPoint) -> ZResult<()> {
        if !LocatorInspector::default()
            .is_multicast(&peer.to_locator())
            .await?
        {
            let this = self.clone();
            self.spawn(async move { this.peer_connector_retry(peer).await });
            Ok(())
        } else {
            bail!("Forbidden multicast endpoint in connect list!")
        }
    }

    async fn peer_connector_retry(&self, peer: EndPoint) {
        let retry_config = self.get_connect_retry_config(&peer);
        let mut period = retry_config.period();
        let cancellation_token = self.get_cancellation_token();
        loop {
            tracing::trace!("Trying to connect to configured peer {}", peer);
            let endpoint = peer.clone();
            tokio::select! {
                res = tokio::time::timeout(retry_config.timeout(), self.manager().open_transport_unicast(endpoint)) => {
                    match res {
                        Ok(Ok(transport)) => {
                            tracing::debug!("Successfully connected to configured peer {}", peer);
                            if let Ok(Some(orch_transport)) = transport.get_callback() {
                                if let Some(orch_transport) = orch_transport
                                    .as_any()
                                    .downcast_ref::<super::RuntimeSession>()
                                {
                                    *zwrite!(orch_transport.endpoint) = Some(peer);
                                }
                            }
                            break;
                        }
                        Ok(Err(e)) => {
                            tracing::debug!(
                                "Unable to connect to configured peer {}! {}. Retry in {:?}.",
                                peer,
                                e,
                                period.duration()
                            );
                        }
                        Err(e) => {
                            tracing::debug!(
                                "Unable to connect to configured peer {}! {}. Retry in {:?}.",
                                peer,
                                e,
                                period.duration()
                            );
                        }
                    }
                }
                _ = cancellation_token.cancelled() => { break; }
            }
            tokio::time::sleep(period.next_duration()).await;
        }
    }

    pub(crate) async fn scout<Fut, F>(
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

            let scout: ScoutingMessage = Scout {
                version: zenoh_protocol::VERSION,
                what: matcher,
                zid: None,
            }
            .into();
            let mut wbuf = vec![];
            let mut writer = wbuf.writer();
            let codec = Zenoh080::new();
            codec.write(&mut writer, &scout).unwrap();

            loop {
                for socket in sockets {
                    tracing::trace!(
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
                        tracing::debug!(
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
                tokio::time::sleep(delay).await;
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
                            let codec = Zenoh080::new();
                            let res: Result<ScoutingMessage, DidntRead> = codec.read(&mut reader);
                            if let Ok(msg) = res {
                                tracing::trace!("Received {:?} from {}", msg.body, peer);
                                if let ScoutingBody::Hello(hello) = &msg.body {
                                    if matcher.matches(hello.whatami) {
                                        if let Loop::Break = f(hello.clone()).await {
                                            break;
                                        }
                                    } else {
                                        tracing::warn!("Received unexpected Hello: {:?}", msg.body);
                                    }
                                }
                            } else {
                                tracing::trace!(
                                    "Received unexpected UDP datagram from {}: {:?}",
                                    peer,
                                    &buf.as_slice()[..n]
                                );
                            }
                        }
                        Err(e) => tracing::debug!("Error receiving UDP datagram: {}", e),
                    }
                }
            }
            .boxed()
        }));
        tokio::select! {
            _ = send => {},
            _ = recvs => {},
        }
    }

    #[must_use]
    async fn connect(&self, zid: &ZenohId, locators: &[Locator]) -> bool {
        const ERR: &str = "Unable to connect to newly scouted peer ";

        let inspector = LocatorInspector::default();
        for locator in locators {
            let is_multicast = match inspector.is_multicast(locator).await {
                Ok(im) => im,
                Err(e) => {
                    tracing::trace!("{} {} on {}: {}", ERR, zid, locator, e);
                    continue;
                }
            };

            let endpoint = locator.to_owned().into();
            let retry_config = self.get_connect_retry_config(&endpoint);
            let manager = self.manager();
            if is_multicast {
                match tokio::time::timeout(
                    retry_config.timeout(),
                    manager.open_transport_multicast(endpoint),
                )
                .await
                {
                    Ok(Ok(transport)) => {
                        tracing::debug!(
                            "Successfully connected to newly scouted peer: {:?}",
                            transport
                        );
                        return true;
                    }
                    Ok(Err(e)) => tracing::trace!("{} {} on {}: {}", ERR, zid, locator, e),
                    Err(e) => tracing::trace!("{} {} on {}: {}", ERR, zid, locator, e),
                }
            } else {
                match tokio::time::timeout(
                    retry_config.timeout(),
                    manager.open_transport_unicast(endpoint),
                )
                .await
                {
                    Ok(Ok(transport)) => {
                        tracing::debug!(
                            "Successfully connected to newly scouted peer: {:?}",
                            transport
                        );
                        return true;
                    }
                    Ok(Err(e)) => tracing::trace!("{} {} on {}: {}", ERR, zid, locator, e),
                    Err(e) => tracing::trace!("{} {} on {}: {}", ERR, zid, locator, e),
                }
            }
        }

        tracing::warn!(
            "Unable to connect to any locator of scouted peer {}: {:?}",
            zid,
            locators
        );
        false
    }

    pub async fn connect_peer(&self, zid: &ZenohId, locators: &[Locator]) {
        let manager = self.manager();
        if zid != &manager.zid() {
            let has_unicast = manager.get_transport_unicast(zid).await.is_some();
            let has_multicast = {
                let mut hm = manager.get_transport_multicast(zid).await.is_some();
                for t in manager.get_transports_multicast().await {
                    if let Ok(l) = t.get_link() {
                        if let Some(g) = l.group.as_ref() {
                            hm |= locators.iter().any(|l| l == g);
                        }
                    }
                }
                hm
            };

            if !has_unicast && !has_multicast {
                tracing::debug!("Try to connect to peer {} via any of {:?}", zid, locators);
                let _ = self.connect(zid, locators).await;
            } else {
                tracing::trace!("Already connected scouted peer: {}", zid);
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
                tracing::info!("Found {:?}", hello);
                if !hello.locators.is_empty() {
                    if self.connect(&hello.zid, &hello.locators).await {
                        return Loop::Break;
                    }
                } else {
                    tracing::warn!("Received Hello with no locators: {:?}", hello);
                }
                Loop::Continue
            })
            .await;
            Ok(())
        };
        let timeout = async {
            tokio::time::sleep(timeout).await;
            bail!("timeout")
        };
        tokio::select! {
            res = scout => { res },
            res = timeout => { res }
        }
    }

    async fn connect_all(
        &self,
        ucast_sockets: &[UdpSocket],
        what: WhatAmIMatcher,
        addr: &SocketAddr,
    ) {
        Runtime::scout(ucast_sockets, what, addr, move |hello| async move {
            if !hello.locators.is_empty() {
                self.connect_peer(&hello.zid, &hello.locators).await
            } else {
                tracing::warn!("Received Hello with no locators: {:?}", hello);
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
        tracing::debug!("Waiting for UDP datagram...");
        loop {
            let (n, peer) = mcast_socket.recv_from(&mut buf).await.unwrap();
            if local_addrs.iter().any(|addr| *addr == peer) {
                tracing::trace!("Ignore UDP datagram from own socket");
                continue;
            }

            let mut reader = buf.as_slice()[..n].reader();
            let codec = Zenoh080::new();
            let res: Result<ScoutingMessage, DidntRead> = codec.read(&mut reader);
            if let Ok(msg) = res {
                tracing::trace!("Received {:?} from {}", msg.body, peer);
                if let ScoutingBody::Scout(Scout { what, .. }) = &msg.body {
                    if what.matches(self.whatami()) {
                        let mut wbuf = vec![];
                        let mut writer = wbuf.writer();
                        let codec = Zenoh080::new();

                        let zid = self.manager().zid();
                        let hello: ScoutingMessage = Hello {
                            version: zenoh_protocol::VERSION,
                            whatami: self.whatami(),
                            zid,
                            locators: self.get_locators(),
                        }
                        .into();
                        let socket = get_best_match(&peer.ip(), ucast_sockets).unwrap();
                        tracing::trace!(
                            "Send {:?} to {} on interface {}",
                            hello.body,
                            peer,
                            socket
                                .local_addr()
                                .map_or("unknown".to_string(), |addr| addr.ip().to_string())
                        );
                        codec.write(&mut writer, &hello).unwrap();

                        if let Err(err) = socket.send_to(wbuf.as_slice(), peer).await {
                            tracing::error!("Unable to send {:?} to {}: {}", hello.body, peer, err);
                        }
                    }
                }
            } else {
                tracing::trace!(
                    "Received unexpected UDP datagram from {}: {:?}",
                    peer,
                    &buf.as_slice()[..n]
                );
            }
        }
    }

    pub(super) fn closing_session(session: &RuntimeSession) {
        match session.runtime.whatami() {
            WhatAmI::Client => {
                let runtime = session.runtime.clone();
                let cancellation_token = runtime.get_cancellation_token();
                session.runtime.spawn(async move {
                    let retry_config = runtime.get_global_connect_retry_config();
                    let mut period = retry_config.period();
                    while runtime.start_client().await.is_err() {
                        tokio::select! {
                            _ = tokio::time::sleep(period.next_duration()) => {}
                            _ = cancellation_token.cancelled() => { break; }
                        }
                    }
                });
            }
            _ => {
                if let Some(endpoint) = &*zread!(session.endpoint) {
                    let peers = {
                        session
                            .runtime
                            .state
                            .config
                            .lock()
                            .connect()
                            .endpoints()
                            .clone()
                    };
                    if peers.contains(endpoint) {
                        let endpoint = endpoint.clone();
                        let runtime = session.runtime.clone();
                        session
                            .runtime
                            .spawn(async move { runtime.peer_connector_retry(endpoint).await });
                    }
                }
            }
        }
    }
}
