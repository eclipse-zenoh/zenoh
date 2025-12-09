use std::sync::{Arc, OnceLock};

use zenoh_protocol::{
    core::{Locator, Priority, WhatAmI, ZenohIdProto},
    network::{NetworkMessageExt, NetworkMessageRef},
};

use crate::{
    histogram::Histogram,
    keys::HistogramPerKey,
    labels::{
        MessageLabel, NetworkMessageDroppedPayloadLabels, NetworkMessagePayloadLabels,
        ProtocolLabel, SpaceLabel, TransportLabels, SHM_NUM,
    },
    LinkStats, ReasonLabel, StatsDirection, StatsKeys, StatsRegistry, Tx,
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
            network_message_payload: Default::default(),
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
        self.registry().add_link(&link);
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

    #[allow(clippy::too_many_arguments)]
    pub fn observe_network_message_payload(
        &self,
        direction: StatsDirection,
        message: MessageLabel,
        priority: Priority,
        payload_size: usize,
        space: SpaceLabel,
        keys: &StatsKeys,
        shm: bool,
    ) {
        let (histogram, histogram_per_key) = self.0.network_message_payload[direction as usize]
            [priority as usize][message as usize][shm as usize][space as usize]
            .get_or_init(|| {
                let labels = NetworkMessagePayloadLabels {
                    space,
                    message,
                    priority: priority.into(),
                    shm,
                };
                (
                    self.registry()
                        .network_message_payload(direction)
                        .get_or_create_owned(self.transport(), None, &labels),
                    self.registry()
                        .network_message_payload_per_key(direction)
                        .get_or_create_owned(self.transport(), None, &labels),
                )
            });
        histogram.observe(payload_size as u64);
        histogram_per_key.observe(keys, payload_size as u64);
    }

    pub fn tx_observe_no_link(&self, msg: NetworkMessageRef) {
        self.0.tx_no_link.observe_network_message_dropped(Tx, msg);
    }
}

#[derive(Debug)]
pub struct TransportStatsInner {
    registry: StatsRegistry,
    transport: TransportLabels,
    #[allow(clippy::type_complexity)]
    network_message_payload: [[[[[OnceLock<(Histogram, HistogramPerKey)>; SpaceLabel::NUM]; SHM_NUM];
        MessageLabel::NUM]; Priority::NUM]; StatsDirection::NUM],
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
        }))
    }

    pub(crate) fn observe_with_protocol(
        &self,
        direction: StatsDirection,
        msg: impl NetworkMessageExt,
        protocol: Option<ProtocolLabel>,
    ) {
        let labels = NetworkMessageDroppedPayloadLabels {
            message: MessageLabel::from(msg.body()),
            priority: msg.priority().into(),
            protocol,
            reason: self.0.reason,
        };
        self.0
            .registry
            .network_message_dropped_payload(direction)
            .get_or_create_owned(&self.0.transport, None, &labels)
            .observe(msg.payload_size().unwrap_or_default() as u64)
    }

    pub fn observe_network_message_dropped(
        &self,
        direction: StatsDirection,
        msg: impl NetworkMessageExt,
    ) {
        self.observe_with_protocol(direction, msg, None)
    }
}

#[derive(Debug)]
struct DropStatsInner {
    registry: StatsRegistry,
    transport: TransportLabels,
    reason: ReasonLabel,
}
