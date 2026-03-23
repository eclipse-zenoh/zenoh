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

//! Benchmark: SHM segment pool reuse vs. cold allocation.
//!
//! # What is measured
//!
//! `alloc/cold` — cost of creating a fresh 1 MB SHM segment:
//!   `shm_open + ftruncate + mmap + mlock` (+ `munmap + shm_unlink` on drop).
//!   This is the per-message cost WITHOUT pool reuse — paid on every alloc
//!   when the pool is disabled or on a pool miss (new size / pool full).
//!
//! `alloc/warm` — cost of a pool hit after warmup:
//!   `ArrayQueue::pop` on alloc, `ArrayQueue::push` on drop.
//!   No kernel calls on the hot path.
//!
//! # Interpreting results
//!
//! The ratio cold/warm shows how much kernel overhead the pool amortizes.
//! For 1 MB at 100 Hz with N streams, the cold path would impose:
//!   N × 100 × cold_cost µs of mmap overhead per second.
//! With the pool that collapses to N × 100 × warm_cost, paid in CAS cycles only.
//!
//! System-level latency impact (ping-pong benchmarks at 100 Hz / 1 MB):
//!   1 stream: p50 ≈ 2.6 ms → pool has no visible effect (mmap amortised at low rate)
//!   4 streams: p50 ≈ 4.5 ms → ~16% improvement (contention path exposed)
//!
//! Run with:
//!   cargo bench --bench shm_pool -p zenoh-shm

use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zenoh_core::Wait;
use zenoh_shm::api::{
    protocol_implementations::posix::posix_shm_provider_backend_talc::{
        PosixShmProviderBackendTalc, ShmPoolConfig,
    },
    provider::{
        memory_layout::MemoryLayout, shm_provider_backend::ShmProviderBackend,
        types::AllocAlignment,
    },
};

const PAYLOAD_SIZE: usize = 1024 * 1024; // 1 MB

fn make_layout() -> MemoryLayout {
    MemoryLayout::new(PAYLOAD_SIZE, AllocAlignment::default()).unwrap()
}

/// Cold path: a new SHM segment is created for each alloc.
///
/// Uses a fresh pool backend per iteration so the pool is always empty and
/// `alloc` falls through to `PosixShmSegment::create` (shm_open + mmap).
fn bench_cold(c: &mut Criterion) {
    let layout = make_layout();
    let mut group = c.benchmark_group("alloc");
    group.throughput(Throughput::Elements(1));

    group.bench_function(BenchmarkId::new("cold", "1MB"), |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                // Fresh backend every iter → pool always starts empty → alloc is cold.
                let backend = PosixShmProviderBackendTalc::builder_with_pool(
                    PAYLOAD_SIZE,
                    ShmPoolConfig::default(),
                )
                .wait()
                .expect("backend create failed");
                let start = Instant::now();
                let chunk = backend.alloc(&layout).expect("alloc failed");
                drop(chunk);
                total += start.elapsed();
                // backend drops here, cleaning up the backing segment
            }
            total
        });
    });

    group.finish();
}

/// Warm path: pool holds a cached segment; alloc = one ArrayQueue::pop CAS,
/// free = one ArrayQueue::push CAS. No syscalls.
fn bench_warm(c: &mut Criterion) {
    let backend =
        PosixShmProviderBackendTalc::builder_with_pool(PAYLOAD_SIZE, ShmPoolConfig::default())
            .wait()
            .expect("failed to create pool backend");
    let layout = make_layout();

    // Warm the pool: first alloc creates the segment; drop returns it to the queue.
    drop(backend.alloc(&layout).expect("warmup alloc failed"));

    let mut group = c.benchmark_group("alloc");
    group.throughput(Throughput::Elements(1));

    group.bench_function(BenchmarkId::new("warm", "1MB"), |b| {
        b.iter(|| {
            let chunk = backend.alloc(&layout).expect("alloc failed");
            drop(chunk); // returns segment to pool → available for next iter
        });
    });

    group.finish();
}

criterion_group!(benches, bench_cold, bench_warm);
criterion_main!(benches);
