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
use futures::select;
use zenoh::prelude::*;
use zenoh_ext::*;

#[async_std::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    let (config, selector, query) = parse_args();

    println!("Open session");
    let session = zenoh::open(config).await.unwrap();

    println!(
        "Register a QueryingSubscriber on {} with an initial query on {}",
        selector,
        query.as_ref().unwrap_or(&selector)
    );
    let mut subscriber = if let Some(key_expr) = query {
        session
            .subscribe_with_query(selector)
            .query_selector(&key_expr)
            .await
            .unwrap()
    } else {
        session.subscribe_with_query(selector).await.unwrap()
    };

    println!("Enter 'd' to issue the query again, or 'q' to quit...");
    let mut stdin = async_std::io::stdin();
    let mut input = [0_u8];
    loop {
        select!(
            sample = subscriber.receiver().next() => {
                let sample = sample.unwrap();
                println!(">> [Subscriber] Received {} ('{}': '{}')",
                    sample.kind, sample.key_expr.as_str(), String::from_utf8_lossy(&sample.value.payload.to_vec()));
            },

            _ = stdin.read_exact(&mut input).fuse() => {
                if input[0] == b'q' { break }
                else if input[0] == b'd' {
                    println!("Do query again");
                    subscriber.query().await.unwrap()
                }
            }
        );
    }
}

fn parse_args() -> (Properties, String, Option<String>) {
    let args = App::new("zenoh-ext query sub example")
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
            Arg::from_usage("-s, --selector=[SELECTOR] 'The key expression to subscribe onto'")
                .default_value("/demo/example/**"),
        )
        .arg(
            Arg::from_usage("-q, --query=[SELECTOR] 'The selector to use for queries (by default it's same than 'selector' option)'"),
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
    let query = args.value_of("query").map(ToString::to_string);

    (config, selector, query)
}
