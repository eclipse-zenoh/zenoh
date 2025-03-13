use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;
use zenoh_buffers::{
    reader::{DidntRead, HasReader},
    writer::HasWriter,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::core::{Locator, WhatAmI, ZenohIdProto};
use zenoh_protocol::scouting::{HelloProto, Scout, ScoutingBody, ScoutingMessage};
use zenoh_result::ZResult;

use super::Runtime;
use crate::net::common::AutoConnect;
use crate::net::runtime::orchestrator::Loop;

const RCV_BUF_SIZE: usize = u16::MAX as usize;

#[derive(Clone)]
pub struct Scouting {
    state: Arc<ScoutState>,
}

struct ScoutState {
    listen: bool,
    autoconnect: AutoConnect,
    addr: SocketAddr,
    mcast_socket: UdpSocket,
    ucast_sockets: Vec<UdpSocket>,
    runtime: Runtime,
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
        let ifaces = Runtime::get_interfaces(&ifaces);
        let mcast_socket = Runtime::bind_mcast_port(&addr, &ifaces, multicast_ttl).await?;
        let ucast_sockets = ifaces
            .into_iter()
            .filter_map(|iface| Runtime::bind_ucast_port(iface, multicast_ttl).ok())
            .collect();

        let state = Arc::new(ScoutState {
            listen,
            autoconnect,
            addr,
            mcast_socket,
            ucast_sockets,
            runtime,
        });

        Ok(Scouting { state })
    }

    pub async fn start(&self) -> ZResult<()> {
        if !self.state.ucast_sockets.is_empty() {
            let this = self.clone();
            match (self.state.listen, self.state.autoconnect.is_enabled()) {
                (true, true) => {
                    self.spawn_abortable(async move {
                        tokio::select! {
                            _ = this.responder(&this.state.mcast_socket, &this.state.ucast_sockets) => {},
                            _ = this.autoconnect_all(
                                &this.state.ucast_sockets,
                                this.state.autoconnect,
                                &this.state.addr
                            ) => {},
                        }
                    });
                }
                (true, false) => {
                    self.spawn_abortable(async move {
                        this.responder(&this.state.mcast_socket, &this.state.ucast_sockets)
                            .await;
                    });
                }
                (false, true) => {
                    self.spawn_abortable(async move {
                        this.autoconnect_all(
                            &this.state.ucast_sockets,
                            this.state.autoconnect,
                            &this.state.addr,
                        )
                        .await
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
