use std::sync::{Arc, OnceLock};

use zenoh_protocol::{
    core::{Locator, Priority, WhatAmI, ZenohIdProto},
    network::{NetworkMessageExt, NetworkMessageRef},
};

use crate::{
    histogram::Histogram,
    labels::{MessageLabel, NetworkMessageDroppedPayloadLabels, TransportLabels},
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
        let transport = TransportLabels {
            remote_zid: zid.map(Into::into),
            remote_whatami: whatami.map(Into::into),
            remote_group: group,
            remote_cn: cn,
        };
        let tx_no_link = DropStats::new(registry.clone(), transport.clone(), ReasonLabel::NoLink);
        Self(Arc::new(TransportStatsInner {
            registry,
            transport,
            tx_no_link,
        }))
    }

    pub(crate) fn registry(&self) -> &StatsRegistry {
        &self.0.registry
    }

    pub(crate) fn transport(&self) -> &TransportLabels {
        &self.0.transport
    }

    pub fn link_stats(&self, src: &Locator, dst: &Locator) -> LinkStats {
        let link = (src, dst).into();
        self.registry().add_link(self.transport(), &link);
        LinkStats::new(self.clone(), link)
    }

    pub fn peer_link_stats(
        &self,
        peer_zid: ZenohIdProto,
        peer_whatami: WhatAmI,
        link_stats: &LinkStats,
    ) -> LinkStats {
        assert!(self.transport().remote_group.is_some());
        let stats = Self::new(
            self.registry().clone(),
            Some(peer_zid),
            Some(peer_whatami),
            None,
            self.transport().remote_group.clone(),
        );
        LinkStats::new(stats, link_stats.link().clone())
    }

    pub fn drop_stats(&self, reason: ReasonLabel) -> DropStats {
        DropStats::new(self.0.registry.clone(), self.0.transport.clone(), reason)
    }

    pub fn tx_observe_no_link(&self, msg: NetworkMessageRef) {
        self.0.tx_no_link.observe_network_message_dropped(Tx, msg);
    }
}

#[derive(Debug)]
pub struct TransportStatsInner {
    registry: StatsRegistry,
    transport: TransportLabels,
    tx_no_link: DropStats,
}

impl Drop for TransportStatsInner {
    fn drop(&mut self) {
        self.registry.remove_transport(&self.transport)
    }
}

#[derive(Clone, Debug)]
pub struct DropStats(Arc<DropStatsInner>);

impl DropStats {
    pub(crate) fn new(
        registry: StatsRegistry,
        transport: TransportLabels,
        reason: ReasonLabel,
    ) -> Self {
        Self(Arc::new(DropStatsInner {
            registry,
            transport,
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
            .get_or_create_owned(&self.0.transport, None, &labels)
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
    transport: TransportLabels,
    reason: ReasonLabel,
    histograms: [[[OnceLock<Histogram>; MessageLabel::NUM]; Priority::NUM]; StatsDirection::NUM],
}
