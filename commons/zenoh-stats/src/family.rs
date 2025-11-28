use std::{
    any::TypeId,
    cell::Cell,
    collections::{hash_map::Entry, HashMap},
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
    keys::HistogramPerKey,
    labels::{DisconnectedLabels, LabelsSetRef, LinkLabels, StatsPath, TransportLabels},
    StatsDirection,
};

const GARBAGE_COLLECTION_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub(crate) struct TransportFamily<S, M, C = fn() -> M>(Arc<RwLock<TransportFamilyInner<S, M, C>>>);

#[derive(Debug)]
struct TransportFamilyInner<S, M, C> {
    transports: HashMap<TransportLabels, TransportState<S, M>>,
    disconnected: HashMap<S, M>,
    constructor: C,
}

#[derive(Debug)]
struct TransportState<S, M> {
    links: HashMap<Option<LinkLabels>, HashMap<S, M>>,
    disconnection: Option<Instant>,
}

impl<S, M> Default for TransportState<S, M> {
    fn default() -> Self {
        Self {
            links: HashMap::new(),
            disconnection: None,
        }
    }
}

impl<S, M, C> Clone for TransportFamily<S, M, C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S: Clone + Hash + Eq, M: Default> Default for TransportFamily<S, M> {
    fn default() -> Self {
        Self::new_with_constructor(Default::default)
    }
}

impl<S: Clone + Hash + Eq, M, C: MetricConstructor<M>> TransportFamily<S, M, C> {
    pub fn new_with_constructor(constructor: C) -> Self {
        Self(Arc::new(RwLock::new(TransportFamilyInner {
            transports: Default::default(),
            disconnected: Default::default(),
            constructor,
        })))
    }
}

impl<S: StatsPath<M> + Clone + Hash + Eq, M: TransportMetric + Clone, C: MetricConstructor<M>>
    TransportFamily<S, M, C>
{
    pub(crate) fn get_or_create_owned(
        &self,
        transport: &TransportLabels,
        link: Option<&LinkLabels>,
        labels: &S,
    ) -> M {
        let inner = &mut *self.0.write().unwrap();
        let family_entry = inner.transports.entry(transport.clone()).or_default();
        family_entry.disconnection = None;
        let link_entry = family_entry
            .links
            .entry(link.cloned())
            .or_default()
            .entry(labels.clone());
        let new_metric = || inner.constructor.new_metric();
        link_entry.or_insert_with(new_metric).clone()
    }

    pub(crate) fn remove_link(&self, transport: &TransportLabels, link: &LinkLabels) {
        let inner = &mut *self.0.write().unwrap();
        if let Some(family) = inner.transports.get_mut(transport) {
            if let Some(metrics) = family.links.remove(&Some(link.clone())) {
                for (labels, metric) in metrics {
                    let no_link_metrics = family.links.entry(None).or_default();
                    let new_metric = || inner.constructor.new_metric();
                    metric.drain_into(no_link_metrics.entry(labels).or_insert_with(new_metric));
                }
            }
        }
    }

    pub(crate) fn remove_transport(&self, transport: &TransportLabels) {
        let inner = &mut *self.0.write().unwrap();
        if let Some(family_entry) = inner.transports.get_mut(transport) {
            family_entry.disconnection = Some(Instant::now());
        }
    }

    fn collect(&self) -> HashMap<S, (M::Collected, Vec<TransportCollected<M>>)> {
        let inner = self.0.read().unwrap();
        let now = Instant::now();
        let needs_garbage_collection = |disconnection: Option<Instant>| matches!(disconnection, Some(instant) if now.duration_since(instant) > GARBAGE_COLLECTION_DELAY);
        let mut garbage_collection: bool = false;
        let mut results = inner
            .disconnected
            .iter()
            .map(|(s, m)| (s.clone(), (m.collect(), Vec::new())))
            .collect::<HashMap<S, (M::Collected, Vec<TransportCollected<M>>)>>();
        for (transport, state) in inner.transports.iter() {
            let disconnected = state.disconnection.is_some();
            garbage_collection |= needs_garbage_collection(state.disconnection);
            if disconnected && !COLLECT_DISCONNECTED.get() {
                continue;
            }
            for (link, metrics) in state.links.iter() {
                for (labels, metric) in metrics {
                    let collected = metric.collect();
                    let transports = match results.entry(labels.clone()) {
                        Entry::Occupied(mut entry) => {
                            M::sum_collected(&mut entry.get_mut().0, &collected);
                            &mut entry.into_mut().1
                        }
                        Entry::Vacant(entry) => {
                            &mut entry.insert((collected.clone(), Vec::new())).1
                        }
                    };
                    let links = match transports.last_mut() {
                        Some(t) if t.transport == *transport => {
                            M::sum_collected(&mut t.collected, &collected);
                            &mut t.links
                        }
                        _ => {
                            transports.push(TransportCollected {
                                transport: transport.clone(),
                                disconnected: DisconnectedLabels { disconnected },
                                collected: collected.clone(),
                                links: Vec::new(),
                            });
                            &mut transports.last_mut().unwrap().links
                        }
                    };
                    if let Some(link) = link {
                        links.push((link.clone(), collected));
                    }
                }
            }
        }
        drop(inner);
        if garbage_collection {
            let inner = &mut *self.0.write().unwrap();
            inner.transports.retain(|_, family| {
                if needs_garbage_collection(family.disconnection) {
                    for (labels, metric) in family
                        .links
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
        results
    }

    pub(crate) fn merge_stats(&self, direction: StatsDirection, json: &mut serde_json::Value) {
        let inner = self.0.read().unwrap();
        for (labels, metric) in inner.disconnected.iter() {
            S::incr_stats(direction, None, None, labels, metric.collect(), json);
        }
        for (transport, family) in inner.transports.iter() {
            for (link, metrics) in family.links.iter() {
                for (labels, metric) in metrics {
                    S::incr_stats(
                        direction,
                        Some(transport),
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

pub(crate) trait TransportMetric: 'static {
    type Collected: Clone;
    fn drain_into(&self, other: &Self);
    fn collect(&self) -> Self::Collected;
    fn sum_collected(collected: &mut Self::Collected, other: &Self::Collected);
    fn encode(
        encoder: &mut MetricEncoder,
        labels: &impl EncodeLabelSet,
        collected: &Self::Collected,
    ) -> fmt::Result;
}

impl TransportMetric for Counter {
    type Collected = u64;

    fn drain_into(&self, other: &Self) {
        other.inc_by(self.get());
    }

    fn collect(&self) -> Self::Collected {
        self.get()
    }

    fn sum_collected(collected: &mut Self::Collected, other: &Self::Collected) {
        *collected += other;
    }

    fn encode(
        encoder: &mut MetricEncoder,
        labels: &impl EncodeLabelSet,
        collected: &Self::Collected,
    ) -> fmt::Result {
        encoder
            .encode_family(labels)?
            .encode_counter::<NoLabelSet, _, u64>(collected, None)
    }
}

struct TransportCollected<M: TransportMetric> {
    transport: TransportLabels,
    disconnected: DisconnectedLabels,
    collected: M::Collected,
    links: Vec<(LinkLabels, M::Collected)>,
}

thread_local! {
    pub(crate) static COLLECT_PER_TRANSPORT: Cell<bool> = const { Cell::new(false) };
    pub(crate) static COLLECT_PER_LINK: Cell<bool> = const { Cell::new(false) };
    pub(crate) static COLLECT_DISCONNECTED: Cell<bool> = const { Cell::new(false) };
    pub(crate) static COLLECT_PER_KEY: Cell<bool> = const { Cell::new(false) };
}

#[derive(Debug)]
pub(crate) struct TransportFamilyCollector<S, M, C> {
    pub(crate) name: String,
    pub(crate) help: String,
    pub(crate) unit: Option<Unit>,
    pub(crate) family: TransportFamily<S, M, C>,
}

impl<
        S: EncodeLabelSet + StatsPath<M> + Clone + Hash + Eq + fmt::Debug + Send + Sync + 'static,
        M: TypedMetric + TransportMetric + Clone + fmt::Debug + Send + Sync + 'static,
        C: MetricConstructor<M> + fmt::Debug + Send + Sync + 'static,
    > Collector for TransportFamilyCollector<S, M, C>
{
    fn encode(&self, mut encoder: DescriptorEncoder) -> fmt::Result {
        if TypeId::of::<M>() == TypeId::of::<HistogramPerKey>() && !COLLECT_PER_KEY.get() {
            return Ok(());
        }
        let collected = self.family.collect();
        let mut metric_encoder =
            encoder.encode_descriptor(&self.name, &self.help, self.unit.as_ref(), M::TYPE)?;
        for (labels, (collected, _)) in collected.iter() {
            M::encode(&mut metric_encoder, labels, collected)?
        }
        if COLLECT_PER_TRANSPORT.get() {
            let per_transport = format!("{name}_per_transport", name = self.name);
            let mut metric_encoder = encoder.encode_descriptor(
                &per_transport,
                &format!("{help} (per transport)", help = self.help),
                self.unit.as_ref(),
                M::TYPE,
            )?;
            for (labels, (_, transports)) in collected.iter() {
                for transport in transports {
                    let labels = (
                        LabelsSetRef(labels),
                        (LabelsSetRef(&transport.transport), transport.disconnected),
                    );
                    M::encode(&mut metric_encoder, &labels, &transport.collected)?
                }
            }
        }
        if COLLECT_PER_LINK.get() {
            let per_link = format!("{name}_per_link", name = self.name);
            let mut metric_encoder = encoder.encode_descriptor(
                &per_link,
                &format!("{help} (per link)", help = self.help),
                self.unit.as_ref(),
                M::TYPE,
            )?;
            for (labels, (_, transports)) in collected.iter() {
                for transport in transports {
                    for (link, collected) in transport.links.iter() {
                        let labels = (
                            LabelsSetRef(labels),
                            (LabelsSetRef(&transport.transport), LabelsSetRef(link)),
                        );
                        M::encode(&mut metric_encoder, &labels, collected)?
                    }
                }
            }
        }
        Ok(())
    }
}
