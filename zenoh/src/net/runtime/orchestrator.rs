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
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv6Addr, SocketAddr},
    ops::DerefMut,
    str::FromStr,
    time::Duration,
};

use futures::{prelude::*, stream::FuturesUnordered};
use socket2::{Domain, Socket, Type};
use tokio::{
    net::UdpSocket,
    sync::{futures::Notified, Mutex, Notify},
};
use tokio_util::sync::CancellationToken;
use zenoh_buffers::{
    reader::{DidntRead, HasReader},
    writer::HasWriter,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_config::{
    get_global_connect_timeout, get_global_listener_timeout, unwrap_or_default,
    ConnectionRetryPeriod, ModeDependent,
};
use zenoh_link::{Locator, LocatorInspector};
use zenoh_protocol::{
    core::{whatami::WhatAmIMatcher, EndPoint, Metadata, PriorityRange, WhatAmI, ZenohIdProto},
    scouting::{HelloProto, Scout, ScoutingBody, ScoutingMessage},
};
use zenoh_result::{bail, zerror, ZResult};

use super::{Runtime, RuntimeSession};
use crate::net::{common::AutoConnect, protocol::linkstate::LinkInfo};

const RCV_BUF_SIZE: usize = u16::MAX as usize;
const SCOUT_INITIAL_PERIOD: Duration = Duration::from_millis(1_000);
const SCOUT_MAX_PERIOD: Duration = Duration::from_millis(8_000);
const SCOUT_PERIOD_INCREASE_FACTOR: u32 = 2;

pub enum Loop {
    Continue,
    Break,
}

#[derive(Default, Debug)]
pub(crate) struct PeerConnector {
    zid: Option<ZenohIdProto>,
    terminated: bool,
}

#[derive(Default, Debug)]
pub(crate) struct StartConditions {
    notify: Notify,
    peer_connectors: Mutex<Vec<PeerConnector>>,
}

impl StartConditions {
    pub(crate) fn notified(&self) -> Notified<'_> {
        self.notify.notified()
    }

    pub(crate) async fn add_peer_connector(&self) -> usize {
        let mut peer_connectors = self.peer_connectors.lock().await;
        peer_connectors.push(PeerConnector::default());
        peer_connectors.len() - 1
    }

    pub(crate) async fn add_peer_connector_zid(&self, zid: ZenohIdProto) {
        let mut peer_connectors = self.peer_connectors.lock().await;
        if !peer_connectors.iter().any(|pc| pc.zid == Some(zid)) {
            peer_connectors.push(PeerConnector {
                zid: Some(zid),
                terminated: false,
            })
        }
    }

    pub(crate) async fn set_peer_connector_zid(&self, idx: usize, zid: ZenohIdProto) {
        let mut peer_connectors = self.peer_connectors.lock().await;
        if let Some(peer_connector) = peer_connectors.get_mut(idx) {
            peer_connector.zid = Some(zid);
        }
    }

    pub(crate) async fn terminate_peer_connector(&self, idx: usize) {
        let mut peer_connectors = self.peer_connectors.lock().await;
        if let Some(peer_connector) = peer_connectors.get_mut(idx) {
            peer_connector.terminated = true;
        }
        if !peer_connectors.iter().any(|pc| !pc.terminated) {
            self.notify.notify_one()
        }
    }

    pub(crate) async fn terminate_peer_connector_zid(&self, zid: ZenohIdProto) {
        let mut peer_connectors = self.peer_connectors.lock().await;
        if let Some(peer_connector) = peer_connectors.iter_mut().find(|pc| pc.zid == Some(zid)) {
            peer_connector.terminated = true;
        } else {
            peer_connectors.push(PeerConnector {
                zid: Some(zid),
                terminated: true,
            })
        }
        if !peer_connectors.iter().any(|pc| !pc.terminated) {
            self.notify.notify_one()
        }
    }
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
        let (peers, scouting, autoconnect, addr, ifaces, timeout, multicast_ttl) = {
            let guard = &self.state.config.lock().0;
            (
                guard
                    .connect()
                    .endpoints()
                    .client()
                    .unwrap_or(&vec![])
                    .clone(),
                unwrap_or_default!(guard.scouting().multicast().enabled()),
                *unwrap_or_default!(guard.scouting().multicast().autoconnect().client()),
                unwrap_or_default!(guard.scouting().multicast().address()),
                unwrap_or_default!(guard.scouting().multicast().interface()),
                std::time::Duration::from_millis(unwrap_or_default!(guard.scouting().timeout())),
                unwrap_or_default!(guard.scouting().multicast().ttl()),
            )
        };
        match peers.len() {
            0 => {
                if scouting {
                    tracing::info!("Scouting...");
                    let ifaces = Runtime::get_interfaces(&ifaces);
                    if ifaces.is_empty() {
                        bail!("Unable to find multicast interface!")
                    } else {
                        let sockets: Vec<UdpSocket> = ifaces
                            .into_iter()
                            .filter_map(|iface| Runtime::bind_ucast_port(iface, multicast_ttl).ok())
                            .collect();
                        if sockets.is_empty() {
                            bail!("Unable to bind UDP port to any multicast interface!")
                        } else {
                            self.connect_first(&sockets, autoconnect, &addr, timeout)
                                .await
                        }
                    }
                } else {
                    bail!("No peer specified and multicast scouting deactivated!")
                }
            }
            _ => self.connect_peers(&peers, true).await,
        }
    }

    async fn start_peer(&self) -> ZResult<()> {
        let (
            listeners,
            peers,
            scouting,
            wait_scouting,
            listen,
            autoconnect,
            addr,
            ifaces,
            delay,
            linkstate,
        ) = {
            let guard = &self.state.config.lock().0;
            (
                guard.listen().endpoints().peer().unwrap_or(&vec![]).clone(),
                guard
                    .connect()
                    .endpoints()
                    .peer()
                    .unwrap_or(&vec![])
                    .clone(),
                unwrap_or_default!(guard.scouting().multicast().enabled()),
                unwrap_or_default!(guard.open().return_conditions().connect_scouted()),
                *unwrap_or_default!(guard.scouting().multicast().listen().peer()),
                AutoConnect::multicast(guard, WhatAmI::Peer, self.zid().into()),
                unwrap_or_default!(guard.scouting().multicast().address()),
                unwrap_or_default!(guard.scouting().multicast().interface()),
                Duration::from_millis(unwrap_or_default!(guard.scouting().delay())),
                unwrap_or_default!(guard.routing().peer().mode()) == *"linkstate",
            )
        };

        self.bind_listeners(&listeners).await?;

        self.connect_peers(&peers, false).await?;

        if scouting {
            self.start_scout(listen, autoconnect, addr, ifaces).await?;
        }

        if linkstate {
            tokio::time::sleep(delay).await;
        } else if wait_scouting
            && (scouting || !peers.is_empty())
            && tokio::time::timeout(delay, self.state.start_conditions.notified())
                .await
                .is_err()
            && !peers.is_empty()
        {
            tracing::warn!("Scouting delay elapsed before start conditions are met.");
        }
        Ok(())
    }

    async fn start_router(&self) -> ZResult<()> {
        let (listeners, peers, scouting, listen, autoconnect, addr, ifaces, delay) = {
            let guard = &self.state.config.lock().0;
            (
                guard
                    .listen()
                    .endpoints()
                    .router()
                    .unwrap_or(&vec![])
                    .clone(),
                guard
                    .connect()
                    .endpoints()
                    .router()
                    .unwrap_or(&vec![])
                    .clone(),
                unwrap_or_default!(guard.scouting().multicast().enabled()),
                *unwrap_or_default!(guard.scouting().multicast().listen().router()),
                AutoConnect::multicast(guard, WhatAmI::Router, self.zid().into()),
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

    async fn start_scout(
        &self,
        listen: bool,
        autoconnect: AutoConnect,
        addr: SocketAddr,
        ifaces: String,
    ) -> ZResult<()> {
        let multicast_ttl = {
            let config_guard = self.config().lock();
            let config = &config_guard.0;
            unwrap_or_default!(config.scouting().multicast().ttl())
        };
        let ifaces = Runtime::get_interfaces(&ifaces);
        let mcast_socket = Runtime::bind_mcast_port(&addr, &ifaces, multicast_ttl).await?;
        if !ifaces.is_empty() {
            let sockets: Vec<UdpSocket> = ifaces
                .into_iter()
                .filter_map(|iface| Runtime::bind_ucast_port(iface, multicast_ttl).ok())
                .collect();
            if !sockets.is_empty() {
                let this = self.clone();
                match (listen, autoconnect.is_enabled()) {
                    (true, true) => {
                        self.spawn_abortable(async move {
                            tokio::select! {
                                _ = this.responder(&mcast_socket, &sockets) => {},
                                _ = this.autoconnect_all(
                                    &sockets,
                                    autoconnect,
                                    &addr
                                ) => {},
                            }
                        });
                    }
                    (true, false) => {
                        self.spawn_abortable(async move {
                            this.responder(&mcast_socket, &sockets).await;
                        });
                    }
                    (false, true) => {
                        self.spawn_abortable(async move {
                            this.autoconnect_all(&sockets, autoconnect, &addr).await
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
                self.connect_peers_impl(peers, single_link).await
            })
            .await;
            match res {
                Ok(r) => r,
                Err(_) => {
                    let e = zerror!("Unable to connect to any of {:?}. Timeout!", peers);
                    tracing::warn!("{}", &e);
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
        let mut peers_to_retry = Vec::new();
        for peer in peers {
            let endpoint = peer.clone();
            let retry_config = self.get_connect_retry_config(&endpoint);
            if retry_config.timeout().is_zero() || self.get_global_connect_timeout().is_zero() {
                tracing::debug!(
                    "Try to connect: {:?}: global timeout: {:?}, retry: {:?}",
                    endpoint,
                    self.get_global_connect_timeout(),
                    retry_config
                );
                // try to connect and exit immediately without retry
                if self.peer_connector(endpoint).await.is_ok() {
                    return Ok(());
                }
            } else {
                peers_to_retry.push(endpoint);
            }
        }
        // sequentially try to connect to one of the remaining peers
        // respecting connection retry delays
        match self.peers_connector_retry(peers_to_retry, true).await {
            Ok(_) => Ok(()),
            Err(_) => {
                let e = zerror!("Unable to connect to any of {:?}! ", peers);
                tracing::warn!("{}", &e);
                Err(e.into())
            }
        }
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
                if let Err(e) = self.peer_connector(endpoint).await {
                    if retry_config.exit_on_failure {
                        return Err(e);
                    }
                }
            } else if retry_config.exit_on_failure {
                // try to connect with retry waiting
                let _ = self.peer_connector_retry(endpoint).await;
            } else {
                // try to connect in background
                if let Err(e) = self.spawn_peer_connector(endpoint.clone()).await {
                    tracing::warn!("Error connecting to {}: {}", endpoint, e);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    async fn peer_connector(&self, peer: EndPoint) -> ZResult<()> {
        match self.manager().open_transport_unicast(peer.clone()).await {
            Ok(transport) => {
                if let Ok(Some(orch_transport)) = transport.get_callback() {
                    if let Some(orch_transport) = orch_transport
                        .as_any()
                        .downcast_ref::<super::RuntimeSession>()
                    {
                        zwrite!(orch_transport.endpoints).insert(peer);
                    }
                }
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Unable to connect to {}! {}", peer, e);
                Err(e)
            }
        }
    }

    fn get_listen_retry_config(&self, endpoint: &EndPoint) -> zenoh_config::ConnectionRetryConf {
        let guard = &self.state.config.lock().0;
        zenoh_config::get_retry_config(guard, Some(endpoint), true)
    }

    fn get_connect_retry_config(&self, endpoint: &EndPoint) -> zenoh_config::ConnectionRetryConf {
        let guard = &self.state.config.lock().0;
        zenoh_config::get_retry_config(guard, Some(endpoint), false)
    }

    fn get_global_listener_timeout(&self) -> std::time::Duration {
        let guard = &self.state.config.lock().0;
        get_global_listener_timeout(guard)
    }

    fn get_global_connect_timeout(&self) -> std::time::Duration {
        let guard = &self.state.config.lock().0;
        get_global_connect_timeout(guard)
    }

    async fn bind_listeners(&self, listeners: &[EndPoint]) -> ZResult<()> {
        if listeners.is_empty() {
            tracing::warn!("Starting with no listener endpoints!");
            return Ok(());
        }
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

    pub async fn bind_mcast_port(
        sockaddr: &SocketAddr,
        ifaces: &[IpAddr],
        multicast_ttl: u32,
    ) -> ZResult<UdpSocket> {
        let socket = match Socket::new(Domain::for_address(*sockaddr), Type::DGRAM, None) {
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
        socket.set_multicast_ttl_v4(multicast_ttl)?;

        if sockaddr.is_ipv6() && multicast_ttl > 1 {
            tracing::warn!("UDP Multicast TTL has been set to a value greater than 1 on a socket bound to an IPv6 address. This might not have the desired effect");
        }

        // UdpSocket::from_std requires a runtime even though it's a sync function
        let udp_socket = zenoh_runtime::ZRuntime::Net
            .block_in_place(async { UdpSocket::from_std(socket.into()) })?;
        Ok(udp_socket)
    }

    pub fn bind_ucast_port(addr: IpAddr, multicast_ttl: u32) -> ZResult<UdpSocket> {
        let sockaddr = || SocketAddr::new(addr, 0);
        let socket = match Socket::new(Domain::for_address(sockaddr()), Type::DGRAM, None) {
            Ok(socket) => socket,
            Err(err) => {
                tracing::warn!("Unable to create datagram socket: {}", err);
                bail!(err=> "Unable to create datagram socket");
            }
        };
        match socket.bind(&sockaddr().into()) {
            Ok(()) => {
                #[allow(clippy::or_fun_call)]
                let local_addr = socket
                    .local_addr()
                    .unwrap_or(sockaddr().into())
                    .as_socket()
                    .unwrap_or(sockaddr());
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
        socket.set_multicast_ttl_v4(multicast_ttl)?;

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
            let idx = self.state.start_conditions.add_peer_connector().await;
            let config_guard = this.config().lock();
            let config = &config_guard.0;
            let gossip = unwrap_or_default!(config.scouting().gossip().enabled());
            let wait_declares = unwrap_or_default!(config.open().return_conditions().declares());
            drop(config_guard);
            self.spawn(async move {
                if let Ok(zid) = this.peer_connector_retry(peer).await {
                    this.state
                        .start_conditions
                        .set_peer_connector_zid(idx, zid)
                        .await;
                }
                if !gossip && (!wait_declares || this.whatami() != WhatAmI::Peer) {
                    this.state
                        .start_conditions
                        .terminate_peer_connector(idx)
                        .await;
                }
            });
            Ok(())
        } else {
            bail!("Forbidden multicast endpoint in connect list!")
        }
    }

    async fn peers_connector_retry(
        &self,
        peers: Vec<EndPoint>,
        stop_after_first_connection: bool,
    ) -> ZResult<Vec<ZenohIdProto>> {
        async fn wait_next_peer_retry(
            peer: EndPoint,
            period: ConnectionRetryPeriod,
            wait_time: Duration,
            cancellation_token: CancellationToken,
        ) -> Option<(EndPoint, ConnectionRetryPeriod)> {
            tokio::select! {
                _ = tokio::time::sleep(wait_time) => {
                    Some((peer, period))
                }
                _ = cancellation_token.cancelled() => {
                    None
                }
            }
        }

        let mut connected_peers = Vec::new();

        let mut tasks = FuturesUnordered::new();
        let cancellation_token = self.get_cancellation_token();

        for peer in peers {
            let retry_config = self.get_connect_retry_config(&peer);
            let period = retry_config.period();
            tasks.push(wait_next_peer_retry(
                peer,
                period,
                Duration::ZERO,
                cancellation_token.clone(),
            ));
        }

        while let Some(task) = tasks.next().await {
            if let Some((peer, mut period)) = task {
                tracing::debug!(
                    "Try to connect: {:?}: global timeout: {:?}, retry: {:?}",
                    peer,
                    self.get_global_connect_timeout(),
                    self.get_connect_retry_config(&peer)
                );
                match self.manager().open_transport_unicast(peer.clone()).await {
                    Ok(transport) => {
                        tracing::debug!("Successfully connected to configured peer {}", peer);
                        if let Ok(Some(orch_transport)) = transport.get_callback() {
                            if let Some(orch_transport) = orch_transport
                                .as_any()
                                .downcast_ref::<super::RuntimeSession>()
                            {
                                zwrite!(orch_transport.endpoints).insert(peer);
                            }
                        }
                        if let Ok(zid) = transport.get_zid() {
                            connected_peers.push(zid);
                        }
                        if stop_after_first_connection {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::debug!(
                            "Unable to connect to configured peer {}! {}. Retry in {:?}.",
                            peer,
                            e,
                            period.duration()
                        );
                        let wait_time = period.next_duration();
                        tasks.push(wait_next_peer_retry(
                            peer,
                            period,
                            wait_time,
                            cancellation_token.clone(),
                        ));
                    }
                }
            }
        }
        if connected_peers.is_empty() {
            bail!("Peer connector terminated without connecting to any endpoint")
        } else {
            Ok(connected_peers)
        }
    }

    async fn peer_connector_retry(&self, peer: EndPoint) -> ZResult<ZenohIdProto> {
        self.peers_connector_retry(vec![peer], true)
            .await
            .map(|peers| peers[0])
    }

    pub async fn scout<Fut, F>(
        sockets: &[UdpSocket],
        matcher: WhatAmIMatcher,
        mcast_addr: &SocketAddr,
        f: F,
    ) where
        F: Fn(HelloProto) -> Fut + std::marker::Send + std::marker::Sync + Clone,
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

    /// Returns `true` if a new Transport instance is established with `zid` or had already been established.
    #[must_use]
    async fn connect(&self, zid: &ZenohIdProto, scouted_locators: &[Locator]) -> bool {
        if !self.insert_pending_connection(*zid).await {
            tracing::debug!("Already connecting to {}. Ignore.", zid);
            return false;
        }

        const ERR: &str = "Unable to connect to newly scouted peer";

        let configured_locators = self
            .state
            .config
            .lock()
            .0
            .connect()
            .endpoints()
            .get(self.whatami())
            .iter()
            .flat_map(|e| e.iter().map(EndPoint::to_locator))
            .collect::<HashSet<_>>();

        let locators = scouted_locators
            .iter()
            .filter(|l| !configured_locators.contains(l))
            .collect::<Vec<&Locator>>();

        if locators.is_empty() {
            tracing::debug!(
                "Already connecting to locators of {} (connect configuration). Ignore.",
                zid
            );
            return false;
        }

        let manager = self.manager();

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
            let priorities = locator
                .metadata()
                .get(Metadata::PRIORITIES)
                .and_then(|p| PriorityRange::from_str(p).ok());
            let reliability = inspector.is_reliable(locator).ok();
            if !manager
                .get_transport_unicast(zid)
                .await
                .as_ref()
                .is_some_and(|t| {
                    t.get_links().is_ok_and(|ls| {
                        ls.iter().any(|l| {
                            l.priorities == priorities
                                && inspector.is_reliable(&l.dst).ok() == reliability
                        })
                    })
                })
            {
                if is_multicast {
                    match manager.open_transport_multicast(endpoint).await {
                        Ok(transport) => {
                            tracing::debug!(
                                "Successfully connected to newly scouted peer: {:?}",
                                transport
                            );
                        }
                        Err(e) => tracing::trace!("{} {} on {}: {}", ERR, zid, locator, e),
                    }
                } else {
                    match manager.open_transport_unicast(endpoint).await {
                        Ok(transport) => {
                            tracing::debug!(
                                "Successfully connected to newly scouted peer: {:?}",
                                transport
                            );
                        }
                        Err(e) => tracing::trace!("{} {} on {}: {}", ERR, zid, locator, e),
                    }
                }
            } else {
                tracing::trace!(
                    "Will not attempt to connect to {} via {}: already connected to this peer for this PriorityRange-Reliability pair",
                    zid, locator
                );
            }
        }

        self.remove_pending_connection(zid).await;

        if self.manager().get_transport_unicast(zid).await.is_none() {
            tracing::warn!(
                "Unable to connect to any locator of scouted peer {}: {:?}",
                zid,
                scouted_locators
            );
            false
        } else {
            true
        }
    }

    /// Returns `true` if a new Transport instance is established with `zid` or had already been established.
    pub async fn connect_peer(&self, zid: &ZenohIdProto, locators: &[Locator]) -> bool {
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
                self.connect(zid, locators).await
            } else {
                tracing::trace!("Already connected scouted peer: {}", zid);
                true
            }
        } else {
            true
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

    async fn autoconnect_all(
        &self,
        ucast_sockets: &[UdpSocket],
        autoconnect: AutoConnect,
        addr: &SocketAddr,
    ) {
        Runtime::scout(
            ucast_sockets,
            autoconnect.matcher(),
            addr,
            move |hello| async move {
                if hello.locators.is_empty() {
                    tracing::warn!("Received Hello with no locators: {:?}", hello);
                } else if autoconnect.should_autoconnect(hello.zid, hello.whatami) {
                    self.connect_peer(&hello.zid, &hello.locators).await;
                }
                Loop::Continue
            },
        )
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
            if local_addrs.contains(&peer) {
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
                        let hello: ScoutingMessage = HelloProto {
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

    pub(super) fn closed_session(session: &RuntimeSession) {
        if session.runtime.is_closed() {
            return;
        }

        if zread!(session.endpoints).is_empty() {
            return;
        }
        let mut peers = session
            .runtime
            .state
            .config
            .lock()
            .0
            .connect()
            .endpoints()
            .get(session.runtime.state.whatami)
            .unwrap_or(&vec![])
            .clone();

        if session.runtime.whatami() != WhatAmI::Client {
            let endpoints = std::mem::take(zwrite!(session.endpoints).deref_mut());
            peers.retain(|p| endpoints.contains(p));
        }

        if !peers.is_empty() {
            let runtime = session.runtime.clone();
            session.runtime.spawn(async move {
                runtime
                    .peers_connector_retry(peers, runtime.whatami() == WhatAmI::Client)
                    .await
            });
        }
    }

    pub(super) fn closed_link(session: &RuntimeSession, endpoint: EndPoint) {
        if session.runtime.whatami() == WhatAmI::Client {
            // Currently Client can have only one link,
            // so we process reconnect in closed_session
            return;
        }
        if session.runtime.is_closed() {
            return;
        }
        let peers = session
            .runtime
            .state
            .config
            .lock()
            .0
            .connect()
            .endpoints()
            .get(session.runtime.state.whatami)
            .unwrap_or(&vec![])
            .clone();

        if peers.contains(&endpoint) && zwrite!(session.endpoints).remove(&endpoint) {
            let runtime = session.runtime.clone();
            session.runtime.spawn(async move {
                let _ = runtime.peer_connector_retry(endpoint).await;
            });
        }
    }

    #[allow(dead_code)]
    pub(crate) fn update_network(&self) -> ZResult<()> {
        let router = self.router();
        let _ctrl_lock = zlock!(router.tables.ctrl_lock);
        let mut tables = zwrite!(router.tables.tables);
        router
            .tables
            .hat_code
            .update_from_config(&mut tables, &router.tables, self)
    }

    pub(crate) fn get_links_info(&self) -> HashMap<ZenohIdProto, LinkInfo> {
        let router = self.router();
        let tables = zread!(router.tables.tables);
        router.tables.hat_code.links_info(&tables)
    }
}
