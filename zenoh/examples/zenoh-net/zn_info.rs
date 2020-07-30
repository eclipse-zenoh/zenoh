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

//
// Argument parsing -- look at the main for the zenoh-related code
//
fn parse_args() -> Config {
    let args = App::new("zenoh-net info example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
                .possible_values(&["peer", "client"])
                .default_value("peer"),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .get_matches();

    Config::default()
        .mode(
            args.value_of("mode")
                .map(|m| Config::parse_mode(m))
                .unwrap()
                .unwrap(),
        )
        .add_peers(
            args.values_of("peer")
                .map(|p| p.collect())
                .or_else(|| Some(vec![]))
                .unwrap(),
        )
}

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let config: Config = parse_args();

    let mut ps = Properties::new();
    ps.push((properties::ZN_USER_KEY, b"user".to_vec()));
    ps.push((properties::ZN_PASSWD_KEY, b"password".to_vec()));

    println!("Opening session...");
    let session = open(config, Some(ps)).await.unwrap();

    let info = session.info().await;
    for (key, value) in info {
        println!(
            "{} : {}",
            properties::to_str(key).unwrap(),
            hex::encode_upper(value)
        );
    }
}
