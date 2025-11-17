use std::{
    collections::HashMap,
    fmt,
    hash::Hash,
    sync::{Arc, RwLock},
};

use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeLabelSet, MetricEncoder, NoLabelSet},
    metrics::{counter::Counter, family::MetricConstructor, TypedMetric},
    registry::Unit,
};
use zenoh_link_commons::LinkId;

use crate::{
    labels::{RemoteLabels, StatsPath},
    StatsDirection,
};

#[derive(Debug)]
pub(crate) struct PerRemoteFamily<S, M, C = fn() -> M>(Arc<RwLock<PerRemoteFamilyInner<S, M, C>>>);

#[derive(Debug)]
struct PerRemoteFamilyInner<S, M, C> {
    connected: HashMap<RemoteLabels, RemoteFamily<S, M>>,
    disconnected: HashMap<S, M>,
    constructor: C,
}

#[derive(Debug)]
struct RemoteFamily<S, M> {
    family: HashMap<S, HashMap<Option<LinkId>, M>>,
    disconnected: bool,
}

impl<S, M> Default for RemoteFamily<S, M> {
    fn default() -> Self {
        Self {
            family: HashMap::new(),
            disconnected: false,
        }
    }
}

impl<S, M, C> Clone for PerRemoteFamily<S, M, C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S: Clone + Hash + Eq, M: Default> Default for PerRemoteFamily<S, M> {
    fn default() -> Self {
        Self::new_with_constructor(Default::default)
    }
}

impl<S: Clone + Hash + Eq, M, C: MetricConstructor<M>> PerRemoteFamily<S, M, C> {
    pub fn new_with_constructor(constructor: C) -> Self {
        Self(Arc::new(RwLock::new(PerRemoteFamilyInner {
            connected: Default::default(),
            disconnected: Default::default(),
            constructor,
        })))
    }
}

impl<S: StatsPath<M> + Clone + Hash + Eq, M: PerRemoteMetric + Clone, C: MetricConstructor<M>>
    PerRemoteFamily<S, M, C>
{
    pub fn get_or_create_owned(
        &self,
        remote: &RemoteLabels,
        labels: &S,
        link_id: Option<LinkId>,
    ) -> M {
        let inner = &mut *self.0.write().unwrap();
        let family_entry = inner.connected.entry(remote.clone()).or_default();
        family_entry.disconnected = false;
        let link_entry = family_entry
            .family
            .entry(labels.clone())
            .or_default()
            .entry(link_id);
        let new_metric = || inner.constructor.new_metric();
        link_entry.or_insert_with(new_metric).clone()
    }

    pub(crate) fn remove_link(&self, remote: &RemoteLabels, link_id: LinkId) {
        let inner = &mut *self.0.write().unwrap();
        if let Some(family_entry) = inner.connected.get_mut(remote) {
            for links in family_entry.family.values_mut() {
                if let Some(metric) = links.remove(&Some(link_id)) {
                    let new_metric = || inner.constructor.new_metric();
                    metric.drain_into(links.entry(None).or_insert_with(new_metric));
                }
            }
        }
    }

    pub(crate) fn remove_transport(&self, remote: &RemoteLabels) {
        let inner = &mut *self.0.write().unwrap();
        if let Some(family_entry) = inner.connected.get_mut(remote) {
            family_entry.disconnected = true;
        }
    }

    fn collect(&self) -> CollectResult<S, M> {
        let inner = self.0.read().unwrap();
        let mut all = inner
            .disconnected
            .iter()
            .map(|(s, m)| (s.clone(), m.collect()))
            .collect::<HashMap<_, _>>();
        let mut per_remotes = Vec::new();
        for (remote, family) in inner.connected.iter() {
            for (labels, links) in family.family.iter() {
                let collected = M::merge_collected(links.values().map(|m| m.collect()));
                per_remotes.push(((remote.clone(), labels.clone()), collected.clone()));
                if let Some(c) = all.get_mut(labels) {
                    *c = M::merge_collected([c.clone(), collected]);
                } else {
                    all.insert(labels.clone(), collected);
                }
            }
        }
        drop(inner);
        let inner = &mut *self.0.write().unwrap();
        inner.connected.retain(|_, family| {
            if family.disconnected {
                for (labels, mut links) in family.family.drain() {
                    if let Some(metric) = links.remove(&None) {
                        let new_metric = || inner.constructor.new_metric();
                        metric.drain_into(
                            inner.disconnected.entry(labels).or_insert_with(new_metric),
                        );
                    }
                }
            }
            !family.disconnected
        });
        CollectResult { all, per_remotes }
    }

    pub(crate) fn merge_stats(&self, direction: StatsDirection, json: &mut serde_json::Value) {
        let inner = self.0.read().unwrap();
        for (labels, metric) in inner.disconnected.iter() {
            S::incr_stats(direction, None, labels, None, metric.collect(), json);
        }
        for (remote, family) in inner.connected.iter() {
            for (labels, links) in family.family.iter() {
                for (link, metric) in links {
                    S::incr_stats(
                        direction,
                        Some(remote),
                        labels,
                        *link,
                        metric.collect(),
                        json,
                    );
                }
            }
        }
    }
}

pub(crate) struct CollectResult<S, M: PerRemoteMetric> {
    all: HashMap<S, M::Collected>,
    per_remotes: Vec<((RemoteLabels, S), M::Collected)>,
}

pub(crate) trait PerRemoteMetric: 'static {
    type Collected: Default + Clone;
    fn drain_into(&self, other: &Self);
    fn collect(&self) -> Self::Collected;
    fn merge_collected(
        collected_iter: impl IntoIterator<Item = Self::Collected>,
    ) -> Self::Collected;
    fn encode(encoder: MetricEncoder, collected: Self::Collected) -> fmt::Result;
}

impl PerRemoteMetric for Counter {
    type Collected = u64;

    fn drain_into(&self, other: &Self) {
        other.inc_by(self.get());
    }

    fn collect(&self) -> Self::Collected {
        self.get()
    }

    fn merge_collected(
        collected_iter: impl IntoIterator<Item = Self::Collected>,
    ) -> Self::Collected {
        collected_iter.into_iter().sum()
    }

    fn encode(mut encoder: MetricEncoder, collected: Self::Collected) -> fmt::Result {
        encoder.encode_counter::<NoLabelSet, _, u64>(&collected, None)
    }
}

#[derive(Debug)]
pub(crate) struct PerRemoteFamilyCollector<S, M, C> {
    pub(crate) name: String,
    pub(crate) help: String,
    pub(crate) unit: Option<Unit>,
    pub(crate) family: PerRemoteFamily<S, M, C>,
}

impl<
        S: EncodeLabelSet + StatsPath<M> + Clone + Hash + Eq + fmt::Debug + Send + Sync + 'static,
        M: TypedMetric + PerRemoteMetric + Clone + fmt::Debug + Send + Sync + 'static,
        C: MetricConstructor<M> + fmt::Debug + Send + Sync + 'static,
    > Collector for PerRemoteFamilyCollector<S, M, C>
{
    fn encode(&self, mut encoder: DescriptorEncoder) -> fmt::Result {
        let collected = self.family.collect();
        let mut metric_encoder =
            encoder.encode_descriptor(&self.name, &self.help, self.unit.as_ref(), M::TYPE)?;
        for (labels, collected) in collected.all {
            M::encode(metric_encoder.encode_family(&labels)?, collected)?
        }
        let per_remote_name = format!("{name}_per_remote", name = self.name);
        let mut metric_encoder = encoder.encode_descriptor(
            &per_remote_name,
            &format!("{help} (per remote)", help = self.help),
            self.unit.as_ref(),
            M::TYPE,
        )?;
        for (labels, collected) in collected.per_remotes {
            M::encode(metric_encoder.encode_family(&labels)?, collected)?
        }
        Ok(())
    }
}
