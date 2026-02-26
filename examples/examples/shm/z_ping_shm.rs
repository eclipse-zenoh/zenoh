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
use zenoh::{
    bytes::ZBytes, key_expr::keyexpr, qos::CongestionControl, shm::ShmProviderBuilder, Config, Wait,
};
use zenoh_examples::CommonArgs;

fn main() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, warmup, size, n) = parse_args();

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

    // Create SHM provider with default backend
    // NOTE: For extended PosixShmProviderBackend API please check z_posix_shm_provider.rs
    let provider = ShmProviderBuilder::default_backend(size).wait().unwrap();

    // Allocate an SHM buffer
    // NOTE: For allocation API please check z_alloc_shm.rs example
    // NOTE: For buf's API please check z_bytes_shm.rs example
    let buf = provider.alloc(size).wait().unwrap();

    // convert ZShmMut into ZBytes as ZShmMut does not support Clone
    let buf: ZBytes = buf.into();

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
