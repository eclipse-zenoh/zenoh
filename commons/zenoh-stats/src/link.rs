use std::{
    array,
    cell::Cell,
    sync::{Arc, OnceLock},
};

use prometheus_client::metrics::counter::Counter;
use scoped_tls::scoped_thread_local;
use zenoh_link_commons::LinkId;
use zenoh_protocol::{core::Priority, network::NetworkMessageExt};

use crate::{
    histogram::Histogram,
    labels::{
        BytesLabels, MessageLabel, NetworkMessageLabels, NetworkMessagePayloadLabels, ReasonLabel,
        SpaceLabel, TransportMessageLabels,
    },
    DropStats, Rx, StatsDirection, TransportStats, Tx,
};

scoped_thread_local! {
    static RX_LINK: LinkStats
}
thread_local! {
    pub(self) static RX_LINK_LEVEL_INFO: Cell<Option<LinkLevelInfo>> = const { Cell::new(None) };
}
thread_local! {
    pub(self) static TX_ROUTER_LEVEL_INFO: Cell<Option<RouterLevelInfo>> = const { Cell::new(None) };
}

pub fn with_tx_observe_network_message<R>(
    is_admin: bool,
    payload_size: usize,
    f: impl FnOnce() -> R,
) -> R {
    let r_info = RouterLevelInfo::new(is_admin, payload_size);
    TX_ROUTER_LEVEL_INFO.set(Some(r_info));
    let res = f();
    TX_ROUTER_LEVEL_INFO.set(None);
    res
}

pub fn rx_observe_network_message_finalize(is_admin: bool, payload_size: usize) {
    if let Some(l_info) = RX_LINK_LEVEL_INFO.get() {
        let r_info = RouterLevelInfo::new(is_admin, payload_size);
        RX_LINK.with(|stats| {
            stats.observe_network_message_payload(Rx, l_info, r_info);
        });
    }
}

#[derive(Debug, Clone)]
pub struct LinkStats(Arc<LinkStatsInner>);

impl LinkStats {
    pub(crate) fn new(transport_stats: TransportStats, link_id: LinkId, protocol: String) -> Self {
        let registry = transport_stats.registry();
        let remote = transport_stats.remote();
        let bytes = array::from_fn(|dir| {
            let labels = BytesLabels {
                protocol: protocol.clone(),
            };
            registry
                .bytes(StatsDirection::from_index(dir))
                .get_or_create_owned(remote, &labels, Some(link_id))
        });
        let transport_message = array::from_fn(|dir| {
            let labels = TransportMessageLabels {
                protocol: protocol.clone(),
            };
            registry
                .transport_message(StatsDirection::from_index(dir))
                .get_or_create_owned(remote, &labels, Some(link_id))
        });
        let tx_congestion =
            DropStats::new(registry.clone(), remote.clone(), ReasonLabel::Congestion);
        Self(Arc::new(LinkStatsInner {
            transport_stats,
            link_id,
            protocol,
            bytes,
            transport_message,
            network_message: Default::default(),
            network_message_payload: Default::default(),
            tx_congestion,
        }))
    }

    pub(crate) fn id(&self) -> LinkId {
        self.0.link_id
    }

    pub(crate) fn protocol(&self) -> &str {
        &self.0.protocol
    }

    pub fn inc_bytes(&self, direction: StatsDirection, bytes: u64) {
        self.0.bytes[direction as usize].inc_by(bytes);
    }

    pub fn inc_transport_message(&self, direction: StatsDirection, count: u64) {
        self.0.transport_message[direction as usize].inc_by(count);
    }

    fn inc_network_message(&self, direction: StatsDirection, msg: LinkLevelInfo) {
        self.0.network_message[Tx as usize][msg.priority as usize][msg.message as usize]
            [msg.shm as usize]
            .get_or_init(|| {
                let remote = self.0.transport_stats.remote();
                let labels = NetworkMessageLabels {
                    message: msg.message,
                    priority: msg.priority.into(),
                    shm: msg.shm,
                    protocol: self.0.protocol.clone(),
                };
                self.0
                    .transport_stats
                    .registry()
                    .network_message(direction)
                    .get_or_create_owned(remote, &labels, Some(self.0.link_id))
            })
            .inc();
    }

    fn observe_network_message_payload(
        &self,
        direction: StatsDirection,
        l_info: LinkLevelInfo,
        r_info: RouterLevelInfo,
    ) {
        self.0.network_message_payload[Tx as usize][l_info.priority as usize]
            [l_info.message as usize][l_info.shm as usize][r_info.space as usize]
            .get_or_init(|| {
                let remote = self.0.transport_stats.remote();
                let labels = NetworkMessagePayloadLabels {
                    space: r_info.space,
                    message: l_info.message,
                    priority: l_info.priority.into(),
                    shm: l_info.shm,
                    protocol: self.0.protocol.clone(),
                };
                self.0
                    .transport_stats
                    .registry()
                    .network_message_payload(direction)
                    .get_or_create_owned(remote, &labels, Some(self.0.link_id))
            })
            .observe(r_info.payload_size as u64);
    }

    pub fn tx_observe_network_message_finalize(&self, msg: impl NetworkMessageExt) {
        let l_info = LinkLevelInfo::new(&msg);
        self.inc_network_message(Tx, l_info);
        if let Some(r_info) = TX_ROUTER_LEVEL_INFO.get() {
            self.observe_network_message_payload(Tx, l_info, r_info);
        }
    }

    pub fn with_rx_observe_network_message<M: NetworkMessageExt, R>(
        &self,
        msg: M,
        f: impl FnOnce(M) -> R,
    ) -> R {
        let l_info = LinkLevelInfo::new(&msg);
        self.inc_network_message(Tx, l_info);
        RX_LINK_LEVEL_INFO.set(Some(l_info));
        let res = RX_LINK.set(self, || f(msg));
        RX_LINK_LEVEL_INFO.set(None);
        res
    }

    pub fn tx_observe_congestion(&self, msg: impl NetworkMessageExt) {
        self.0
            .tx_congestion
            .observe_network_message_dropped(Tx, msg);
    }
}

const SHM_NUM: usize = 2;

#[derive(Debug)]
struct LinkStatsInner {
    transport_stats: TransportStats,
    link_id: LinkId,
    protocol: String,
    bytes: [Counter; StatsDirection::NUM],
    transport_message: [Counter; StatsDirection::NUM],
    #[allow(clippy::type_complexity)]
    network_message:
        [[[[OnceLock<Counter>; SHM_NUM]; MessageLabel::NUM]; Priority::NUM]; StatsDirection::NUM],
    #[allow(clippy::type_complexity)]
    network_message_payload: [[[[[OnceLock<Histogram>; SpaceLabel::NUM]; SHM_NUM];
        MessageLabel::NUM]; Priority::NUM]; StatsDirection::NUM],
    tx_congestion: DropStats,
}

impl Drop for LinkStatsInner {
    fn drop(&mut self) {
        self.transport_stats
            .registry()
            .remove_link(self.transport_stats.remote(), self.link_id);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LinkLevelInfo {
    priority: Priority,
    message: MessageLabel,
    shm: bool,
}

impl LinkLevelInfo {
    fn new(msg: impl NetworkMessageExt) -> Self {
        Self {
            priority: msg.priority(),
            message: msg.body().into(),
            #[cfg(not(feature = "shared-memory"))]
            shm: false,
            #[cfg(feature = "shared-memory")]
            shm: msg.is_shm(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RouterLevelInfo {
    space: SpaceLabel,
    payload_size: usize,
}

impl RouterLevelInfo {
    fn new(is_admin: bool, payload_size: usize) -> Self {
        Self {
            space: if is_admin {
                SpaceLabel::Admin
            } else {
                SpaceLabel::User
            },
            payload_size,
        }
    }
}
