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

//! Benchmark: Notifier/Waiter notification round-trip.
//!
//! Measures two paths through the synchronisation primitive:
//!
//! `notify_wait/sticky` — producer calls `notify()` before the consumer calls
//!   `wait()`. The flag is already set so `wait()` returns without blocking.
//!   This is the common case in the transport pipeline when the batch is ready
//!   before the tx thread checks. Hot path: lock + flag check + unlock (no
//!   condvar sleep).
//!
//! `notify_wait/cross_thread` — a dedicated notifier thread signals the
//!   measured thread. `wait()` actually blocks on the condvar until the signal
//!   arrives. Measures true cross-thread wakeup latency.
//!
//! # Regression baseline (event_listener, before this PR)
//!
//! Profiling a zenoh SHM publisher at 1 MB / 100 Hz showed `event_listener` taking
//! **12.18% CPU** in `drop_in_place::<EventListener>` on the sender thread —
//! heap-allocating and dropping a linked-list node on *every* batch send cycle.
//!
//! Expected results after this PR (Mutex+Condvar):
//!   sticky:       ~20–50 ns   (lock + flag + unlock, no allocation)
//!   cross_thread: ~2–10 µs    (futex wakeup round-trip)
//!
//! Run with:
//!   cargo bench --bench notify_wait -p zenoh-sync

use std::{
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zenoh_sync::event;

/// Sticky path: notify() fires before wait() — no blocking, just flag check.
///
/// Represents the hot path when the batch is already ready when the tx thread
/// wakes up. Before this PR: EventListener allocated on the heap every cycle.
/// After: lock + u8 check + unlock.
fn bench_sticky(c: &mut Criterion) {
    let mut group = c.benchmark_group("notify_wait");
    group.throughput(Throughput::Elements(1));

    group.bench_function(BenchmarkId::new("sticky", ""), |b| {
        let (notifier, waiter) = event::new();
        b.iter(|| {
            notifier.notify().unwrap();
            waiter.wait().unwrap();
        });
    });

    group.finish();
}

/// Cross-thread path: waiter blocks until a dedicated notifier thread fires.
///
/// Measures the full condvar wakeup round-trip between two threads. Uses
/// a Barrier to ensure the waiter is sleeping before notify() fires, giving
/// a clean measurement of the unpark/futex latency.
fn bench_cross_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("notify_wait");
    group.throughput(Throughput::Elements(1));

    group.bench_function(BenchmarkId::new("cross_thread", ""), |b| {
        b.iter_custom(|iters| {
            // Barrier: ensures waiter is blocked before notifier fires.
            let barrier = Arc::new(Barrier::new(2));

            let (notifier, waiter) = event::new();
            let notifier = Arc::new(notifier);

            let notifier_thread = {
                let notifier = Arc::clone(&notifier);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    for _ in 0..iters {
                        barrier.wait(); // wait until waiter is about to block
                        notifier.notify().unwrap();
                    }
                })
            };

            let mut total = Duration::ZERO;
            for _ in 0..iters {
                barrier.wait(); // signal notifier thread we're ready
                let start = Instant::now();
                waiter.wait().unwrap();
                total += start.elapsed();
            }

            notifier_thread.join().unwrap();
            total
        });
    });

    group.finish();
}

criterion_group!(benches, bench_sticky, bench_cross_thread);
criterion_main!(benches);
