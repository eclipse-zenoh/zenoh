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

use std::{convert::TryInto, time::Instant};

use clap::Parser;
use zenoh::{
    bytes::ZBytes, qos::{CongestionControl, Priority}, sample::Locality, Wait
};
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
        let throughput = (self.round_size as f64) / elapsed;
        println!("{throughput} msg/s");
    }
}
impl Drop for Stats {
    fn drop(&mut self) {
        let Some(global_start) = self.global_start else {
            return;
        };
        let elapsed = global_start.elapsed().as_secs_f64();
        let total = self.round_size * self.finished_rounds + self.round_count;
        let throughput = total as f64 / elapsed;
        println!("Received {total} messages over {elapsed:.2}s: {throughput}msg/s");
    }
}

fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");
    let args = Args::parse();

    let session = zenoh::open(args.common).wait().unwrap();

    let key_expr = "test/thr";

    let mut stats = Stats::new(args.number);
    session
        .declare_subscriber(key_expr)
        .callback_mut(move |_sample| {
            stats.increment();
            if stats.finished_rounds >= args.samples {
                std::process::exit(0)
            }
        })
        .background()
        .wait()
        .unwrap();
    
    let mut prio = Priority::DEFAULT;
    if let Some(p) = args.priority {
        prio = p.try_into().unwrap();
    }

    let publisher = session
        .declare_publisher(key_expr)
        .congestion_control(CongestionControl::Block)
        .priority(prio)
        .express(args.express)
        .allowed_destination(args.allowed_destination)
        .wait()
        .unwrap();

    println!("Press CTRL-C to quit...");
    let payload_size = args.payload_size;
    let data: ZBytes = (0..payload_size)
        .map(|i| (i % 10) as u8)
        .collect::<Vec<u8>>()
        .into();
    let mut count: usize = 0;
    let mut start = std::time::Instant::now();
    loop {
        publisher.put(data.clone()).wait().unwrap();

        if args.print {
            if count < args.number {
                count += 1;
            } else {
                let thpt = count as f64 / start.elapsed().as_secs_f64();
                println!("{thpt} msg/s");
                count = 0;
                start = std::time::Instant::now();
            }
        }
    }
}

#[derive(Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "10")]
    /// Number of throughput measurements.
    samples: usize,
    /// express for sending data
    #[arg(long, default_value = "false")]
    express: bool,
    /// Priority for sending data
    #[arg(short, long)]
    priority: Option<u8>,
    /// Print the statistics
    #[arg(short = 't', long)]
    print: bool,
    /// Number of messages in each throughput measurements
    #[arg(short, long, default_value = "10000000")]
    number: usize,
    #[arg(short, long, default_value = "any")]
    allowed_destination: Locality,
    /// Sets the size of the payload to publish
    #[arg(long, default_value = "8")]
    payload_size: usize,
    #[command(flatten)]
    common: CommonArgs,
}
