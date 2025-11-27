use std::{
    array, fmt,
    fmt::Write,
    hash::Hash,
    iter,
    sync::{Arc, RwLock},
};

use prometheus_client::{
    encoding::text::encode,
    metrics::{
        counter::Counter,
        family::{Family, MetricConstructor},
        gauge::Gauge,
        info::Info,
    },
    registry::{Registry, Unit},
};
use zenoh_protocol::core::{WhatAmI, ZenohIdProto};

use crate::{
    family::{
        TransportFamily, TransportFamilyCollector, TransportMetric, COLLECT_PER_LINK,
        COLLECT_PER_TRANSPORT,
    },
    histogram::{Histogram, HistogramBuckets, PAYLOAD_SIZE_BUCKETS},
    labels::{
        BytesLabels, LinkLabels, LocalityLabel, NetworkMessageDroppedPayloadLabels,
        NetworkMessageLabels, NetworkMessagePayloadLabels, ProtocolLabels, ResourceDeclaredLabels,
        ResourceLabel, StatsPath, TransportLabels, TransportMessageLabels,
    },
    stats::init_stats,
    Rx, StatsDirection, TransportStats, Tx,
};

#[derive(Debug, Clone)]
pub struct StatsRegistry(Arc<StatsRegistryInner>);

impl StatsRegistry {
    pub fn new(zid: ZenohIdProto, whatami: WhatAmI, build_version: impl Into<String>) -> Self {
        let mut registry = Registry::with_prefix_and_labels(
            "zenoh",
            [
                ("local_id".into(), zid.to_string().into()),
                ("local_whatami".into(), whatami.to_string().into()),
            ]
            .into_iter(),
        );
        registry.register(
            "build",
            "Zenoh build version",
            Info::new([("version", build_version.into())]),
        );
        let transports_opened = Gauge::default();
        registry.register(
            "transports_opened",
            "Count of transports currently opened",
            transports_opened.clone(),
        );
        let links_opened = Family::default();
        registry.register(
            "links_opened",
            "Count of transports currently opened",
            links_opened.clone(),
        );
        let resources_declared = Family::default();
        registry.register(
            "resources_declared",
            "Count of resources currently declared",
            resources_declared.clone(),
        );
        let bytes = array::from_fn(|_dir| TransportFamily::default());
        let transport_message = array::from_fn(|_dir| TransportFamily::default());
        let network_message = array::from_fn(|_dir| TransportFamily::default());
        let network_message_payload =
            array::from_fn(|_dir| TransportFamily::new_with_constructor(PAYLOAD_SIZE_BUCKETS));
        let network_message_dropped_payload =
            array::from_fn(|_dir| TransportFamily::new_with_constructor(PAYLOAD_SIZE_BUCKETS));
        for dir in [Tx, Rx] {
            let action = match dir {
                Tx => "sent",
                Rx => "received",
            };
            registry.register_collector(Box::new(TransportFamilyCollector {
                name: format!("{dir}"),
                help: format!("Count of transport messages bytes {action}"),
                unit: Some(Unit::Bytes),
                family: bytes[dir as usize].clone(),
            }));
            registry.register_collector(Box::new(TransportFamilyCollector {
                name: format!("{dir}_transport_message"),
                help: format!("Count of transport messages {action}"),
                unit: None,
                family: transport_message[dir as usize].clone(),
            }));
            registry.register_collector(Box::new(TransportFamilyCollector {
                name: format!("{dir}_network_message"),
                help: format!("Count of network messages {action}"),
                unit: None,
                family: network_message[dir as usize].clone(),
            }));
            registry.register_collector(Box::new(TransportFamilyCollector {
                name: format!("{dir}_network_message_payload"),
                help: format!("Histogram of network messages payload {action}"),
                unit: Some(Unit::Bytes),
                family: network_message_payload[dir as usize].clone(),
            }));
            registry.register_collector(Box::new(TransportFamilyCollector {
                name: format!("{dir}_network_message_dropped_payload"),
                help: format!("Histogram of network messages payload dropped while {action}"),
                unit: Some(Unit::Bytes),
                family: network_message_dropped_payload[dir as usize].clone(),
            }));
        }
        Self(Arc::new(StatsRegistryInner {
            registry: RwLock::new(registry),
            transports_opened,
            links_opened,
            resources_declared,
            bytes,
            transport_message,
            network_message,
            network_message_payload,
            network_message_dropped_payload,
        }))
    }

    pub fn inc_resource_declared(&self, resource: ResourceLabel, locality: LocalityLabel) {
        let labels = ResourceDeclaredLabels { resource, locality };
        self.0.resources_declared.get_or_create(&labels).inc();
    }

    pub fn dec_resource_declared(&self, resource: ResourceLabel, locality: LocalityLabel) {
        let labels = ResourceDeclaredLabels { resource, locality };
        self.0.resources_declared.get_or_create(&labels).dec();
    }

    pub fn encode_metrics(
        &self,
        writer: &mut impl Write,
        per_transport: bool,
        per_link: bool,
    ) -> fmt::Result {
        let registry = self.0.registry.read().unwrap();
        COLLECT_PER_TRANSPORT.set(per_transport);
        COLLECT_PER_LINK.set(per_link);
        encode(writer, &registry)?;
        Ok(())
    }

    pub(crate) fn bytes(
        &self,
        direction: StatsDirection,
    ) -> &TransportFamily<BytesLabels, Counter> {
        &self.0.bytes[direction as usize]
    }

    pub(crate) fn transport_message(
        &self,
        direction: StatsDirection,
    ) -> &TransportFamily<TransportMessageLabels, Counter> {
        &self.0.transport_message[direction as usize]
    }

    pub(crate) fn network_message(
        &self,
        direction: StatsDirection,
    ) -> &TransportFamily<NetworkMessageLabels, Counter> {
        &self.0.network_message[direction as usize]
    }

    pub(crate) fn network_message_payload(
        &self,
        direction: StatsDirection,
    ) -> &TransportFamily<NetworkMessagePayloadLabels, Histogram, HistogramBuckets> {
        &self.0.network_message_payload[direction as usize]
    }

    pub(crate) fn network_message_dropped_payload(
        &self,
        direction: StatsDirection,
    ) -> &TransportFamily<NetworkMessageDroppedPayloadLabels, Histogram, HistogramBuckets> {
        &self.0.network_message_dropped_payload[direction as usize]
    }

    fn families(&self) -> impl Iterator<Item = (StatsDirection, &dyn TransportFamilyAny)> {
        [Tx, Rx].into_iter().flat_map(|dir| {
            iter::repeat(dir).zip([
                &self.0.bytes[dir as usize] as &dyn TransportFamilyAny,
                &self.0.transport_message[dir as usize],
                &self.0.network_message[dir as usize],
                &self.0.network_message_payload[dir as usize],
                &self.0.network_message_dropped_payload[dir as usize],
            ])
        })
    }

    pub fn merge_stats(&self, json: &mut serde_json::Value) {
        init_stats(json);
        for (dir, family) in self.families() {
            family.merge_stats(dir, json);
        }
    }

    pub fn unicast_transport_stats(
        &self,
        zid: ZenohIdProto,
        whatami: WhatAmI,
        cn: Option<String>,
    ) -> TransportStats {
        self.0.transports_opened.inc();
        TransportStats::new(self.clone(), Some(zid), Some(whatami), cn, None)
    }

    pub fn multicast_transport_stats(&self, group: String) -> TransportStats {
        self.0.transports_opened.inc();
        TransportStats::new(self.clone(), None, None, None, Some(group))
    }

    pub(crate) fn add_link(&self, link: &LinkLabels) {
        let protocol = ProtocolLabels {
            protocol: link.protocol(),
        };
        self.0.links_opened.get_or_create(&protocol).inc();
    }

    pub(crate) fn remove_transport(&self, transport: &TransportLabels) {
        for (_, family) in self.families() {
            family.remove_transport(transport);
        }
        self.0.transports_opened.dec();
    }

    pub(crate) fn remove_link(&self, transport: &TransportLabels, link: &LinkLabels) {
        for (_, family) in self.families() {
            family.remove_link(transport, link);
        }
        if transport.remote_group.is_none() || transport.remote_zid.is_none() {
            let protocol = ProtocolLabels {
                protocol: link.protocol(),
            };
            self.0.links_opened.get_or_create(&protocol).dec();
        }
    }
}

#[derive(Debug)]
struct StatsRegistryInner {
    registry: RwLock<Registry>,
    transports_opened: Gauge,
    links_opened: Family<ProtocolLabels, Gauge>,
    resources_declared: Family<ResourceDeclaredLabels, Gauge>,
    bytes: [TransportFamily<BytesLabels, Counter>; StatsDirection::NUM],
    transport_message: [TransportFamily<TransportMessageLabels, Counter>; StatsDirection::NUM],
    network_message: [TransportFamily<NetworkMessageLabels, Counter>; StatsDirection::NUM],
    network_message_payload:
        [TransportFamily<NetworkMessagePayloadLabels, Histogram, HistogramBuckets>;
            StatsDirection::NUM],
    network_message_dropped_payload:
        [TransportFamily<NetworkMessageDroppedPayloadLabels, Histogram, HistogramBuckets>;
            StatsDirection::NUM],
}

pub(crate) trait TransportFamilyAny {
    fn remove_link(&self, transport: &TransportLabels, link: &LinkLabels);
    fn remove_transport(&self, transport: &TransportLabels);
    fn merge_stats(&self, direction: StatsDirection, json: &mut serde_json::Value);
}

impl<S: StatsPath<M> + Clone + Hash + Eq, M: TransportMetric + Clone, C: MetricConstructor<M>>
    TransportFamilyAny for TransportFamily<S, M, C>
{
    fn remove_link(&self, transport: &TransportLabels, link: &LinkLabels) {
        self.remove_link(transport, link);
    }

    fn remove_transport(&self, transport: &TransportLabels) {
        self.remove_transport(transport);
    }

    fn merge_stats(&self, direction: StatsDirection, json: &mut serde_json::Value) {
        self.merge_stats(direction, json);
    }
}
