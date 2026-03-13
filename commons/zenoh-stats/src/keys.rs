use std::{
    cell::UnsafeCell,
    collections::HashMap,
    fmt, iter,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
};

use prometheus_client::{
    encoding::{EncodeLabelSet, MetricEncoder, NoLabelSet},
    metrics::{MetricType, TypedMetric},
};
use smallvec::SmallVec;
use zenoh_keyexpr::{
    keyexpr,
    keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, IKeyExprTreeNode, KeBoxTree},
};

use crate::{family::TransportMetric, histogram::HistogramBuckets, labels::LabelsSetRef};

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct StatsKeys(SmallVec<[(u64, usize); 1]>);

#[derive(Default)]
pub struct StatsKeyCache {
    keys: UnsafeCell<StatsKeys>,
    generation: AtomicU64,
    mutex: Mutex<()>,
}

unsafe impl Send for StatsKeyCache {}
unsafe impl Sync for StatsKeyCache {}

#[derive(Default)]
pub struct StatsKeysTree {
    generation: u64,
    tree: Option<KeBoxTree<usize>>,
}

impl StatsKeysTree {
    /// # Safety
    ///
    /// The cache must not be used with another tree.
    #[inline(always)]
    pub unsafe fn get_keys<'a>(
        &self,
        cache: impl FnOnce() -> Option<&'a StatsKeyCache>,
        keyexpr: impl FnOnce() -> Option<&'a keyexpr>,
    ) -> StatsKeys {
        if self.tree.is_none() {
            return StatsKeys::default();
        }
        if let Some(cache) = cache() {
            if cache.generation.load(Ordering::Acquire) != self.generation {
                self.update_cache(cache, keyexpr);
            }
            // SAFETY: Dereference the raw pointer.
            return unsafe { &*cache.keys.get() }.clone();
        }
        self.compute_keys(keyexpr)
    }

    #[cold]
    fn update_cache<'a>(
        &self,
        cache: &StatsKeyCache,
        keyexpr: impl FnOnce() -> Option<&'a keyexpr>,
    ) {
        // Compute the key before locking, in order to shorten the critical section.
        // The keys may be computed twice, but it matters less than blocking on a lock.
        let keys = self.compute_keys(keyexpr);
        let _guard = cache.mutex.lock().unwrap();
        // Do not override the cache if it has already been set.
        if cache.generation.load(Ordering::Acquire) == self.generation {
            return;
        }
        unsafe { *cache.keys.get() = keys };
        cache.generation.store(self.generation, Ordering::Release);
    }

    #[cold]
    fn compute_keys<'a>(&self, keyexpr: impl FnOnce() -> Option<&'a keyexpr>) -> StatsKeys {
        let tree = self.tree.as_ref().unwrap();
        let Some(keyexpr) = keyexpr() else {
            return StatsKeys::default();
        };
        let keys = tree
            .intersecting_nodes(keyexpr)
            .filter_map(|n| n.weight().cloned())
            .map(|key| (self.generation, key))
            .collect();
        StatsKeys(keys)
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct StatsKeysRegistry(Arc<RwLock<(Vec<String>, u64)>>);

impl StatsKeysRegistry {
    pub(crate) fn update_keys<'a>(
        &self,
        tree: &mut StatsKeysTree,
        keyexprs: impl IntoIterator<Item = &'a keyexpr>,
    ) {
        let keyexprs = keyexprs.into_iter().collect::<Vec<_>>();
        let (keys, generation) = &mut *self.0.write().unwrap();
        if keys.len() == keyexprs.len()
            && keys.iter().zip(&keyexprs).all(|(k1, k2)| k1 == k2.as_str())
        {
            return;
        }
        keys.clear();
        *generation += 1;
        tree.generation = *generation;
        tree.tree = None;
        for (i, keyexpr) in keyexprs.into_iter().enumerate() {
            keys.insert(i, keyexpr.to_string());
            tree.tree
                .get_or_insert_with(Default::default)
                .insert(keyexpr, i);
        }
    }

    pub(crate) fn keys(&self) -> Vec<String> {
        self.0.read().unwrap().0.clone()
    }
}

#[derive(Debug)]
struct HistogramPerKeyInner {
    stats_keys: StatsKeysRegistry,
    buckets: HistogramBuckets,
    #[allow(clippy::type_complexity)]
    map: ahash::HashMap<(u64, usize), (u64, Vec<(u64, u64)>)>,
}

impl HistogramPerKeyInner {
    fn histogram(&mut self, key: (u64, usize)) -> &mut (u64, Vec<(u64, u64)>) {
        self.map.entry(key).or_insert_with(|| {
            let buckets = (self.buckets.0.iter())
                .chain([&u64::MAX])
                .map(|b| (*b, 0))
                .collect();
            (0, buckets)
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HistogramPerKey(Arc<Mutex<HistogramPerKeyInner>>);

impl HistogramPerKey {
    pub(crate) fn new(buckets: HistogramBuckets, stats_keys: StatsKeysRegistry) -> Self {
        Self(Arc::new(Mutex::new(HistogramPerKeyInner {
            stats_keys,
            buckets,
            map: Default::default(),
        })))
    }

    #[inline(always)]
    pub(crate) fn observe(&self, keys: &StatsKeys, value: u64) {
        if keys.0.is_empty() {
            return;
        }
        self.observe_cold(keys, value);
    }

    #[cold]
    fn observe_cold(&self, keys: &StatsKeys, value: u64) {
        let inner = &mut *self.0.lock().unwrap();
        for key in keys.0.iter().copied() {
            let (sum, buckets) = inner.histogram(key);
            let (_, count) = buckets.iter_mut().find(|(b, _)| value <= *b).unwrap();
            *count += 1;
            *sum += value;
        }
    }
}

impl TypedMetric for HistogramPerKey {
    const TYPE: MetricType = MetricType::Histogram;
}

impl TransportMetric for HistogramPerKey {
    type Collected = HashMap<String, (f64, u64, Vec<(f64, u64)>)>;

    fn drain_into(&self, other: &Self) {
        let inner = &mut *self.0.lock().unwrap();
        let other = &mut other.0.lock().unwrap();
        for (key, (sum, buckets)) in inner.map.drain() {
            let (other_sum, other_buckets) = other.histogram(key);
            *other_sum += sum;
            for ((b, c), (other_b, other_c)) in iter::zip(buckets, other_buckets) {
                debug_assert_eq!(b, *other_b);
                *other_c += c;
            }
        }
    }

    fn collect(&self) -> Self::Collected {
        let inner = &mut *self.0.lock().unwrap();
        let (keys, generation) = &*inner.stats_keys.0.read().unwrap();
        let map_histogram = |sum, buckets: &[(u64, u64)]| {
            (
                sum as f64,
                buckets.iter().map(|(_, c)| c).sum(),
                buckets.iter().map(|(b, c)| (*b as f64, *c)).collect(),
            )
        };
        let mut collected = HashMap::new();
        inner.map.retain(|(gen, key), (sum, buckets)| {
            if gen != generation {
                return false;
            }
            collected.insert(keys[*key].clone(), map_histogram(*sum, buckets));
            true
        });
        collected
    }

    fn sum_collected(collected: &mut Self::Collected, other: &Self::Collected) {
        for (other_key, other) in other {
            if let Some((sum, count, buckets)) = collected.get_mut(other_key) {
                let (other_sum, other_count, other_buckets) = other;
                *sum += other_sum;
                *count += other_count;
                for ((b, c), (other_b, other_c)) in iter::zip(buckets, other_buckets) {
                    debug_assert_eq!(b, other_b);
                    *c += other_c;
                }
            } else {
                collected.insert(other_key.clone(), other.clone());
            }
        }
    }

    fn encode(
        encoder: &mut MetricEncoder,
        labels: &impl EncodeLabelSet,
        collected: &Self::Collected,
    ) -> fmt::Result {
        for (key, (sum, count, buckets)) in collected {
            encoder
                .encode_family(&(LabelsSetRef(labels), &[("key", key)] as &[_]))?
                .encode_histogram::<NoLabelSet>(*sum, *count, buckets, None)?;
        }
        Ok(())
    }
}
