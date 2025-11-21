use std::sync::{Arc, OnceLock};

use zenoh_protocol::{
    core::{Locator, Priority, WhatAmI, ZenohIdProto},
    network::{NetworkMessageExt, NetworkMessageRef},
};

use crate::{
    histogram::Histogram,
    labels::{LinkLabels, MessageLabel, NetworkMessageDroppedPayloadLabels, RemoteLabels},
    LinkStats, ReasonLabel, StatsDirection, StatsRegistry, Tx,
};

#[derive(Debug, Clone)]
pub struct TransportStats(Arc<TransportStatsInner>);

impl TransportStats {
    pub(crate) fn new(
        registry: StatsRegistry,
        zid: Option<ZenohIdProto>,
        whatami: Option<WhatAmI>,
        cn: Option<String>,
        group: Option<String>,
    ) -> Self {
        let remote = RemoteLabels {
            remote_zid: zid.map(Into::into),
            remote_whatami: whatami.map(Into::into),
            remote_group: group,
            remote_cn: cn,
        };
        let tx_no_link = DropStats::new(registry.clone(), remote.clone(), ReasonLabel::NoLink);
        Self(Arc::new(TransportStatsInner {
            registry,
            remote,
            tx_no_link,
        }))
    }

    pub(crate) fn registry(&self) -> &StatsRegistry {
        &self.0.registry
    }

    pub(crate) fn remote(&self) -> &RemoteLabels {
        &self.0.remote
    }

    pub fn link_stats(&self, src: &Locator, dst: &Locator) -> LinkStats {
        LinkStats::new(self.clone(), (src, dst).into())
    }

    pub fn peer_link_stats(
        &self,
        peer_zid: ZenohIdProto,
        peer_whatami: WhatAmI,
        link_stats: &LinkStats,
    ) -> LinkStats {
        assert!(self.remote().remote_group.is_some());
        let stats = Self::new(
            self.registry().clone(),
            Some(peer_zid),
            Some(peer_whatami),
            None,
            self.remote().remote_group.clone(),
        );
        LinkStats::new(stats, link_stats.link().clone())
    }

    pub fn drop_stats(&self, reason: ReasonLabel) -> DropStats {
        DropStats::new(self.0.registry.clone(), self.0.remote.clone(), reason)
    }

    pub fn tx_observe_no_link(&self, msg: NetworkMessageRef) {
        self.0.tx_no_link.observe_network_message_dropped(Tx, msg);
    }
}

#[derive(Debug)]
pub struct TransportStatsInner {
    registry: StatsRegistry,
    remote: RemoteLabels,
    tx_no_link: DropStats,
}

impl Drop for TransportStatsInner {
    fn drop(&mut self) {
        self.registry.remove_transport(&self.remote)
    }
}

#[derive(Clone, Debug)]
pub struct DropStats(Arc<DropStatsInner>);

impl DropStats {
    pub(crate) fn new(registry: StatsRegistry, remote: RemoteLabels, reason: ReasonLabel) -> Self {
        Self(Arc::new(DropStatsInner {
            registry,
            remote,
            reason,
            histograms: Default::default(),
        }))
    }

    fn get_or_create_owned(
        &self,
        direction: StatsDirection,
        msg: impl NetworkMessageExt,
    ) -> Histogram {
        let labels = NetworkMessageDroppedPayloadLabels {
            message: MessageLabel::from(msg.body()),
            priority: msg.priority().into(),
            protocol: None,
            reason: self.0.reason,
        };
        self.0
            .registry
            .network_message_dropped_payload(direction)
            .get_or_create_owned(&self.0.remote, &labels, &LinkLabels::default())
    }

    pub fn observe_network_message_dropped(
        &self,
        direction: StatsDirection,
        msg: impl NetworkMessageExt,
    ) {
        self.0.histograms[direction as usize][msg.priority() as usize]
            [MessageLabel::from(msg.body()) as usize]
            .get_or_init(|| self.get_or_create_owned(direction, msg.as_ref()))
            .observe(msg.payload_size().unwrap_or_default() as u64)
    }
}

#[derive(Debug)]
struct DropStatsInner {
    registry: StatsRegistry,
    remote: RemoteLabels,
    reason: ReasonLabel,
    histograms: [[[OnceLock<Histogram>; MessageLabel::NUM]; Priority::NUM]; StatsDirection::NUM],
}
