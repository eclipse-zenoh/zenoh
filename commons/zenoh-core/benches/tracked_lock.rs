//
// Copyright (c) 2026 ZettaScale Technology
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
//! Cost of the re-entrancy counter on `zlock!` / `zread!` / `zwrite!`.
//!
//! Each pair benchmarks the same acquisition twice — once raw, once through the
//! instrumented macro — **inside one binary at one commit**. That is deliberate:
//! comparing two builds would leave codegen and machine noise as confounds for
//! an effect expected to be zero, and a zero cannot be distinguished from a
//! confound that happens to cancel. Here the only difference between the arms is
//! the wrapper.
//!
//! The claim under test:
//!
//! * `--release`: the arms must be indistinguishable. The counter is
//!   `debug_assertions`-gated, so it should compile to nothing; a real gap means
//!   the gating is wrong, not that the counter is expensive.
//! * debug: a gap is expected and acceptable. Report it, do not hide it.
//!
//! Uncontended by construction. A contended lock is dominated by the syscall,
//! which would mask exactly the effect being measured.

use std::sync::{Mutex, RwLock};

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use zenoh_core::{zlock, zread, zwrite};

fn benchmark(c: &mut Criterion) {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    let mutex = Mutex::new(0u64);
    let rwlock = RwLock::new(0u64);
    // Distinct instances for the nesting benchmark below: std::sync::RwLock
    // documents recursive read() on the SAME instance as unspecified ("might
    // panic or deadlock" if a writer queues in between) -- a realistic
    // nesting scenario in this codebase is a mutex held alongside several
    // DIFFERENT rwlocks, never the same rwlock re-entered on one thread.
    let rwlock_b = RwLock::new(0u64);
    let rwlock_c = RwLock::new(0u64);

    let mut group = c.benchmark_group(format!("uncontended/{profile}"));

    group.bench_function("mutex/raw", |b| {
        b.iter(|| {
            let mut g = mutex.lock().unwrap();
            *g = g.wrapping_add(1);
        })
    });
    group.bench_function("mutex/zlock", |b| {
        b.iter(|| {
            let mut g = zlock!(mutex);
            *g = g.wrapping_add(1);
        })
    });

    group.bench_function("rwlock/raw_read", |b| {
        b.iter(|| {
            let g = rwlock.read().unwrap();
            std::hint::black_box(*g)
        })
    });
    group.bench_function("rwlock/zread", |b| {
        b.iter(|| {
            let g = zread!(rwlock);
            std::hint::black_box(*g)
        })
    });

    group.bench_function("rwlock/raw_write", |b| {
        b.iter(|| {
            let mut g = rwlock.write().unwrap();
            *g = g.wrapping_add(1);
        })
    });
    group.bench_function("rwlock/zwrite", |b| {
        b.iter(|| {
            let mut g = zwrite!(rwlock);
            *g = g.wrapping_add(1);
        })
    });

    // Nesting is the shape the counter actually exists for, and the one where a
    // per-guard cost would compound.
    group.bench_function("nested4/raw", |b| {
        b.iter_batched(
            || (),
            |()| {
                let _a = mutex.lock().unwrap();
                let _b = rwlock.read().unwrap();
                let _c = rwlock_b.read().unwrap();
                let _d = rwlock_c.read().unwrap();
                std::hint::black_box(*_d)
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("nested4/macros", |b| {
        b.iter_batched(
            || (),
            |()| {
                let _a = zlock!(mutex);
                let _b = zread!(rwlock);
                let _c = zread!(rwlock_b);
                let _d = zread!(rwlock_c);
                std::hint::black_box(*_d)
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
