use std::{
    fmt, iter,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use prometheus_client::{
    encoding::{EncodeMetric, MetricEncoder, NoLabelSet},
    metrics::{family::MetricConstructor, MetricType, TypedMetric},
};

use crate::family::TransportMetric;

pub const PAYLOAD_SIZE_BUCKETS: HistogramBuckets =
    HistogramBuckets(&[0, 1 << 5, 1 << 10, 1 << 15, 1 << 20, 1 << 25, 1 << 30]);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HistogramBuckets(pub &'static [u64]);

impl MetricConstructor<Histogram> for HistogramBuckets {
    fn new_metric(&self) -> Histogram {
        Histogram::new(self.clone())
    }
}

fn bound_to_f64(b: u64) -> f64 {
    if b == u64::MAX {
        // prometheus_client treats `f64::MAX` as infinity
        return f64::MAX;
    }
    b as f64
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

impl TransportMetric for Histogram {
    type Collected = (f64, u64, Vec<(f64, u64)>);

    fn drain_into(&self, other: &Self) {
        (other.0.sum).fetch_add(self.0.sum.load(Ordering::Relaxed), Ordering::Relaxed);
        for ((b, c), (other_b, other_c)) in iter::zip(&self.0.buckets, &other.0.buckets) {
            debug_assert_eq!(b, other_b);
            other_c.fetch_add(c.load(Ordering::Relaxed), Ordering::Relaxed);
        }
    }

    fn collect(&self) -> Self::Collected {
        let sum = self.0.sum.load(Ordering::Relaxed) as f64;
        let buckets = (self.0.buckets.iter())
            .map(|(b, c)| (bound_to_f64(*b), c.load(Ordering::Relaxed)))
            .collect::<Vec<_>>();
        let count = buckets.iter().map(|(_, c)| c).sum();
        (sum, count, buckets)
    }

    fn sum_collected(
        (sum, count, buckets): &mut Self::Collected,
        (other_sum, other_count, other_buckets): &Self::Collected,
    ) {
        *sum += other_sum;
        *count += other_count;
        for ((b, c), (other_b, other_c)) in iter::zip(buckets, other_buckets) {
            debug_assert_eq!(b, other_b);
            *c += other_c;
        }
    }

    fn encode(mut encoder: MetricEncoder, collected: &Self::Collected) -> fmt::Result {
        let (sum, count, buckets) = collected;
        encoder.encode_histogram::<NoLabelSet>(*sum, *count, buckets, None)
    }
}

impl EncodeMetric for Histogram {
    fn encode(&self, encoder: MetricEncoder) -> Result<(), fmt::Error> {
        <Self as TransportMetric>::encode(encoder, &self.collect())
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}
