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
    histogram::{Histogram, HistogramBuckets, PAYLOAD_SIZE_BUCKETS},
    labels::{
        BytesLabels, LinkLabels, LocalityLabel, NetworkMessageDroppedPayloadLabels,
        NetworkMessageLabels, NetworkMessagePayloadLabels, RemoteLabels, ResourceDeclaredLabels,
        ResourceLabel, StatsPath, TransportMessageLabels,
    },
    per_remote::{PerRemoteFamily, PerRemoteFamilyCollector, PerRemoteMetric},
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
        let transport_opened = Gauge::default();
        registry.register(
            "transport_opened",
            "Count of transports currently opened",
            transport_opened.clone(),
        );
        let resource_declared = Family::default();
        registry.register(
            "resource_declared",
            "Count of resources currently declared",
            resource_declared.clone(),
        );
        let bytes = array::from_fn(|_dir| PerRemoteFamily::default());
        let transport_message = array::from_fn(|_dir| PerRemoteFamily::default());
        let network_message = array::from_fn(|_dir| PerRemoteFamily::default());
        let network_message_payload =
            array::from_fn(|_dir| PerRemoteFamily::new_with_constructor(PAYLOAD_SIZE_BUCKETS));
        let network_message_dropped_payload =
            array::from_fn(|_dir| PerRemoteFamily::new_with_constructor(PAYLOAD_SIZE_BUCKETS));
        for dir in [Tx, Rx] {
            let action = match dir {
                Tx => "sent",
                Rx => "received",
            };
            registry.register_collector(Box::new(PerRemoteFamilyCollector {
                name: format!("{dir}"),
                help: format!("Count of transport messages bytes {action}"),
                unit: Some(Unit::Bytes),
                family: bytes[dir as usize].clone(),
            }));
            registry.register_collector(Box::new(PerRemoteFamilyCollector {
                name: format!("{dir}_transport_message"),
                help: format!("Count of transport messages {action}"),
                unit: None,
                family: transport_message[dir as usize].clone(),
            }));
            registry.register_collector(Box::new(PerRemoteFamilyCollector {
                name: format!("{dir}_network_message"),
                help: format!("Count of network messages {action}"),
                unit: None,
                family: network_message[dir as usize].clone(),
            }));
            registry.register_collector(Box::new(PerRemoteFamilyCollector {
                name: format!("{dir}_network_message_payload"),
                help: format!("Histogram of network messages payload {action}"),
                unit: Some(Unit::Bytes),
                family: network_message_payload[dir as usize].clone(),
            }));
            registry.register_collector(Box::new(PerRemoteFamilyCollector {
                name: format!("{dir}_network_message_payload_dropped"),
                help: format!("Histogram of network messages payload dropped while {action}"),
                unit: Some(Unit::Bytes),
                family: network_message_dropped_payload[dir as usize].clone(),
            }));
        }
        Self(Arc::new(StatsRegistryInner {
            registry: RwLock::new(registry),
            transport_opened,
            resource_declared,
            bytes,
            transport_message,
            network_message,
            network_message_payload,
            network_message_dropped_payload,
        }))
    }

    pub fn inc_transport_opened(&self) {
        self.0.transport_opened.inc();
    }

    pub fn dec_transport_opened(&self) {
        self.0.transport_opened.dec();
    }

    pub fn inc_resource_declared(&self, resource: ResourceLabel, locality: LocalityLabel) {
        let labels = ResourceDeclaredLabels { resource, locality };
        self.0.resource_declared.get_or_create(&labels).inc();
    }

    pub fn dec_resource_declared(&self, resource: ResourceLabel, locality: LocalityLabel) {
        let labels = ResourceDeclaredLabels { resource, locality };
        self.0.resource_declared.get_or_create(&labels).dec();
    }

    pub fn encode_metrics(&self, writer: &mut impl Write) -> fmt::Result {
        encode(writer, &self.0.registry.read().unwrap())?;
        Ok(())
    }

    pub(crate) fn bytes(
        &self,
        direction: StatsDirection,
    ) -> &PerRemoteFamily<BytesLabels, Counter> {
        &self.0.bytes[direction as usize]
    }

    pub(crate) fn transport_message(
        &self,
        direction: StatsDirection,
    ) -> &PerRemoteFamily<TransportMessageLabels, Counter> {
        &self.0.transport_message[direction as usize]
    }

    pub(crate) fn network_message(
        &self,
        direction: StatsDirection,
    ) -> &PerRemoteFamily<NetworkMessageLabels, Counter> {
        &self.0.network_message[direction as usize]
    }

    pub(crate) fn network_message_payload(
        &self,
        direction: StatsDirection,
    ) -> &PerRemoteFamily<NetworkMessagePayloadLabels, Histogram, HistogramBuckets> {
        &self.0.network_message_payload[direction as usize]
    }

    pub(crate) fn network_message_dropped_payload(
        &self,
        direction: StatsDirection,
    ) -> &PerRemoteFamily<NetworkMessageDroppedPayloadLabels, Histogram, HistogramBuckets> {
        &self.0.network_message_dropped_payload[direction as usize]
    }

    fn families(&self) -> impl Iterator<Item = (StatsDirection, &dyn PerRemoteFamilyAny)> {
        [Tx, Rx].into_iter().flat_map(|dir| {
            iter::repeat(dir).zip([
                &self.0.bytes[dir as usize] as &dyn PerRemoteFamilyAny,
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
        TransportStats::new(self.clone(), Some(zid), Some(whatami), cn, None)
    }

    pub fn multicast_transport_stats(&self, group: String) -> TransportStats {
        TransportStats::new(self.clone(), None, None, None, Some(group))
    }

    pub(crate) fn remove_transport(&self, remote: &RemoteLabels) {
        for (_, family) in self.families() {
            family.remove_transport(remote);
        }
    }

    pub(crate) fn remove_link(&self, remote: &RemoteLabels, link: &LinkLabels) {
        for (_, family) in self.families() {
            family.remove_link(remote, link);
        }
    }
}

#[derive(Debug)]
struct StatsRegistryInner {
    registry: RwLock<Registry>,
    transport_opened: Gauge,
    resource_declared: Family<ResourceDeclaredLabels, Gauge>,
    bytes: [PerRemoteFamily<BytesLabels, Counter>; StatsDirection::NUM],
    transport_message: [PerRemoteFamily<TransportMessageLabels, Counter>; StatsDirection::NUM],
    network_message: [PerRemoteFamily<NetworkMessageLabels, Counter>; StatsDirection::NUM],
    network_message_payload:
        [PerRemoteFamily<NetworkMessagePayloadLabels, Histogram, HistogramBuckets>;
            StatsDirection::NUM],
    network_message_dropped_payload:
        [PerRemoteFamily<NetworkMessageDroppedPayloadLabels, Histogram, HistogramBuckets>;
            StatsDirection::NUM],
}

pub(crate) trait PerRemoteFamilyAny {
    fn remove_link(&self, remote: &RemoteLabels, link: &LinkLabels);
    fn remove_transport(&self, remote: &RemoteLabels);
    fn merge_stats(&self, direction: StatsDirection, json: &mut serde_json::Value);
}

impl<S: StatsPath<M> + Clone + Hash + Eq, M: PerRemoteMetric + Clone, C: MetricConstructor<M>>
    PerRemoteFamilyAny for PerRemoteFamily<S, M, C>
{
    fn remove_link(&self, remote: &RemoteLabels, link: &LinkLabels) {
        self.remove_link(remote, link);
    }

    fn remove_transport(&self, remote: &RemoteLabels) {
        self.remove_transport(remote);
    }

    fn merge_stats(&self, direction: StatsDirection, json: &mut serde_json::Value) {
        self.merge_stats(direction, json);
    }
}
