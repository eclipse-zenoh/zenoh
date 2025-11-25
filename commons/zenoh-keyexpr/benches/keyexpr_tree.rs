//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicU64,
};

use rand::SeedableRng;
use zenoh_keyexpr::{
    fuzzer::KeyExprFuzzer,
    keyexpr_tree::{
        impls::{HashMapProvider, VecSetProvider},
        traits::*,
        KeArcTree, KeBoxTree,
    },
    OwnedKeyExpr,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Averager {
    avg: f64,
    count: usize,
}
impl Averager {
    fn add(&mut self, value: f64) {
        self.avg += value;
        self.count += 1
    }
    fn avg(&self) -> f64 {
        self.avg / self.count as f64
    }
}

fn main() {
    for &total in &[10, 100, 1000, 10000][2..] {
        for (wildness, no_double_stars) in [(0., true), (0.1, true), (0.1, false)] {
            let mut intersections = Averager::default();
            let results = Benchmarker::benchmark(|b| {
                let keys = KeySet::generate(total, wildness, no_double_stars);
                let mut ketree = KeBoxTree::new();
                let mut vectree: KeBoxTree<_, bool, VecSetProvider> = KeBoxTree::default();
                let mut hashtree: KeBoxTree<_, bool, HashMapProvider> = KeBoxTree::default();
                let mut ahashtree: KeBoxTree<_, bool, HashMapProvider<ahash::AHasher>> =
                    KeBoxTree::default();
                let (kearctree, mut token): (KeArcTree<i32>, _) = KeArcTree::new().unwrap();
                let mut map = HashMap::new();
                for key in keys.iter() {
                    b.run_once("ketree_insert", || ketree.insert(key, 0));
                    b.run_once("kearctree_insert", || {
                        kearctree.insert(&mut token, key, 0);
                    });
                    b.run_once("vectree_insert", || vectree.insert(key, 0));
                    b.run_once("hashtree_insert", || hashtree.insert(key, 0));
                    b.run_once("ahashtree_insert", || ahashtree.insert(key, 0));
                    b.run_once("hashmap_insert", || map.insert(key.to_owned(), 0));
                }
                for key in keys.iter() {
                    b.run_once("ketree_fetch", || ketree.node(key));
                    b.run_once("kearctree_fetch", || kearctree.node(&token, key));
                    b.run_once("vectree_fetch", || vectree.node(key));
                    b.run_once("hashtree_fetch", || hashtree.node(key));
                    b.run_once("ahashtree_fetch", || ahashtree.node(key));
                    b.run_once("hashmap_fetch", || map.get(key));
                }
                for key in keys.iter() {
                    intersections.add(ketree.intersecting_nodes(key).count() as f64);
                    b.run_once("ketree_intersect", || {
                        ketree.intersecting_nodes(key).count()
                    });
                    b.run_once("kearctree_intersect", || {
                        kearctree.intersecting_nodes(&token, key).count()
                    });
                    b.run_once("vectree_intersect", || {
                        vectree.intersecting_nodes(key).count()
                    });
                    b.run_once("hashtree_intersect", || {
                        hashtree.intersecting_nodes(key).count()
                    });
                    b.run_once("ahashtree_intersect", || {
                        ahashtree.intersecting_nodes(key).count()
                    });
                    b.run_once("hashmap_intersect", || {
                        map.iter().filter(|(k, _)| key.intersects(k)).count()
                    });
                }
                for key in keys.iter() {
                    b.run_once("ketree_include", || ketree.included_nodes(key).count());
                    b.run_once("kearctree_include", || {
                        kearctree.included_nodes(&token, key).count()
                    });
                    b.run_once("vectree_include", || vectree.included_nodes(key).count());
                    b.run_once("hashtree_include", || hashtree.included_nodes(key).count());
                    b.run_once("ahashtree_include", || {
                        ahashtree.included_nodes(key).count()
                    });
                    b.run_once("hashmap_include", || {
                        map.iter().filter(|(k, _)| key.includes(k)).count()
                    });
                }
            });
            for name in [
                "ketree_insert",
                "kearctree_insert",
                "vectree_insert",
                "hashtree_insert",
                "ahashtree_insert",
                "hashmap_insert",
                "ketree_fetch",
                "kearctree_fetch",
                "vectree_fetch",
                "hashtree_fetch",
                "ahashtree_fetch",
                "hashmap_fetch",
                "ketree_intersect",
                "kearctree_intersect",
                "vectree_intersect",
                "hashtree_intersect",
                "ahashtree_intersect",
                "hashmap_intersect",
                "ketree_include",
                "kearctree_include",
                "vectree_include",
                "hashtree_include",
                "ahashtree_include",
                "hashmap_include",
            ] {
                let b = results.benches.get(name).unwrap();
                let stats = b.full_stats();
                let intersect = intersections.avg();
                let stars = if no_double_stars { "" } else { "**" };
                println!(
                    "{name}_{total}keys_{wildness}{stars}wilds ({intersect:.1})\n\t{stats:.2e}"
                )
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
        std::hint::black_box(f());
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
    fn generate(total: usize, wildness: f64, no_double_stars: bool) -> Self {
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
                if wilds.len() < n_wilds && !(no_double_stars && key.contains("**")) {
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
