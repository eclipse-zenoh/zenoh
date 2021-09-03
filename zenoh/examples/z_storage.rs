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
use zenoh::queryable::STORAGE;
use zenoh::utils::resource_name;
use zenoh::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, selector) = parse_args();

    let mut stored: HashMap<String, Sample> = HashMap::new();

    println!("Opening session...");
    let session = open(config).await.unwrap();

    println!("Declaring Subscriber on {}", selector);
    let mut subscriber = session.subscribe(&selector).await.unwrap();

    println!("Declaring Queryable on {}", selector);
    let mut queryable = session
        .register_queryable(&selector)
        .kind(STORAGE)
        .await
        .unwrap();

    let mut stdin = async_std::io::stdin();
    let mut input = [0u8];
    loop {
        select!(
            sample = subscriber.receiver().next().fuse() => {
                let sample = sample.unwrap();
                println!(">> [Subscription listener] Received ('{}': '{}')",
                    sample.res_name, String::from_utf8_lossy(&sample.value.payload.contiguous()));
                stored.insert(sample.res_name.clone(), sample);
            },

            query = queryable.receiver().next().fuse() => {
                let query = query.unwrap();
                println!(">> [Query handler        ] Handling '{}'", query.selector());
                for (stored_name, sample) in stored.iter() {
                    if resource_name::intersect(query.selector().res_name, stored_name) {
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

fn parse_args() -> (Properties, String) {
    let args = App::new("zenoh-net storage example")
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
        Properties::from(std::fs::read_to_string(conf_file).unwrap())
    } else {
        Properties::default()
    };
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }
    if args.is_present("no-multicast-scouting") {
        config.insert("multicast_scouting".to_string(), "false".to_string());
    }

    let selector = args.value_of("selector").unwrap().to_string();

    (config, selector)
}
