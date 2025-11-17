use std::{
    fmt,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use prometheus_client::{
    encoding::{EncodeMetric, MetricEncoder, NoLabelSet},
    metrics::{family::MetricConstructor, MetricType, TypedMetric},
};

use crate::per_remote::PerRemoteMetric;

pub const PAYLOAD_SIZE_BUCKETS: HistogramBuckets =
    HistogramBuckets(&[0, 1 << 5, 1 << 10, 1 << 15, 1 << 20, 1 << 25, 1 << 30]);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HistogramBuckets(pub &'static [u64]);

impl MetricConstructor<Histogram> for HistogramBuckets {
    fn new_metric(&self) -> Histogram {
        Histogram::new(self.clone())
    }
}

#[derive(Debug)]
struct HistogramInner {
    sum: AtomicU64,
    buckets: Vec<(u64, AtomicU64)>,
}

#[derive(Debug, Clone)]
pub struct Histogram(Arc<HistogramInner>);

impl Histogram {
    pub fn new(buckets: HistogramBuckets) -> Self {
        Self(Arc::new(HistogramInner {
            sum: AtomicU64::new(0),
            buckets: buckets
                .0
                .iter()
                .chain([&u64::MAX])
                .map(|b| (*b, AtomicU64::new(0)))
                .collect(),
        }))
    }

    pub fn observe(&self, value: u64) {
        let (_, count) = self.0.buckets.iter().find(|(b, _)| value <= *b).unwrap();
        count.fetch_add(1, Ordering::Relaxed);
        self.0.sum.fetch_add(value, Ordering::Relaxed);
    }
}

impl TypedMetric for Histogram {
    const TYPE: MetricType = MetricType::Histogram;
}

impl PerRemoteMetric for Histogram {
    type Collected = (f64, u64, Vec<(f64, u64)>);

    fn drain_into(&self, other: &Self) {
        (other.0.sum).fetch_add(self.0.sum.load(Ordering::Relaxed), Ordering::Relaxed);
        for ((_, c), (_, other_c)) in self.0.buckets.iter().zip(&other.0.buckets) {
            other_c.fetch_add(c.load(Ordering::Relaxed), Ordering::Relaxed);
        }
    }

    fn collect(&self) -> Self::Collected {
        let sum = self.0.sum.load(Ordering::Relaxed) as f64;
        let buckets = (self.0.buckets.iter())
            .map(|(b, c)| (*b as f64, c.load(Ordering::Relaxed)))
            .collect::<Vec<_>>();
        let count = buckets.iter().map(|(_, c)| c).sum();
        (sum, count, buckets)
    }

    fn merge_collected(
        collected_iter: impl IntoIterator<Item = Self::Collected>,
    ) -> Self::Collected {
        let add_buckets = |b1: Vec<(f64, u64)>, b2: Vec<(f64, u64)>| -> Vec<_> {
            b1.into_iter()
                .zip(b2)
                .map(|((b1, c1), (_, c2))| (b1, c1 + c2))
                .collect()
        };
        collected_iter
            .into_iter()
            .reduce(|(s1, c1, b1), (s2, c2, b2)| (s1 + s2, c1 + c2, add_buckets(b1, b2)))
            .unwrap_or_default()
    }

    fn encode(mut encoder: MetricEncoder, collected: Self::Collected) -> fmt::Result {
        let (sum, count, buckets) = collected;
        encoder.encode_histogram::<NoLabelSet>(sum, count, &buckets, None)
    }
}

impl EncodeMetric for Histogram {
    fn encode(&self, encoder: MetricEncoder) -> Result<(), fmt::Error> {
        <Self as PerRemoteMetric>::encode(encoder, self.collect())
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}
