//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use clap::{App, Arg};
use std::time::Instant;
use zenoh::prelude::*;
use zenoh::publisher::CongestionControl;

fn main() {
    // initiate logging
    env_logger::init();

    let (config, size, n) = parse_args();
    let session = zenoh::open(config).wait().unwrap();

    // The key expression to publish data on
    let key_expr_ping = session.register_expr("/test/ping").wait().unwrap();

    // The key expression to wait the response back
    let key_expr_pong = session.register_expr("/test/pong").wait().unwrap();

    let mut sub = session.subscribe(&key_expr_pong).wait().unwrap();

    let data: Value = (0usize..size)
        .map(|i| (i % 10) as u8)
        .collect::<Vec<u8>>()
        .into();

    let mut samples = Vec::with_capacity(n);

    // -- warmup --
    let wun = 1000;
    let stream = sub.receiver();
    for _ in 0..wun {
        let data = data.clone();
        session
            .put(&key_expr_ping, data)
            // Make sure to not drop messages because of congestion control
            .congestion_control(CongestionControl::Block)
            .wait()
            .unwrap();

        let _ = stream.recv();
    }

    for _ in 0..n {
        let data = data.clone();
        let write_time = Instant::now();
        session
            .put(&key_expr_ping, data)
            // Make sure to not drop messages because of congestion control
            .congestion_control(CongestionControl::Block)
            .wait()
            .unwrap();

        let _ = stream.recv();
        let ts = write_time.elapsed().as_micros();
        samples.push(ts);
    }

    for (i, rtt) in samples.iter().enumerate().take(n) {
        println!("{} bytes: seq={} time={:?}µs", size, i, rtt);
    }
}

fn parse_args() -> (Properties, usize, usize) {
    let args = App::new("zenoh roundtrip ping example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...   'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(
            Arg::from_usage("-n, --samples=[N]          'The number of round-trips to measure'")
                .default_value("100"),
        )
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .arg(Arg::from_usage(
            "<PAYLOAD_SIZE>          'Sets the size of the payload to publish'",
        ))
        .get_matches();

    let mut config = Properties::default();
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }
    if args.is_present("no-multicast-scouting") {
        config.insert("multicast_scouting".to_string(), "false".to_string());
    }
    let n: usize = args.value_of("samples").unwrap().parse().unwrap();
    let size: usize = args.value_of("PAYLOAD_SIZE").unwrap().parse().unwrap();

    (config, size, n)
}
