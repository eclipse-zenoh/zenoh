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
use clap::{App, Arg};
use std::io::{stdin, Read};
use std::time::Instant;
use zenoh::config::Config;
use zenoh::prelude::sync::*;

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
        let elapsed = self.global_start.unwrap().elapsed().as_secs_f64();
        let total = self.round_size * self.finished_rounds + self.round_count;
        let throughtput = total as f64 / elapsed;
        println!("Received {total} messages over {elapsed:.2}s: {throughtput}msg/s");
    }
}

fn main() {
    // initiate logging
    env_logger::init();

    let (config, m, n) = parse_args();

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

    for byte in stdin().bytes() {
        match byte {
            Ok(b'q') => break,
            _ => std::thread::yield_now(),
        }
    }
}

fn parse_args() -> (Config, usize, usize) {
    let args = App::new("zenoh throughput sub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode (peer by default).")
                .possible_values(["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --connect=[ENDPOINT]...   'Endpoints to connect to.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listen=[ENDPOINT]...   'Endpoints to listen on.'",
        ))
        .arg(
            Arg::from_usage("-s, --samples=[number] 'Number of throughput measurements.'")
                .default_value("10"),
        )
        .arg(
            Arg::from_usage(
                "-n, --number=[number] 'Number of messages in each throughput measurements.'",
            )
            .default_value("100000"),
        )
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Config::from_file(conf_file).unwrap()
    } else {
        Config::default()
    };
    if let Some(Ok(mode)) = args.value_of("mode").map(|mode| mode.parse()) {
        config.set_mode(Some(mode)).unwrap();
    }
    if let Some(values) = args.values_of("connect") {
        config
            .connect
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if let Some(values) = args.values_of("listen") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    let samples: usize = args.value_of("samples").unwrap().parse().unwrap();
    let number: usize = args.value_of("number").unwrap().parse().unwrap();

    (config, samples, number)
}
