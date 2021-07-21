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
use std::convert::{TryFrom, TryInto};
use zenoh::transcoding::ChangeReceiver;
use zenoh::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, selector) = parse_args();

    println!("New zenoh...");
    let session = open(config.into()).await.unwrap();

    println!("Subscribe to {}'...\n", selector);
    let mut change_stream: ChangeReceiver = session
        .subscribe(&selector.try_into().unwrap())
        .await
        .unwrap()
        .into();

    let mut stdin = async_std::io::stdin();
    let mut input = [0u8];
    loop {
        select!(
            change = change_stream.next().fuse() => {
                let change = change.unwrap();
                println!(
                    ">> [Subscription listener] received {:?} for {} : {:?} with timestamp {}",
                    change.kind,
                    change.path,
                    change.value,
                    change.timestamp
                )
            }

            _ = stdin.read_exact(&mut input).fuse() => {
                if input[0] == b'q' {break}
            }
        );
    }

    change_stream.close().await.unwrap();
    session.close().await.unwrap();
}

fn parse_args() -> (Properties, String) {
    let args = App::new("zenoh subscriber example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(
            Arg::from_usage("-s, --selector=[selector] 'The selection of resources to subscribe'")
                .default_value("/demo/example/**"),
        )
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Properties::try_from(std::path::Path::new(conf_file)).unwrap()
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
