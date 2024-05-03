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
use std::time::{Duration, Instant};

use clap::Parser;
use zenoh::prelude::*;
use zenoh_examples::CommonArgs;

fn main() {
    // Initiate logging
    zenoh_util::try_init_log_from_env();

    let (mut config, warmup, size, n) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_ping_shm` to operate
    // over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
    // subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
    config.transport.shared_memory.set_enabled(true).unwrap();

    let session = zenoh::open(config).wait().unwrap();

    // The key expression to publish data on
    let key_expr_ping = keyexpr::new("test/ping").unwrap();

    // The key expression to wait the response back
    let key_expr_pong = keyexpr::new("test/pong").unwrap();

    let sub = session.declare_subscriber(key_expr_pong).wait().unwrap();
    let publisher = session
        .declare_publisher(key_expr_ping)
        .congestion_control(CongestionControl::Block)
        .wait()
        .unwrap();

    let mut samples = Vec::with_capacity(n);

    // Construct an SHM backend
    let backend = {
        // NOTE: code in this block is a specific PosixSharedMemoryProviderBackend API.
        // The initialisation of SHM backend is completely backend-specific and user is free to do
        // anything reasonable here. This code is execuated at the provider's first use

        // Alignment for POSIX SHM provider
        // All allocations will be aligned corresponding to this alignment -
        // that means that the provider will be able to satisfy allocation layouts
        // with alignment <= provider_alignment
        let provider_alignment = AllocAlignment::default();

        // Create layout for POSIX Provider's memory
        let provider_layout = MemoryLayout::new(size, provider_alignment).unwrap();

        PosixSharedMemoryProviderBackend::builder()
            .with_layout(provider_layout)
            .res()
            .unwrap()
    };

    // Construct an SHM provider for particular backend and POSIX_PROTOCOL_ID
    let shared_memory_provider = SharedMemoryProviderBuilder::builder()
        .protocol_id::<POSIX_PROTOCOL_ID>()
        .backend(backend)
        .res();

    let buf = shared_memory_provider
        .alloc_layout()
        .size(size)
        .res()
        .unwrap()
        .alloc()
        .res()
        .unwrap();

    // convert ZSliceShmMut into ZSlice as ZSliceShmMut does not support Clone
    let buf: ZSlice = buf.into();

    // -- warmup --
    println!("Warming up for {warmup:?}...");
    let now = Instant::now();
    while now.elapsed() < warmup {
        publisher.put(buf.clone()).wait().unwrap();
        let _ = sub.recv().unwrap();
    }

    for _ in 0..n {
        let buf = buf.clone();
        let write_time = Instant::now();
        publisher.put(buf).wait().unwrap();

        let _ = sub.recv();
        let ts = write_time.elapsed().as_micros();
        samples.push(ts);
    }

    for (i, rtt) in samples.iter().enumerate().take(n) {
        println!(
            "{} bytes: seq={} rtt={:?}µs lat={:?}µs",
            size,
            i,
            rtt,
            rtt / 2
        );
    }
}

#[derive(Parser)]
struct Args {
    #[arg(short, long, default_value = "1")]
    /// The number of seconds to warm up (float)
    warmup: f64,
    #[arg(short = 'n', long, default_value = "100")]
    /// The number of round-trips to measure
    samples: usize,
    /// Sets the size of the payload to publish
    payload_size: usize,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, Duration, usize, usize) {
    let args = Args::parse();
    (
        args.common.into(),
        Duration::from_secs_f64(args.warmup),
        args.payload_size,
        args.samples,
    )
}
