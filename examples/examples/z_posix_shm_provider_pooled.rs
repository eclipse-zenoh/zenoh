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

//! Demonstrates [`PosixShmProviderBackend`] with the segment pool enabled.
//!
//! The pool reuses SHM segments across allocations, turning the hot path from
//! `shm_open + mmap` (~314 µs) into a single `ArrayQueue` CAS (~134 ns).
//!
//! # When to use `builder_with_pool`
//!
//! Use the pool when:
//! - you allocate and free fixed-size buffers at high rate (e.g. pub/sub throughput)
//! - profiling shows `__dquot_alloc_space` or `shm_open` in your hot path
//!
//! Leave the pool off (use `builder`) when:
//! - buffer sizes vary widely and a warm pool is unlikely
//! - you are memory-constrained and cannot afford idle segments
//!
//! # Running
//!
//! ```bash
//! cargo run --example z_posix_shm_provider_pooled --features unstable,shared-memory
//! ```

use zenoh::{
    shm::{PosixShmProviderBackend, ShmPoolConfig, ShmProviderBuilder},
    Wait,
};

fn main() {
    // Buffer size used throughout this example.
    let buf_size = 4096_usize;

    // ------------------------------------------------------------------
    // Build a backend with the segment pool enabled.
    //
    // `max_idle_per_size` caps how many idle segments of each size the pool
    // holds.  A value of 4 means up to 4 free 4096-byte segments are kept
    // warm; any beyond that are unmapped immediately on drop.
    // ------------------------------------------------------------------
    let pool_config = ShmPoolConfig {
        max_idle_per_size: 4,
    };
    let backend = PosixShmProviderBackend::builder_with_pool(buf_size, pool_config)
        .wait()
        .expect("failed to create pooled PosixShmProviderBackend");

    // Wrap in a provider (handles alignment, concurrency, and ZBytes integration).
    let provider = ShmProviderBuilder::backend(backend).wait();

    // ------------------------------------------------------------------
    // Warm path: allocate, use, drop — segment is returned to the pool.
    // The second and subsequent allocations reuse the pooled segment
    // instead of calling shm_open + mmap again.
    // ------------------------------------------------------------------
    for i in 0..8_u8 {
        let mut buf = provider
            .alloc(buf_size)
            .wait()
            .unwrap_or_else(|_| panic!("alloc {i} failed"));

        buf[0] = i;
        println!("alloc {i}: buf[0] = {}", buf[0]);

        // `buf` drops here — segment is returned to the pool.
    }

    println!("done");
}
