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

const N: u128 = 100000;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let config = parse_args();

    println!("Opening session...");
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
    session
        .declare_callback_subscriber(&reskey, &sub_info, move |_sample| {
            if count == 0 {
                start = Instant::now();
                count += 1;
            } else if count < N {
                count += 1;
            } else {
                print_stats(start);
                count = 0;
            }
        })
        .await
        .unwrap();

    // Stop forever
    future::pending::<()>().await;

    // @TODO: Uncomment these once the writer starvation has been solved on the RwLock
    // session.undeclare_subscriber(sub).await.unwrap();
    // session.close().await.unwrap();
}

fn print_stats(start: Instant) {
    let elapsed = start.elapsed().as_secs_f64();
    let thpt = (N as f64) / elapsed;
    println!("{} msg/s", thpt);
}

fn parse_args() -> Config {
    let args = App::new("zenoh-net throughput sub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode.")
                .possible_values(&["peer", "client"])
                .default_value("peer"),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...   'Peer locators used to initiate the zenoh session.'",
        ))
        .get_matches();

    Config::default()
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
}
