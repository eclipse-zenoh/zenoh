use std::{
    collections::HashMap,
    fmt,
    hash::Hash,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeLabelSet, MetricEncoder, NoLabelSet},
    metrics::{counter::Counter, family::MetricConstructor, TypedMetric},
    registry::Unit,
};

use crate::{
    labels::{LinkLabels, RemoteLabels, StatsPath},
    StatsDirection,
};

const GARBAGE_COLLECTION_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub(crate) struct PerRemoteFamily<S, M, C = fn() -> M>(Arc<RwLock<PerRemoteFamilyInner<S, M, C>>>);

#[derive(Debug)]
struct PerRemoteFamilyInner<S, M, C> {
    per_remote: HashMap<RemoteLabels, RemoteFamily<S, M>>,
    disconnected: HashMap<S, M>,
    constructor: C,
}

#[derive(Debug)]
struct RemoteFamily<S, M> {
    per_link: HashMap<Option<LinkLabels>, HashMap<S, M>>,
    disconnection: Option<Instant>,
}

impl<S, M> Default for RemoteFamily<S, M> {
    fn default() -> Self {
        Self {
            per_link: HashMap::new(),
            disconnection: None,
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
            per_remote: Default::default(),
            disconnected: Default::default(),
            constructor,
        })))
    }
}

impl<S: StatsPath<M> + Clone + Hash + Eq, M: PerRemoteMetric + Clone, C: MetricConstructor<M>>
    PerRemoteFamily<S, M, C>
{
    pub fn get_or_create_owned(&self, remote: &RemoteLabels, labels: &S, link: &LinkLabels) -> M {
        let inner = &mut *self.0.write().unwrap();
        let family_entry = inner.per_remote.entry(remote.clone()).or_default();
        family_entry.disconnection = None;
        let link_entry = family_entry
            .per_link
            .entry(Some(link.clone()))
            .or_default()
            .entry(labels.clone());
        let new_metric = || inner.constructor.new_metric();
        link_entry.or_insert_with(new_metric).clone()
    }

    pub(crate) fn remove_link(&self, remote: &RemoteLabels, link: &LinkLabels) {
        let inner = &mut *self.0.write().unwrap();
        if let Some(family) = inner.per_remote.get_mut(remote) {
            if let Some(metrics) = family.per_link.remove(&Some(link.clone())) {
                for (labels, metric) in metrics {
                    let no_link_metrics = family.per_link.entry(None).or_default();
                    let new_metric = || inner.constructor.new_metric();
                    metric.drain_into(no_link_metrics.entry(labels).or_insert_with(new_metric));
                }
            }
        }
    }

    pub(crate) fn remove_transport(&self, remote: &RemoteLabels) {
        let inner = &mut *self.0.write().unwrap();
        if let Some(family_entry) = inner.per_remote.get_mut(remote) {
            family_entry.disconnection = Some(Instant::now());
        }
    }

    fn collect(&self) -> CollectResult<S, M> {
        let inner = self.0.read().unwrap();
        let now = Instant::now();
        let is_disconnected = |disconnection: Option<Instant>| matches!(disconnection, Some(instant) if now.duration_since(instant) > GARBAGE_COLLECTION_DELAY);
        let mut has_disconnection: bool = false;
        let mut all = inner
            .disconnected
            .iter()
            .map(|(s, m)| (s.clone(), m.collect()))
            .collect::<HashMap<_, _>>();
        let mut per_remotes = Vec::new();
        for (remote, family) in inner.per_remote.iter() {
            has_disconnection |= is_disconnected(family.disconnection);
            for (link, metrics) in family.per_link.iter() {
                for (labels, metric) in metrics {
                    let collected = metric.collect();
                    if let Some(link) = link {
                        per_remotes.push((
                            ((remote.clone(), link.clone()), labels.clone()),
                            collected.clone(),
                        ));
                    }
                    if let Some(c) = all.get_mut(labels) {
                        *c = M::merge_collected([c.clone(), collected]);
                    } else {
                        all.insert(labels.clone(), collected);
                    }
                }
            }
        }
        drop(inner);
        if has_disconnection {
            let inner = &mut *self.0.write().unwrap();
            inner.per_remote.retain(|_, family| {
                if is_disconnected(family.disconnection) {
                    for (labels, metric) in family
                        .per_link
                        .get_mut(&None)
                        .into_iter()
                        .flat_map(HashMap::drain)
                    {
                        let new_metric = || inner.constructor.new_metric();
                        metric.drain_into(
                            inner.disconnected.entry(labels).or_insert_with(new_metric),
                        );
                    }
                    return false;
                }
                true
            });
        }
        CollectResult { all, per_remotes }
    }

    pub(crate) fn merge_stats(&self, direction: StatsDirection, json: &mut serde_json::Value) {
        let inner = self.0.read().unwrap();
        for (labels, metric) in inner.disconnected.iter() {
            S::incr_stats(direction, None, None, labels, metric.collect(), json);
        }
        for (remote, family) in inner.per_remote.iter() {
            for (link, metrics) in family.per_link.iter() {
                for (labels, metric) in metrics {
                    S::incr_stats(
                        direction,
                        Some(remote),
                        link.as_ref(),
                        labels,
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
    per_remotes: Vec<(((RemoteLabels, LinkLabels), S), M::Collected)>,
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
