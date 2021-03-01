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
use zenoh::net::RBuf;
use zenoh::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();
    let (config, size) = parse_args();

    let data: RBuf = (0usize..size)
        .map(|i| (i % 10) as u8)
        .collect::<Vec<u8>>()
        .into();

    println!("New zenoh...");
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    let path: Path = Path::try_from("/test/thr").unwrap();
    let value = Value::from(data);
    loop {
        workspace.put(&path, value.clone()).await.unwrap();
    }
}

fn parse_args() -> (Properties, usize) {
    let args = App::new("zenoh throughput put example")
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
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .arg(Arg::from_usage(
            "<PAYLOAD_SIZE>          'Sets the size of the payload to put'",
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

    let size = args
        .value_of("PAYLOAD_SIZE")
        .unwrap()
        .parse::<usize>()
        .unwrap();

    (config, size)
}
