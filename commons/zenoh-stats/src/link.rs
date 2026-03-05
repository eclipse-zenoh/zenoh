use std::{
    array,
    sync::{Arc, OnceLock},
};

use prometheus_client::metrics::counter::Counter;
use zenoh_protocol::{core::Priority, network::NetworkMessageExt};

use crate::{
    labels::{
        BytesLabels, LinkLabels, MessageLabel, NetworkMessageLabels, ProtocolLabel, ReasonLabel,
        TransportMessageLabels,
    },
    DropStats, StatsDirection, TransportStats, Tx,
};

#[derive(Debug, Clone)]
pub struct LinkStats(Arc<LinkStatsInner>);

impl LinkStats {
    pub(crate) fn new(transport_stats: TransportStats, link: LinkLabels) -> Self {
        let registry = transport_stats.registry();
        let transport = transport_stats.transport();
        let protocol = link.protocol();
        let bytes = array::from_fn(|dir| {
            let labels = BytesLabels {
                protocol: protocol.clone(),
            };
            registry
                .bytes(StatsDirection::from_index(dir))
                .get_or_create_owned(transport, Some(&link), &labels)
        });
        let transport_message = array::from_fn(|dir| {
            let labels = TransportMessageLabels {
                protocol: protocol.clone(),
            };
            registry
                .transport_message(StatsDirection::from_index(dir))
                .get_or_create_owned(transport, Some(&link), &labels)
        });
        let tx_congestion = DropStats::new(
            registry.clone(),
            transport.clone(),
            ReasonLabel::Congestion,
            Some(protocol.clone()),
        );
        Self(Arc::new(LinkStatsInner {
            transport_stats,
            link,
            protocol,
            bytes,
            transport_message,
            network_message: Default::default(),
            tx_congestion,
        }))
    }

    pub(crate) fn link(&self) -> &LinkLabels {
        &self.0.link
    }

    pub fn inc_bytes(&self, direction: StatsDirection, bytes: u64) {
        self.0.bytes[direction as usize].inc_by(bytes);
    }

    pub fn inc_transport_message(&self, direction: StatsDirection, count: u64) {
        self.0.transport_message[direction as usize].inc_by(count);
    }

    pub fn inc_network_message(&self, direction: StatsDirection, msg: impl NetworkMessageExt) {
        let priority = msg.priority();
        let message = msg.body().into();
        #[cfg(feature = "shared-memory")]
        let shm = msg.is_shm();
        #[cfg(not(feature = "shared-memory"))]
        let shm = false;
        self.0.network_message[direction as usize][priority as usize][message as usize]
            [shm as usize]
            .get_or_init(|| {
                let transport = self.0.transport_stats.transport();
                let labels = NetworkMessageLabels {
                    message,
                    priority: priority.into(),
                    shm,
                    protocol: self.0.protocol.clone(),
                };
                self.0
                    .transport_stats
                    .registry()
                    .network_message(direction)
                    .get_or_create_owned(transport, Some(self.link()), &labels)
            })
            .inc();
    }

    pub fn tx_observe_congestion(&self, msg: impl NetworkMessageExt) {
        self.0
            .tx_congestion
            .observe_network_message_dropped_payload(Tx, msg)
    }
}

const SHM_NUM: usize = 2;

#[derive(Debug)]
struct LinkStatsInner {
    transport_stats: TransportStats,
    link: LinkLabels,
    protocol: ProtocolLabel,
    bytes: [Counter; StatsDirection::NUM],
    transport_message: [Counter; StatsDirection::NUM],
    #[allow(clippy::type_complexity)]
    network_message:
        [[[[OnceLock<Counter>; SHM_NUM]; MessageLabel::NUM]; Priority::NUM]; StatsDirection::NUM],
    tx_congestion: DropStats,
}

impl Drop for LinkStatsInner {
    fn drop(&mut self) {
        self.transport_stats
            .registry()
            .remove_link(self.transport_stats.transport(), &self.link);
    }
}
