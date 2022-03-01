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
use async_std::task::sleep;
use clap::{App, Arg};
use futures::prelude::*;
use futures::select;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::net::protocol::io::SplitBuffer;
use zenoh_ext::*;

#[async_std::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    let (config, key_expr, query) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!(
        "Creating a QueryingSubscriber on {} with an initial query on {}",
        key_expr,
        query.as_ref().unwrap_or(&key_expr)
    );
    let mut subscriber = if let Some(selector) = query {
        session
            .subscribe_with_query(key_expr)
            .query_selector(&selector)
            .await
            .unwrap()
    } else {
        session.subscribe_with_query(key_expr).await.unwrap()
    };

    println!("Enter 'd' to issue the query again, or 'q' to quit...");
    let mut stdin = async_std::io::stdin();
    let mut input = [0_u8];
    loop {
        select!(
            sample = subscriber.next() => {
                let sample = sample.unwrap();
                println!(">> [Subscriber] Received {} ('{}': '{}')",
                    sample.kind, sample.key_expr.as_str(), String::from_utf8_lossy(&sample.value.payload.contiguous()));
            },

            _ = stdin.read_exact(&mut input).fuse() => {
                match input[0] {
                    b'q' => break,
                    b'd' => {
                        println!("Do query again");
                        subscriber.query().await.unwrap()
                    }
                    0 => sleep(Duration::from_secs(1)).await,
                    _ => (),
                }
            }
        );
    }
}

fn parse_args() -> (Config, String, Option<String>) {
    let args = App::new("zenoh-ext query sub example")
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
            Arg::from_usage("-k, --key=[KEYEXPR] 'The key expression to subscribe onto'")
                .default_value("/demo/example/**"),
        )
        .arg(
            Arg::from_usage("-q, --query=[SELECTOR] 'The selector to use for queries (by default it's same than 'selector' option)'"),
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
    if let Some(values) = args.values_of("listeners") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    let key_expr = args.value_of("key").unwrap().to_string();
    let query = args.value_of("query").map(ToString::to_string);

    (config, key_expr, query)
}
