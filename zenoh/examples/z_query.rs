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
use zenoh::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, selector, target) = parse_args();

    println!("Opening session...");
    let session = open(config.into()).await.unwrap();

    println!("Sending Query '{}'...", selector);
    let mut replies = session.get(&selector.into()).target(target).await.unwrap();
    while let Some(reply) = replies.next().await {
        println!(
            ">> [Reply handler] received ('{}': '{}')",
            reply.data.res_name,
            String::from_utf8_lossy(&reply.data.payload.contiguous())
        )
    }
}

fn parse_args() -> (Properties, String, QueryTarget) {
    let args = App::new("zenoh-net query example")
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
            Arg::from_usage("-s, --selector=[SELECTOR] 'The selection of resources to query'")
                .default_value("/demo/example/**"),
        )
        .arg(
            Arg::from_usage("-k, --kind=[KIND] 'The KIND of queryables to query'")
                .possible_values(&["ALL_KINDS", "STORAGE", "EVAL"])
                .default_value("ALL_KINDS"),
        )
        .arg(
            Arg::from_usage("-t, --target=[TARGET] 'The target queryables of the query'")
                .possible_values(&["ALL", "BEST_MATCHING", "ALL_COMPLETE", "NONE"])
                .default_value("ALL"),
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

    let kind = match args.value_of("kind") {
        Some("STORAGE") => queryable::STORAGE,
        Some("EVAL") => queryable::EVAL,
        _ => queryable::ALL_KINDS,
    };

    let target = match args.value_of("target") {
        Some("BEST_MATCHING") => Target::BestMatching,
        Some("ALL_COMPLETE") => Target::AllComplete,
        Some("NONE") => Target::None,
        _ => Target::All,
    };

    (config, selector, QueryTarget { kind, target })
}
