use std::{
    array,
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
scoped_thread_local! {
    static RX_MESSAGE: MessageInfo
}
scoped_thread_local! {
    static TX_SPACE: SpaceLabel
}

pub fn tx_with_space<R>(is_admin: bool, f: impl FnOnce() -> R) -> R {
    let space = if is_admin {
        SpaceLabel::Admin
    } else {
        SpaceLabel::User
    };
    TX_SPACE.set(&space, f)
}

pub fn rx_set_space(is_admin: bool) {
    let space = if is_admin {
        SpaceLabel::Admin
    } else {
        SpaceLabel::User
    };
    if RX_LINK.is_set() {
        RX_LINK.with(|link| {
            RX_MESSAGE.with(|msg| link.observe_network_message_payload(Rx, msg, space))
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

    fn inc_network_message(&self, direction: StatsDirection, msg: &MessageInfo) {
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
        msg: &MessageInfo,
        space: SpaceLabel,
    ) {
        if let Some(payload_size) = msg.payload_size {
            self.0.network_message_payload[Tx as usize][msg.priority as usize]
                [msg.message as usize][msg.shm as usize][space as usize]
                .get_or_init(|| {
                    let remote = self.0.transport_stats.remote();
                    let labels = NetworkMessagePayloadLabels {
                        space,
                        message: msg.message,
                        priority: msg.priority.into(),
                        shm: msg.shm,
                        protocol: self.0.protocol.clone(),
                    };
                    self.0
                        .transport_stats
                        .registry()
                        .network_message_payload(direction)
                        .get_or_create_owned(remote, &labels, Some(self.0.link_id))
                })
                .observe(payload_size);
        }
    }

    pub fn tx_observe_network_message(&self, msg: impl NetworkMessageExt) {
        let info = msg.into();
        self.inc_network_message(Tx, &info);
        if TX_SPACE.is_set() {
            TX_SPACE.with(|space| self.observe_network_message_payload(Tx, &info, *space))
        }
    }

    pub fn rx_observe_network_message<M: NetworkMessageExt, R>(
        &self,
        msg: M,
        f: impl Fn(M) -> R,
    ) -> R {
        let info = msg.as_ref().into();
        self.inc_network_message(Tx, &info);
        if info.payload_size.is_some() {
            RX_LINK.set(self, || RX_MESSAGE.set(&info, || f(msg)))
        } else {
            self.inc_network_message(Rx, &info);
            f(msg)
        }
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

#[derive(Debug)]
pub(crate) struct MessageInfo {
    priority: Priority,
    message: MessageLabel,
    shm: bool,
    payload_size: Option<u64>,
}

impl<M: NetworkMessageExt> From<M> for MessageInfo {
    fn from(value: M) -> Self {
        Self {
            priority: value.priority(),
            message: value.body().into(),
            #[cfg(not(feature = "shared-memory"))]
            shm: false,
            #[cfg(feature = "shared-memory")]
            shm: value.is_shm(),
            payload_size: value.payload_size().map(|s| s as u64),
        }
    }
}
