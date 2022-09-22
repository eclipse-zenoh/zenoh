//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use criterion::{criterion_group, criterion_main, Criterion};
use lazy_static::__Deref;
use rand::SeedableRng;
use zenoh_protocol_core::key_expr::{
    fuzzer::KeyExprFuzzer, intersect::Intersector, keyexpr, OwnedKeyExpr,
};
#[derive(Debug, Clone)]
struct BoundedVec<T>(Vec<T>);
impl<T> BoundedVec<T> {
    fn new(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }
    fn push(&mut self, t: T) -> Result<(), ()>
    where
        T: PartialEq,
    {
        if self.full() || self.0.contains(&t) {
            Err(())
        } else {
            self.0.push(t);
            Ok(())
        }
    }
    fn full(&self) -> bool {
        self.0.len() == self.0.capacity()
    }
}
impl<T> std::ops::Deref for BoundedVec<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
struct BenchSet<'a, S: AsRef<str>, T> {
    name: S,
    set: BoundedVec<T>,
    condition: Box<dyn Fn(T) -> bool + 'a>,
}
impl<'a, S: AsRef<str>, T> BenchSet<'a, S, T> {
    fn new<F: Fn(T) -> bool + 'a>(name: S, capacity: usize, condition: F) -> Self {
        BenchSet {
            name,
            set: BoundedVec::new(capacity),
            condition: Box::new(condition),
        }
    }
}

fn bench<S: AsRef<str>, I: for<'a> Intersector<&'a keyexpr, &'a keyexpr>>(
    c: &mut Criterion,
    matcher: I,
    matcher_type: &str,
    set: &BenchSet<S, &keyexpr>,
) {
    let label = set.name.as_ref();
    let label = format!("{label} {matcher_type}");
    eprintln!("Running bench {label}");
    let set = set.set.deref();
    c.bench_function(&label, |b| {
        b.iter(|| {
            for a in set {
                for b in set {
                    matcher.intersect(a, b);
                }
            }
        })
    });
}
macro_rules! bench_for {
	($c: expr, $sets: expr, $($t: expr),*) => {
        eprintln!();
		$(bench($c, $t, stringify!($t), $sets);)*
	};
}
fn is_wild(ke: &keyexpr) -> bool {
    ke.contains('*')
}
fn no_sub_wild(ke: &keyexpr) -> bool {
    ke.contains('*') && !ke.contains('$')
}
fn isnt_wild(ke: &keyexpr) -> bool {
    !ke.contains('*')
}
fn criterion_benchmark(c: &mut Criterion) {
    const N_KEY_EXPR: usize = 100;
    const RNG_SEED: [u8; 32] = [42; 32];
    let mut ke_owner: Vec<OwnedKeyExpr> = Vec::new();
    let mut bench_sets = [
        BenchSet::new("isnt_wild|no_sub_wild", N_KEY_EXPR, |ke| {
            isnt_wild(ke) || no_sub_wild(ke)
        }),
        BenchSet::new("isnt_wild", N_KEY_EXPR, isnt_wild),
        BenchSet::new("no_sub_wild", N_KEY_EXPR, no_sub_wild),
        BenchSet::new("is_sub_wild", N_KEY_EXPR, |ke| {
            is_wild(ke) && !no_sub_wild(ke)
        }),
        BenchSet::new("isnt_wild|is_wild|no_sub_wild", N_KEY_EXPR, |ke| {
            isnt_wild(ke) || is_wild(ke) || no_sub_wild(ke)
        }),
    ];
    for ke in KeyExprFuzzer(rand::rngs::StdRng::from_seed(RNG_SEED)) {
        println!("{}", ke);
        ke_owner.push(ke);
        let ke =
            unsafe { std::mem::transmute::<&keyexpr, &keyexpr>(ke_owner.last().unwrap().deref()) };
        for set in bench_sets.iter_mut() {
            if (set.condition)(ke) {
                let _ = set.set.push(ke);
            }
        }
        if bench_sets.iter().all(|set| set.set.full()) {
            break;
        }
    }
    for set in &bench_sets {
        let (non_wilds, wilds, restricted_wilds) =
            set.set
                .iter()
                .fold((0, 0, 0), |(non_wilds, wilds, restricted_wilds), ke| {
                    (
                        non_wilds + isnt_wild(ke) as usize,
                        wilds + (is_wild(ke) && !no_sub_wild(ke)) as usize,
                        restricted_wilds + no_sub_wild(ke) as usize,
                    )
                });
        eprintln!(
            "{} completed: {} non-wilds, {} restricted wilds, {} others",
            set.name, non_wilds, restricted_wilds, wilds
        );
    }
    use zenoh_protocol_core::key_expr::intersect::ClassicIntersector;
    for bench in &bench_sets {
        bench_for!(c, bench, ClassicIntersector);
        // LeftToRightIntersector(LTRChunkIntersector),
        // MiddleOutIntersector(LTRChunkIntersector)
    }

    // for set in &bench_sets {
    //     let (non_wilds, wilds, restricted_wilds) =
    //         set.set
    //             .iter()
    //             .fold((0, 0, 0), |(non_wilds, wilds, restricted_wilds), ke| {
    //                 (
    //                     non_wilds + isnt_wild(ke) as usize,
    //                     wilds + (is_wild(ke) && !no_sub_wild(ke)) as usize,
    //                     restricted_wilds + no_sub_wild(ke) as usize,
    //                 )
    //             });
    //     eprintln!(
    //         "{} completed: {} non-wilds, {} restricted wilds, {} others",
    //         set.name, non_wilds, restricted_wilds, wilds
    //     );
    //     for ke in set.set.iter() {
    //         eprintln!("\t{}", ke);
    //     }
    // }
    println!("DONE")
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
