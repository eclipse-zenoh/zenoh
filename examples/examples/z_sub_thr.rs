use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
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
use std::time::Instant;

use clap::Parser;
use zenoh::{Config, Wait};
use zenoh_examples::CommonArgs;

struct Start {
    round: Instant,
    global: Option<Instant>,
}

struct Stats {
    received: AtomicU64,
    round_size: u64,
    start: Mutex<Start>,
}
impl Stats {
    fn new(round_size: usize) -> Self {
        Stats {
            received: AtomicU64::new(0),
            round_size: round_size as u64,
            start: Mutex::new(Start {
                round: Instant::now(),
                global: None,
            }),
        }
    }
    fn increment(&self) -> usize {
        let prev_received = self.received.fetch_add(1, Ordering::Relaxed);
        if prev_received % self.round_size == 0 {
            let mut start = self.start.lock().unwrap();
            let now = Instant::now();
            if prev_received == 0 {
                start.global = Some(now);
            } else {
                let elapsed = now.duration_since(start.round).as_secs_f64();
                let throughput = (self.round_size as f64) / elapsed;
                println!("{throughput} msg/s");
            }
            start.round = now;
        }
        (prev_received / self.round_size) as usize
    }
}
impl Drop for Stats {
    fn drop(&mut self) {
        let start = self.start.get_mut().unwrap();
        let Some(global_start) = start.global else {
            return;
        };
        let elapsed = global_start.elapsed().as_secs_f64();
        let total = *self.received.get_mut();
        let throughput = total as f64 / elapsed;
        println!("Received {total} messages over {elapsed:.2}s: {throughput}msg/s");
    }
}

fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, m, n) = parse_args();

    let session = zenoh::open(config).wait().unwrap();

    let key_expr = "test/thr";

    let stats = Stats::new(n);
    let thread = std::thread::current();
    session
        .declare_subscriber(key_expr)
        .callback(move |_sample| {
            if stats.increment() >= m {
                thread.unpark();
            }
        })
        .background()
        .wait()
        .unwrap();

    println!("Press CTRL-C to quit...");
    std::thread::park();
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "10")]
    /// Number of throughput measurements.
    samples: usize,
    #[arg(short, long, default_value = "100000")]
    /// Number of messages in each throughput measurements.
    number: usize,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, usize, usize) {
    let args = Args::parse();
    (args.common.into(), args.samples, args.number)
}
