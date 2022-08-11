//
// Copyright (c) 2022 ZettaScale Technology
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
#[macro_use]
extern crate criterion;

use criterion::Criterion;

#[derive(Debug, Clone, PartialEq)]
pub enum QueryTarget {
    BestMatching,
    All,
    AllComplete,
    None,
    #[cfg(feature = "complete_n")]
    Complete(u64),
}

impl Default for QueryTarget {
    fn default() -> Self {
        QueryTarget::BestMatching
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct QueryTAK {
    pub storage: QueryTarget,
    pub eval: QueryTarget,
    // @TODO: finalise
}

pub const QUERY_TARGET_DEFAULT: QueryTAK = QueryTAK {
    storage: QueryTarget::BestMatching,
    eval: QueryTarget::BestMatching,
};

impl Default for &QueryTAK {
    fn default() -> Self {
        &QUERY_TARGET_DEFAULT
    }
}

pub fn pass_by_ref_option(arg: &Option<QueryTAK>, sum: &mut u64) {
    let v = arg.as_ref().unwrap_or_default();
    match v.eval {
        QueryTarget::BestMatching => *sum *= 3,
        QueryTarget::AllComplete => *sum *= 2,
        QueryTarget::All => (),
        QueryTarget::None => *sum *= 5,
        #[cfg(feature = "complete_n")]
        QueryTarget::Complete(_) => *sum *= 4,
    }
}

pub fn pass_by_option(arg: Option<QueryTAK>, sum: &mut u64) {
    let v = arg.unwrap_or_default();
    match v.eval {
        QueryTarget::BestMatching => *sum *= 3,
        QueryTarget::AllComplete => *sum *= 2,
        QueryTarget::All => (),
        QueryTarget::None => *sum *= 5,
        #[cfg(feature = "complete_n")]
        QueryTarget::Complete(_) => *sum *= 4,
    }
}

pub fn pass_by_ref(arg: &QueryTAK, sum: &mut u64) {
    match arg.eval {
        QueryTarget::BestMatching => *sum *= 3,
        QueryTarget::AllComplete => *sum *= 2,
        QueryTarget::All => (),
        QueryTarget::None => *sum *= 5,
        #[cfg(feature = "complete_n")]
        QueryTarget::Complete(_) => *sum *= 4,
    }
}

pub fn pass_by_val(arg: QueryTAK, sum: &mut u64) {
    match arg.eval {
        QueryTarget::BestMatching => *sum *= 3,
        QueryTarget::AllComplete => *sum *= 2,
        QueryTarget::All => (),
        QueryTarget::None => *sum *= 5,
        #[cfg(feature = "complete_n")]
        QueryTarget::Complete(_) => *sum *= 4,
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
            pass_by_ref(&QueryTAK::default(), &mut sum5);
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
            pass_by_val(QueryTAK::default(), &mut sum7);
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
