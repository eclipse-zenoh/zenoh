use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicU64,
};

use rand::SeedableRng;
use zenoh_collections::keyexpr_tree::{
    keyed_set_tree::{KeyedSetProvider, VecSetProvider},
    IKeyExprTree, IKeyExprTreeExt, KeyExprTree,
};
use zenoh_protocol_core::key_expr::{fuzzer::KeyExprFuzzer, OwnedKeyExpr};

fn main() {
    for total in [10, 100, 1000, 10000] {
        for wildness in [0., 0.1] {
            let results = Benchmarker::benchmark(|b| {
                let keys = KeySet::generate(total, wildness);
                let mut tree = KeyExprTree::<_, KeyedSetProvider>::new();
                let mut vectree = KeyExprTree::<_, VecSetProvider>::new();
                let mut map = HashMap::new();
                for key in keys.iter() {
                    b.run_once("ketree_insert", || tree.insert(key, 0));
                    b.run_once("vectree_insert", || vectree.insert(key, 0));
                    b.run_once("hashmap_insert", || map.insert(key.to_owned(), 0));
                }
                for key in keys.iter() {
                    b.run_once("ketree_fetch", || tree.node(key));
                    b.run_once("vectree_fetch", || vectree.node(key));
                    b.run_once("hashmap_fetch", || map.get(key));
                }
                for key in keys.iter() {
                    b.run_once("ketree_intersect", || tree.intersecting_nodes(key).count());
                    b.run_once("vectree_intersect", || {
                        vectree.intersecting_nodes(key).count()
                    });
                    b.run_once("hashmap_intersect", || {
                        map.iter().filter(|(k, _)| key.intersects(k)).count()
                    });
                }
                for key in keys.iter() {
                    b.run_once("ketree_include", || tree.included_nodes(key).count());
                    b.run_once("vectree_include", || vectree.included_nodes(key).count());
                    b.run_once("hashmap_include", || {
                        map.iter().filter(|(k, _)| key.includes(k)).count()
                    });
                }
            });
            for name in [
                "ketree_insert",
                "vectree_insert",
                "hashmap_insert",
                "ketree_fetch",
                "vectree_fetch",
                "hashmap_fetch",
                "ketree_intersect",
                "vectree_intersect",
                "hashmap_intersect",
                "ketree_include",
                "vectree_include",
                "hashmap_include",
            ] {
                let b = results.benches.get(name).unwrap();
                let stats = b.full_stats();
                println!("{name}_{total}keys_{wildness}wilds\n\t{:.2e}", stats)
            }
            println!();
        }
    }
}

pub struct Benchmarker {
    benches: HashMap<String, Bench>,
}
impl Benchmarker {
    fn benchmark<F: FnMut(&mut Self)>(mut f: F) -> Self {
        let mut global = Bench::default();
        let mut this = Benchmarker {
            benches: HashMap::new(),
        };
        global.run_until(
            || f(&mut this),
            std::time::Instant::now() + std::time::Duration::from_secs_f64(3.),
            10,
            0.1,
        );
        this
    }
    fn run_once<F: FnOnce() -> O, O, S: Into<String>>(&mut self, name: S, f: F) {
        self.benches.entry(name.into()).or_default().run_once(f);
    }
}
#[derive(Default)]
pub struct Bench {
    runs: Vec<std::time::Duration>,
}
pub struct Stats {
    mean: std::time::Duration,
    variance: f64,
}
pub struct FullStats {
    base: Stats,
    min: f64,
    max: f64,
}
impl std::fmt::Display for FullStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("avg: ")?;
        self.base.mean.as_secs_f64().fmt(f)?;
        f.write_str("s, min: ")?;
        self.min.fmt(f)?;
        f.write_str("s, max: ")?;
        self.max.fmt(f)?;
        f.write_str("s, var: ")?;
        self.base.variance.fmt(f)
    }
}
impl std::fmt::LowerExp for FullStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("avg: ")?;
        self.base.mean.as_secs_f64().fmt(f)?;
        f.write_str("s, min: ")?;
        self.min.fmt(f)?;
        f.write_str("s, max: ")?;
        self.max.fmt(f)?;
        f.write_str("s, var: ")?;
        self.base.variance.fmt(f)
    }
}
impl Bench {
    fn run_once<F: FnOnce() -> O, O>(&mut self, f: F) {
        let start = std::time::Instant::now();
        criterion::black_box(f());
        let t = start.elapsed();
        self.runs.push(t);
    }
    fn mean(&self) -> std::time::Duration {
        self.runs.iter().sum::<std::time::Duration>() / self.runs.len() as u32
    }
    fn stats(&self) -> Stats {
        let mean = self.mean();
        let variance = self.runs.iter().fold(0., |acc, &it| {
            let diff = (if mean < it { it - mean } else { mean - it }).as_secs_f64();
            acc + diff * diff
        });
        Stats { mean, variance }
    }
    fn full_stats(&self) -> FullStats {
        let base = self.stats();
        let (min, max) = self
            .runs
            .iter()
            .fold((f64::MAX, f64::MIN), |(min, max), it| {
                let it = it.as_secs_f64();
                (min.min(it), max.max(it))
            });
        FullStats { base, min, max }
    }
    fn run_until<F: FnMut() -> O, O>(
        &mut self,
        mut f: F,
        deadline: std::time::Instant,
        min_runs: u32,
        variance: f64,
    ) -> Stats {
        for i in 1.. {
            self.run_once(&mut f);
            let stats = self.stats();
            if deadline < std::time::Instant::now() {
                return stats;
            }
            if i >= min_runs && stats.variance <= variance {
                return stats;
            }
        }
        self.stats()
    }
}

struct KeySet {
    non_wilds: Vec<OwnedKeyExpr>,
    wilds: Vec<OwnedKeyExpr>,
}
static SEED: AtomicU64 = AtomicU64::new(42);
impl KeySet {
    fn generate(total: usize, wildness: f64) -> Self {
        let n_wilds = (total as f64 * wildness) as usize;
        let n_non_wilds = total - n_wilds;
        let mut wilds = HashSet::with_capacity(n_wilds);
        let mut non_wilds = HashSet::with_capacity(n_non_wilds);
        let rng = rand::rngs::StdRng::seed_from_u64(
            SEED.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        );
        let keygen = KeyExprFuzzer(rng);
        for key in keygen {
            if key.is_wild() {
                if wilds.len() < n_wilds {
                    wilds.insert(key);
                }
            } else if non_wilds.len() < n_non_wilds {
                non_wilds.insert(key);
            }
            if wilds.len() == n_wilds && non_wilds.len() == n_non_wilds {
                break;
            }
        }
        KeySet {
            non_wilds: non_wilds.into_iter().collect(),
            wilds: wilds.into_iter().collect(),
        }
    }
    fn iter(&self) -> impl Iterator<Item = &OwnedKeyExpr> {
        self.non_wilds.iter().chain(self.wilds.iter())
    }
}
impl std::fmt::Display for KeySet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total = self.non_wilds.len() + self.wilds.len();
        let wildness = total * 100 / self.wilds.len();
        write!(f, "{total}keys_{wildness}%wilds")
    }
}
