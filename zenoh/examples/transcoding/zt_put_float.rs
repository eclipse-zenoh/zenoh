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
use std::convert::TryFrom;
use zenoh::transcoding::*;
use zenoh::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, path, value) = parse_args();

    println!("New zenoh...");
    let session = open(config.into()).await.unwrap();

    println!("Put Float ('{}': '{}')...\n", path, value);
    session
        .put(&path.into(), Value::from(value).encode().1)
        .encoding(Value::from(value).encode().0)
        .await
        .unwrap();

    session.close().await.unwrap();
}

//
// Argument parsing -- look at the main for the zenoh-related code
//
fn parse_args() -> (Properties, String, f64) {
    let default_value = std::f64::consts::PI.to_string();

    let args = App::new("zenoh put float example")
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
            Arg::from_usage("-p, --path=[PATH]        'The name of the resource to put.'")
                .default_value("/demo/example/zenoh-rs-put"),
        )
        .arg(
            Arg::from_usage("-v, --value=[VALUE]      'The float value of the resource to put.'")
                .default_value(&default_value),
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

    let path = args.value_of("path").unwrap().to_string();
    let value: f64 = args.value_of("value").unwrap().parse().unwrap();

    (config, path, value)
}
