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
use zenoh::net::Config;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let default_value = std::f64::consts::PI.to_string();

    let args = App::new("zenoh put example")
        .arg(Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
            .possible_values(&["peer", "client"]).default_value("peer"))
        .arg(Arg::from_usage("-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'"))
        .arg(Arg::from_usage("-p, --path=[PATH]        'The name of the resource to put.'")
            .default_value("/demo/example/zenoh-rs-put"))
        .arg(Arg::from_usage("-v, --value=[VALUE]      'The value of the resource to put.'")
            .default_value(&default_value))
        .get_matches();

    let config = Config::new(args.value_of("mode").unwrap()).unwrap()
        .add_peers(args.values_of("peer").map(|p| p.collect()).or_else(|| Some(vec![])).unwrap());
    let path    = args.value_of("path").unwrap();
    let value: f64   = args.value_of("value").unwrap().parse().unwrap();

    println!("New zenoh...");
    let zenoh = Zenoh::new(config, None).await.unwrap();
    
    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    println!("Put Float ('{}': '{}')...\n", path, value);
    workspace.put(&path.try_into().unwrap(), &FloatValue::from(value)).await.unwrap();

    zenoh.close().await.unwrap();
}
