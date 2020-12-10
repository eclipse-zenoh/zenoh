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
use std::convert::TryInto;
use zenoh::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, path, value) = parse_args();

    println!("New zenoh...");
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    println!("Put Float ('{}': '{}')...\n", path, value);
    workspace
        .put(&path.try_into().unwrap(), value.into())
        .await
        .unwrap();

    zenoh.close().await.unwrap();
}

//
// Argument parsing -- look at the main for the zenoh-related code
//
fn parse_args() -> (Properties, String, f64) {
    let default_value = std::f64::consts::PI.to_string();

    let args = App::new("zenoh put float example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
                .possible_values(&["peer", "client"])
                .default_value("peer"),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
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
            "--no-scouting 'Disable the scouting mechanism.'",
        ))
        .get_matches();

    let mut config = Properties::default();
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }
    if args.is_present("no-scouting") {
        config.insert("multicast_scouting".to_string(), "false".to_string());
    }

    let path = args.value_of("path").unwrap().to_string();
    let value: f64 = args.value_of("value").unwrap().parse().unwrap();

    (config, path, value)
}
