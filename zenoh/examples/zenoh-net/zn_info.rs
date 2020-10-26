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
use zenoh::net::*;
use zenoh::Properties;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let mut config: Properties = parse_args();
    config.insert("user".to_string(), "user".to_string());
    config.insert("password".to_string(), "password".to_string());

    println!("Opening session...");
    let session = open(config.into()).await.unwrap();

    let info: Properties = session.info().await.into();
    for (key, value) in info.iter() {
        println!("{} : {}", key, value);
    }
}

fn parse_args() -> Properties {
    let args = App::new("zenoh-net info example")
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
        .get_matches();

    let mut config = Properties::default();
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }

    config
}
