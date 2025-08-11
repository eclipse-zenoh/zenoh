use std::{
    collections::HashSet,
    future::Future,
    io::ErrorKind,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use futures::{lock::Mutex, FutureExt};
use itertools::Itertools;
use tokio::{net::UdpSocket, sync::RwLock, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use zenoh_buffers::{
    reader::{DidntRead, HasReader},
    writer::HasWriter,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{
    core::{Locator, WhatAmI, WhatAmIMatcher, ZenohIdProto},
    scouting::{HelloProto, Scout, ScoutingBody, ScoutingMessage},
};
use zenoh_result::ZResult;

use super::Runtime;
use crate::net::{common::AutoConnect, runtime::orchestrator::Loop};

const RCV_BUF_SIZE: usize = u16::MAX as usize;
const SCOUT_INITIAL_PERIOD: Duration = Duration::from_millis(1_000);
const SCOUT_MAX_PERIOD: Duration = Duration::from_millis(8_000);
const SCOUT_PERIOD_INCREASE_FACTOR: u32 = 2;

#[derive(Clone)]
pub struct Scouting {
    state: Arc<ScoutState>,
}

struct ScoutState {
    listen: bool,
    autoconnect: AutoConnect,
    /// The multicast address to send scout messages to.
    addr: SocketAddr,
    /// Interface constraints, "auto" or a comma-separated IP address list..
    ifaces: String,
    multicast_ttl: u32,
    runtime: Runtime,
    sockets: RwLock<ScoutSockets>,
    cancellation_token: Mutex<CancellationToken>,
}

struct ScoutSockets {
    mcast_socket: UdpSocket,
    ucast_sockets: Vec<UdpSocket>,
}

impl Scouting {
    pub async fn new(
        listen: bool,
        autoconnect: AutoConnect,
        addr: SocketAddr,
        ifaces: String,
        multicast_ttl: u32,
        runtime: Runtime,
    ) -> ZResult<Self> {
        let ifaces_ips = Runtime::get_interfaces(&ifaces);
        let mcast_socket = Runtime::bind_mcast_port(&addr, &ifaces_ips, multicast_ttl).await?;
        let ucast_sockets = ifaces_ips
            .into_iter()
            .filter_map(|iface| Runtime::bind_ucast_port(iface, multicast_ttl).ok())
            .collect();

        let sockets = RwLock::new(ScoutSockets {
            mcast_socket,
            ucast_sockets,
        });
        let cancellation_token = Mutex::new(CancellationToken::new());

        let state = Arc::new(ScoutState {
            listen,
            autoconnect,
            addr,
            ifaces,
            multicast_ttl,
            runtime,
            sockets,
            cancellation_token,
        });

        Ok(Scouting { state })
    }

    pub async fn update_addrs_if_needed(&self) {
        let available_mcast_addrs = Runtime::get_interfaces(&self.state.ifaces)
            .into_iter()
            .collect::<HashSet<_>>();
        let used_mcast_addrs = zasyncread!(self.state.sockets)
            .ucast_sockets
            .iter()
            .filter_map(|s| s.local_addr().ok())
            .map(|s| s.ip())
            .collect::<HashSet<_>>();
        let new_addrs = available_mcast_addrs
            .difference(&used_mcast_addrs)
            .collect::<Vec<_>>();
        let obsolete_addrs = used_mcast_addrs
            .difference(&available_mcast_addrs)
            .collect::<Vec<_>>();

        if !new_addrs.is_empty() || !obsolete_addrs.is_empty() {
            if let Err(e) = self
                .update_scouting_addresses(&new_addrs, &obsolete_addrs)
                .await
            {
                tracing::error!(
                    "Could not update scouting addresses with +{:?}, -{:?}: {}",
                    new_addrs,
                    obsolete_addrs,
                    e
                );
            };
        }
    }

    async fn update_scouting_addresses(
        &self,
        addrs_to_add: &[&IpAddr],
        addrs_to_remove: &[&IpAddr],
    ) -> ZResult<()> {
        tracing::debug!("Join multicast scouting on {addrs_to_add:?}");
        for addr_to_add in addrs_to_add {
            self.join_multicast_group(addr_to_add).await;
        }

        tracing::debug!("Restarting scout routine");
        // TODO: This may interrupt something important, as a connection establishment... fix that.
        zasynclock!(self.state.cancellation_token).cancel();

        {
            let mut sockets = zasyncwrite!(self.state.sockets);
            sockets.ucast_sockets.retain(|s| {
                s.local_addr().map_or(true, |a| {
                    let ip = a.ip();
                    if addrs_to_remove.iter().copied().contains(&ip) {
                        tracing::debug!("Removing socket udp/{}", ip);
                        false
                    } else {
                        true
                    }
                })
            });
            sockets.ucast_sockets.extend(
                addrs_to_add
                    .iter()
                    .filter_map(|&i| Runtime::bind_ucast_port(*i, self.state.multicast_ttl).ok()),
            );
        }

        *zasynclock!(self.state.cancellation_token) = CancellationToken::new();

        self.start().await?;
        tracing::debug!("Scout routine restarted");

        Ok(())
    }

    async fn join_multicast_group(&self, interface_addr: &IpAddr) {
        let sockets = zasyncread!(self.state.sockets);
        if let (IpAddr::V4(new_addr), IpAddr::V4(mcast_addr)) =
            (interface_addr, self.state.addr.ip())
        {
            match sockets
                .mcast_socket
                .join_multicast_v4(mcast_addr, *new_addr)
            {
                Ok(()) => tracing::debug!(
                    "Joined multicast group {} on interface {}",
                    mcast_addr,
                    new_addr,
                ),
                // We already joined the multicast group
                Err(err) if err.kind() == ErrorKind::AddrInUse => (),
                Err(err) => tracing::warn!(
                    "Unable to join multicast group {} on interface {}: {}",
                    mcast_addr,
                    new_addr,
                    err,
                ),
            };
        }
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
        // This can be interrupted anytime: we cannot send "half" beacons,
        // and there's no repercusion if we miss one send.
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

    pub async fn start(&self) -> ZResult<()> {
        if !zasyncread!(self.state.sockets).ucast_sockets.is_empty() {
            let this = self.clone();
            let token = this.get_cancellation_token().await;
            match (self.state.listen, self.state.autoconnect.is_enabled()) {
                (true, true) => {
                    self.spawn_abortable(async move {
                        let sockets = zasyncread!(this.state.sockets);
                        tokio::select! {
                            _ = this.responder(&sockets.mcast_socket, &sockets.ucast_sockets) => {},
                            _ = this.autoconnect_all(
                                &sockets.ucast_sockets,
                                this.state.autoconnect,
                                &this.state.addr
                            ) => {},
                            _ = token.cancelled() => (),
                        }
                    });
                }
                (true, false) => {
                    self.spawn_abortable(async move {
                        let sockets = zasyncread!(this.state.sockets);
                        tokio::select! {
                            _ = this.responder(&sockets.mcast_socket, &sockets.ucast_sockets) => (),
                            _ = token.cancelled() => (),
                        }
                    });
                }
                (false, true) => {
                    self.spawn_abortable(async move {
                        let sockets = zasyncread!(this.state.sockets);
                        tokio::select! {
                            _ = this.autoconnect_all(
                                &sockets.ucast_sockets,
                                this.state.autoconnect,
                                &this.state.addr,
                            ) => (),
                            _ = token.cancelled() => (),
                        }
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn autoconnect_all(
        &self,
        ucast_sockets: &[UdpSocket],
        autoconnect: AutoConnect,
        addr: &SocketAddr,
    ) {
        Self::scout(
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

                        let zid = self.zid();
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

    fn whatami(&self) -> WhatAmI {
        self.state.runtime.whatami()
    }

    fn get_locators(&self) -> Vec<Locator> {
        self.state.runtime.get_locators()
    }

    async fn get_cancellation_token(&self) -> CancellationToken {
        zasynclock!(self.state.cancellation_token).child_token()
    }

    fn zid(&self) -> ZenohIdProto {
        self.state.runtime.manager().zid()
    }

    async fn connect_peer(&self, zid: &ZenohIdProto, locators: &[Locator]) -> bool {
        self.state.runtime.connect_peer(zid, locators).await
    }

    fn spawn_abortable<F, T>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.state.runtime.spawn_abortable(future)
    }
}

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
        .max_by(|sock1, sock2| matching_octets(addr, sock1).cmp(&matching_octets(addr, sock2)))
}
