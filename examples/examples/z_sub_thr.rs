//
// Copyright (c) 2022 ZettaScale Technology
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
use std::time::Instant;
use zenoh::config::Config;
use zenoh::prelude::sync::SyncResolve;

fn main() {
    // initiate logging
    env_logger::init();

    let (config, m, n) = parse_args();

    let session = zenoh::open(config).res().unwrap();

    let key_expr = session.declare_expr("test/thr").res().unwrap();

    let mut count = 0;
    let mut start = Instant::now();

    let mut nm = 0;
    let _sub = session
        .subscribe(&key_expr)
        .callback_mut(move |_sample| {
            if count == 0 {
                start = Instant::now();
                count += 1;
            } else if count < n {
                count += 1;
            } else {
                print_stats(start, n);
                nm += 1;
                count = 0;
                if nm >= m {
                    std::process::exit(0)
                }
            }
        })
        .res()
        .unwrap();

    // Stop forever
    std::thread::park();
}

fn print_stats(start: Instant, n: usize) {
    let elapsed = start.elapsed().as_secs_f64();
    let thpt = (n as f64) / elapsed;
    println!("{} msg/s", thpt);
}

fn parse_args() -> (Config, usize, usize) {
    let args = App::new("zenoh throughput sub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
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
