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
use futures::prelude::*;
use std::convert::TryFrom;
use std::time::Instant;
use zenoh::net::Config;
use zenoh::*;

const N: u64 = 100000;

fn print_stats(start: Instant) {
    let elapsed = start.elapsed().as_secs_f64();
    let thpt = (N as f64) / elapsed;
    println!("{} msg/s", thpt);
}
//
// Argument parsing -- look at the main for the zenoh-related code
//
fn parse_args() -> Config {
    let args = App::new("zenoh throughput sub example")
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

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let config = parse_args();

    println!("New zenoh...");
    let zenoh = Zenoh::new(config, None).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    let path_expr = PathExpr::try_from("/test/thr").unwrap();

    let mut count = 0u64;
    let mut start = Instant::now();

    workspace
        .subscribe_with_callback(&path_expr, move |_change| {
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

    zenoh.close().await.unwrap();
}
