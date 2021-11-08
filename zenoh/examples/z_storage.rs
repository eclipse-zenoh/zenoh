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
#![recursion_limit = "256"]

use clap::{App, Arg};
use futures::prelude::*;
use futures::select;
use std::collections::HashMap;
use zenoh::config::Config;
use zenoh::prelude::*;
use zenoh::queryable::STORAGE;
use zenoh::utils::resource_name;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, selector) = parse_args();

    let mut stored: HashMap<String, Sample> = HashMap::new();

    println!("Open session");
    let session = zenoh::open(config).await.unwrap();

    println!("Register Subscriber on {}", selector);
    let mut subscriber = session.subscribe(&selector).await.unwrap();

    println!("Register Queryable on {}", selector);
    let mut queryable = session
        .register_queryable(&selector)
        .kind(STORAGE)
        .await
        .unwrap();

    let mut stdin = async_std::io::stdin();
    let mut input = [0_u8];
    loop {
        select!(
            sample = subscriber.receiver().next() => {
                let sample = sample.unwrap();
                println!(">> [Subscriber] Received {} ('{}': '{}')",
                    sample.kind, sample.res_name, String::from_utf8_lossy(&sample.value.payload.contiguous()));
                stored.insert(sample.res_name.clone(), sample);
            },

            query = queryable.receiver().next() => {
                let query = query.unwrap();
                println!(">> [Queryable ] Received Query '{}'", query.selector());
                for (stored_name, sample) in stored.iter() {
                    if resource_name::intersect(query.selector().key_selector, stored_name) {
                        query.reply(sample.clone());
                    }
                }
            },

            _ = stdin.read_exact(&mut input).fuse() => {
                if input[0] == b'q' {break} else {async_std::task::sleep(std::time::Duration::from_secs(1)).await}
            }
        );
    }
}

fn parse_args() -> (Config, String) {
    let args = App::new("zenoh storage example")
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
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .arg(
            Arg::from_usage("-s, --selector=[SELECTOR] 'The selection of resources to store'")
                .default_value("/demo/example/**"),
        )
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
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
    match args.value_of("mode").map(|m| m.parse()) {
        Some(Ok(mode)) => {
            config.set_mode(Some(mode)).unwrap();
        }
        Some(Err(())) => panic!("Invalid mode"),
        None => {}
    };
    if let Some(values) = args.values_of("peer") {
        config.peers.extend(values.map(|v| v.parse().unwrap()))
    }
    if let Some(values) = args.values_of("listeners") {
        config.listeners.extend(values.map(|v| v.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    let selector = args.value_of("selector").unwrap().to_string();

    (config, selector)
}
