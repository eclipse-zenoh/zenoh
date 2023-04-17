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
use std::convert::TryInto;
use zenoh::config::Config;
use zenoh::prelude::sync::*;
use zenoh::publication::CongestionControl;

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

fn parse_args() -> (Config, usize, Priority, bool, usize) {
    let args = App::new("zenoh throughput pub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-p, --priority=[PRIO]...  'Priority for sending data.'",
        ))
        .arg(Arg::from_usage(
            "-e, --connect=[ENDPOINT]...  'Endpoints to connect to.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listen=[ENDPOINT]...   'Endpoints to listen on.'",
        ))
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage("-t, --print   'Print the statistics.'"))
        .arg(
            Arg::from_usage(
                "-n, --number=[number] 'Number of messages in each throughput measurements.'",
            )
            .default_value("100000"),
        )
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .arg(Arg::from_usage(
            "<PAYLOAD_SIZE>          'Sets the size of the payload to publish'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Config::from_file(conf_file).unwrap()
    } else {
        Config::default()
    };

    let mut prio = Priority::default();
    if let Some(p) = args.value_of("priority") {
        prio = p.parse::<u8>().unwrap().try_into().unwrap();
    }
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

    let number: usize = args.value_of("number").unwrap().parse().unwrap();

    let size = args
        .value_of("PAYLOAD_SIZE")
        .unwrap()
        .parse::<usize>()
        .unwrap();

    (config, size, prio, args.is_present("print"), number)
}
