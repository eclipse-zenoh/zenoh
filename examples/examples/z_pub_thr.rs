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
use std::convert::TryInto;
use zenoh::config::Config;
use zenoh::prelude::sync::*;
use zenoh::publication::CongestionControl;
use zenoh_examples::CommonArgs;

fn main() {
    // initiate logging
    env_logger::init();
    let (config, size, prio, print, number) = parse_args();

    let data: Value = (0usize..size)
        .map(|i| (i % 10) as u8)
        .collect::<Vec<u8>>()
        .into();

    let session = zenoh::open(config).res().unwrap();

    let publisher = session
        .declare_publisher("test/thr")
        .congestion_control(CongestionControl::Block)
        .priority(prio)
        .res()
        .unwrap();

    let mut count: usize = 0;
    let mut start = std::time::Instant::now();
    loop {
        publisher.put(data.clone()).res().unwrap();

        if print {
            if count < number {
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
    #[arg(short, long)]
    /// Priority for sending data
    priority: Option<u8>,
    #[arg(short = 't', long)]
    /// Print the statistics
    print: bool,
    #[arg(short, long, default_value = "100000")]
    /// Number of messages in each throughput measurements
    number: usize,
    /// Sets the size of the payload to publish
    payload_size: usize,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, usize, Priority, bool, usize) {
    let args = Args::parse();

    let mut prio = Priority::default();
    if let Some(p) = args.priority {
        prio = p.try_into().unwrap();
    }

    (
        args.common.into(),
        args.payload_size,
        prio,
        args.print,
        args.number,
    )
}
