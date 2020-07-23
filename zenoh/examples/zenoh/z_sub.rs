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
#![feature(async_closure)]

use clap::{App, Arg};
use std::convert::TryInto;
use futures::prelude::*;
use zenoh::*;
use zenoh::net::Config;

//
// Argument parsing -- look at the main for the zenoh-related code
//
fn parse_args() -> (Config, String)  {
    let args = App::new("zenoh subscriber example")
        .arg(Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
            .possible_values(&["peer", "client"]).default_value("peer"))
        .arg(Arg::from_usage("-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'"))
        .arg(Arg::from_usage("-p, --path-expr=[PATH_EXPR] 'The selection of resources to get'")
            .default_value("/demo/example/**"))
        .get_matches();

    let config = Config::default()
        .mode(args.value_of("mode").map(|m| Config::parse_mode(m)).unwrap().unwrap())
        .add_peers(args.values_of("peer").map(|p| p.collect()).or_else(|| Some(vec![])).unwrap());
    let path_expr = args.value_of("path-expr").unwrap().to_string();

    (config, path_expr)
}

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, path_expr) = parse_args();

    println!("New zenoh...");
    let zenoh = Zenoh::new(config, None).await.unwrap();
    
    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    println!("Subscribe to {}'...\n", path_expr);
    let mut change_stream = workspace.subscribe(&path_expr.try_into().unwrap()).await.unwrap();
    while let Some(change) = change_stream.next().await {
        println!(">> [Subscription listener] received change {} : {:?}",
            change.path, change.value.unwrap())
    }

    zenoh.close().await.unwrap();
}
