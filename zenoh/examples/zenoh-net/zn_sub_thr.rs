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
use async_std::future;
use clap::{App, Arg};
use std::time::Instant;
use zenoh::net::ResKey::*;
use zenoh::net::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, m, n) = parse_args();

    let session = open(config, None).await.unwrap();

    let reskey = RId(session
        .declare_resource(&RName("/test/thr".to_string()))
        .await
        .unwrap());

    let mut count = 0u128;
    let mut start = Instant::now();

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };
    let mut nm = 0;
    let _sub = session
        .declare_callback_subscriber(&reskey, &sub_info, move |_sample| {
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
        .await
        .unwrap();

    // Stop forever
    future::pending::<()>().await;
}

fn print_stats(start: Instant, n: u128) {
    let elapsed = start.elapsed().as_secs_f64();
    let thpt = (n as f64) / elapsed;
    println!("{} msg/s", thpt);
}

fn parse_args() -> (Config, u32, u128) {
    let args = App::new("zenoh-net throughput sub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode.")
                .possible_values(&["peer", "client"])
                .default_value("peer"),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...   'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
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
        .get_matches();

    let s: u32 = args.value_of("samples").unwrap().parse().unwrap();
    let n: u128 = args.value_of("number").unwrap().parse().unwrap();
    let c = Config::default()
        .mode(
            args.value_of("mode")
                .map(|m| Config::parse_mode(m))
                .unwrap()
                .unwrap(),
        )
        .add_peers(
            args.values_of("peer")
                .map(|p| p.collect())
                .or_else(|| Some(vec![]))
                .unwrap(),
        )
        .add_listeners(
            args.values_of("listener")
                .map(|p| p.collect())
                .or_else(|| Some(vec![]))
                .unwrap(),
        );
    (c, s, n)
}
