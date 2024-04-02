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
use clap::Parser;
use std::time::Instant;
use zenoh::config::Config;
use zenoh::prelude::sync::*;
use zenoh_examples::CommonArgs;

struct Stats {
    round_count: usize,
    round_size: usize,
    finished_rounds: usize,
    round_start: Instant,
    global_start: Option<Instant>,
}
impl Stats {
    fn new(round_size: usize) -> Self {
        Stats {
            round_count: 0,
            round_size,
            finished_rounds: 0,
            round_start: Instant::now(),
            global_start: None,
        }
    }
    fn increment(&mut self) {
        if self.round_count == 0 {
            self.round_start = Instant::now();
            if self.global_start.is_none() {
                self.global_start = Some(self.round_start)
            }
            self.round_count += 1;
        } else if self.round_count < self.round_size {
            self.round_count += 1;
        } else {
            self.print_round();
            self.finished_rounds += 1;
            self.round_count = 0;
        }
    }
    fn print_round(&self) {
        let elapsed = self.round_start.elapsed().as_secs_f64();
        let throughtput = (self.round_size as f64) / elapsed;
        println!("{throughtput} msg/s");
    }
}
impl Drop for Stats {
    fn drop(&mut self) {
        let Some(global_start) = self.global_start else {
            return;
        };
        let elapsed = global_start.elapsed().as_secs_f64();
        let total = self.round_size * self.finished_rounds + self.round_count;
        let throughtput = total as f64 / elapsed;
        println!("Received {total} messages over {elapsed:.2}s: {throughtput}msg/s");
    }
}

fn main() {
    // initiate logging
    zenoh_util::init_log();

    let (mut config, m, n) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_pub_shm_thr` to operate
    // over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
    // subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
    config.transport.shared_memory.set_enabled(true).unwrap();

    let session = zenoh::open(config).res().unwrap();

    let key_expr = "test/thr";

    let mut stats = Stats::new(n);
    let _sub = session
        .declare_subscriber(key_expr)
        .callback_mut(move |_sample| {
            stats.increment();
            if stats.finished_rounds >= m {
                std::process::exit(0)
            }
        })
        .res()
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
