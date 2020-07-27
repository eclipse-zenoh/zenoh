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
#[macro_use]
extern crate criterion;

use criterion::Criterion;

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    BestMatching,
    Complete { n: u64 },
    All,
    None,
}

impl Default for Target {
    fn default() -> Self {
        Target::BestMatching
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct QueryTarget {
    pub storage: Target,
    pub eval: Target,
    // @TODO: finalise
}

pub const QUERY_TARGET_DEFAULT: QueryTarget = QueryTarget {
    storage: Target::BestMatching,
    eval: Target::BestMatching,
};

impl Default for &QueryTarget {
    fn default() -> Self {
        &QUERY_TARGET_DEFAULT
    }
}

pub fn pass_by_ref_option(arg: &Option<QueryTarget>, sum: &mut u64) {
    let v = arg.as_ref().unwrap_or_default();
    match v.eval {
        Target::BestMatching => *sum = *sum * 3,
        Target::Complete { n: _ } => *sum = *sum * 2,
        Target::All => *sum = *sum * 1,
        Target::None => *sum = *sum * 5,
    }
}

pub fn pass_by_option(arg: Option<QueryTarget>, sum: &mut u64) {
    let v = arg.unwrap_or_default();
    match v.eval {
        Target::BestMatching => *sum = *sum * 3,
        Target::Complete { n: _ } => *sum = *sum * 2,
        Target::All => *sum = *sum * 1,
        Target::None => *sum = *sum * 5,
    }
}

pub fn pass_by_ref(arg: &QueryTarget, sum: &mut u64) {
    match arg.eval {
        Target::BestMatching => *sum = *sum * 3,
        Target::Complete { n: _ } => *sum = *sum * 2,
        Target::All => *sum = *sum * 1,
        Target::None => *sum = *sum * 5,
    }
}

pub fn pass_by_val(arg: QueryTarget, sum: &mut u64) {
    match arg.eval {
        Target::BestMatching => *sum = *sum * 3,
        Target::Complete { n: _ } => *sum = *sum * 2,
        Target::All => *sum = *sum * 1,
        Target::None => *sum = *sum * 5,
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut sum1 = 0u64;
    c.bench_function("pass_by_ref_option None", |b| {
        b.iter(|| {
            pass_by_ref_option(&None, &mut sum1);
        })
    });

    let mut sum2 = 0u64;
    let v = Some(QUERY_TARGET_DEFAULT);
    c.bench_function("pass_by_ref_option Some()", |b| {
        b.iter(|| {
            pass_by_ref_option(&v, &mut sum2);
        })
    });

    let mut sum3 = 0u64;
    c.bench_function("pass_by_option None", |b| {
        b.iter(|| {
            pass_by_option(None, &mut sum3);
        })
    });

    let mut sum4 = 0u64;
    c.bench_function("pass_by_option Some()", |b| {
        b.iter(|| {
            pass_by_option(Some(QUERY_TARGET_DEFAULT), &mut sum4);
        })
    });

    let mut sum5 = 0u64;
    c.bench_function("pass_by_ref default", |b| {
        b.iter(|| {
            pass_by_ref(&QueryTarget::default(), &mut sum5);
        })
    });

    let mut sum6 = 0u64;
    c.bench_function("pass_by_ref QUERY_TARGET_DEFAULT", |b| {
        b.iter(|| {
            pass_by_ref(&QUERY_TARGET_DEFAULT, &mut sum6);
        })
    });

    let mut sum7 = 0u64;
    c.bench_function("pass_by_val default", |b| {
        b.iter(|| {
            pass_by_val(QueryTarget::default(), &mut sum7);
        })
    });

    let mut sum8 = 0u64;
    c.bench_function("pass_by_ref QUERY_TARGET_DEFAULT", |b| {
        b.iter(|| {
            pass_by_val(QUERY_TARGET_DEFAULT, &mut sum8);
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
