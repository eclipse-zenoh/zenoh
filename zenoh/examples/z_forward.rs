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
use zenoh::config::Config;

#[async_std::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    let (config, key_expr, forward) = parse_args();

    println!("Openning session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Creating Subscriber on '{}'...", key_expr);
    let mut subscriber = session.subscribe(&key_expr).await.unwrap();
    println!("Creating Publisher on '{}'...", forward);
    let publisher = session.publish(&forward).await.unwrap();
    println!("Forwarding data from '{}' to '{}'...", key_expr, forward);
    subscriber.forward(publisher).await.unwrap();
}

fn parse_args() -> (Config, String, String) {
    let args = App::new("zenoh sub example")
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
            Arg::from_usage("-k, --key=[KEYEXPR] 'The key expression to subscribe to.'")
                .default_value("/demo/example/**"),
        )
        .arg(
            Arg::from_usage("-f, --forward=[KEYEXPR] 'The key expression to forward to.'")
                .default_value("/demo/forward"),
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
    if let Some(values) = args.values_of("peer") {
        config.peers.extend(values.map(|v| v.parse().unwrap()))
    }
    if let Some(values) = args.values_of("listeners") {
        config.listeners.extend(values.map(|v| v.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    let key_expr = args.value_of("key").unwrap().to_string();

    let forward = args.value_of("forward").unwrap().to_string();

    (config, key_expr, forward)
}
